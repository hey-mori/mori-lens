//! macOS product entry point.
//!
//! The full product loop on macOS:
//!
//! ```text
//! pointer settles → AX acquisition → local translation → native card
//! ```
//!
//! The hover state machine, segmentation, language routing, cache, and
//! translation are all platform-neutral (lens-core). The platform-specific
//! pieces are the pointer sampler, the text acquirer, the overlay, and the
//! event loop.
//!
//! Included from `main.rs` behind `#[cfg(target_os = "macos")]`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::cache::{CacheKey, TranslationCache};
use lens_core::card::Card;
use lens_core::geometry::ScreenPoint;
use lens_core::hover::{HoverConfig, HoverEvent, HoverMachine, PointerSample};
use lens_core::language::decide;
use lens_core::settings::{self, Problem, Settings};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_macos::{overlay, AxPermission, MacTextAcquirer, Overlay, SingleInstance};
use lens_translate::OllamaTranslator;

const USAGE: &str = "lens · Mori Lens (macOS)

USAGE
    lens [OPTIONS]

    Rest the pointer on a sentence. A card appears with the translation.

OPTIONS
    --settings <path>   Use this settings file.
    --language <name>   Target language. Default Korean.
    --endpoint <addr>   Where Ollama listens. Default 127.0.0.1:11434.
    --model <name>      Ollama model name.
    --dwell <ms>        How long the pointer must rest. Default 320.
    --auto              Translate without holding a modifier.
    -V, --version       Print the version.
    -h, --help          This text.
";

/// How often to sample the pointer position.
const POINTER_POLL_MS: u64 = 25;

/// How many translation cache entries to keep.
const CACHE_ENTRIES: usize = 256;

fn default_settings_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Mori Lens")
        .join("settings.conf")
}

fn load_settings(path: &std::path::Path) -> (Settings, &'static str, Vec<Problem>, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let (settings, problems) = Settings::parse(&text);
            (settings, "read", problems, false)
        }
        Err(_) => {
            let wrote = path
                .parent()
                .map(|p| std::fs::create_dir_all(p).is_ok())
                .unwrap_or(false)
                && std::fs::write(path, settings::TEMPLATE).is_ok();
            (
                Settings::default(),
                if wrote {
                    "written with the defaults, edit it and restart"
                } else {
                    "not readable and not writable, so this run is the defaults"
                },
                Vec::new(),
                wrote,
            )
        }
    }
}

#[derive(Default)]
struct Flags {
    settings: Option<std::path::PathBuf>,
    auto: bool,
    language: Option<String>,
    model: Option<String>,
    endpoint: Option<String>,
    dwell_ms: Option<u64>,
}

fn parse_flags() -> Result<Flags, String> {
    let mut flags = Flags::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("Mori Lens {} · macOS", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--auto" => flags.auto = true,
            "--settings" => flags.settings = args.next().map(std::path::PathBuf::from),
            "--language" => flags.language = args.next(),
            "--model" => flags.model = args.next(),
            "--endpoint" => flags.endpoint = args.next(),
            "--dwell" => {
                let ms = args
                    .next()
                    .ok_or("--dwell needs milliseconds")?
                    .parse()
                    .map_err(|_| "--dwell needs milliseconds")?;
                flags.dwell_ms = Some(ms);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(flags)
}

/// Get the current mouse position in Lens coordinates (physical pixels,
/// top-left origin) using the native bridge.
fn mouse_position() -> ScreenPoint {
    let (x, y) = match overlay::mouse_position() {
        Some(pos) => pos,
        None => return ScreenPoint::new(0, 0),
    };
    // x, y are in logical points (top-left origin). Convert to physical
    // pixels using the display scale at that point.
    let scale = overlay::display_scale_at(x, y);
    ScreenPoint::new((x * scale).round() as i32, (y * scale).round() as i32)
}

pub fn main() -> std::process::ExitCode {
    let flags = match parse_flags() {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{msg}\n\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };

    // Single instance: prevent two Lens processes from racing.
    let _only_instance = match SingleInstance::claim() {
        Some(guard) => guard,
        None => {
            eprintln!("Mori Lens is already running.");
            return std::process::ExitCode::from(1);
        }
    };

    // Settings.
    let path = flags.settings.unwrap_or_else(default_settings_path);
    let (settings, state, problems, first_run) = load_settings(&path);
    let language = flags.language.unwrap_or(settings.language);
    let endpoint = flags.endpoint.unwrap_or(settings.endpoint);
    let model = flags.model.or(settings.model);

    println!("Mori Lens · macOS");
    if first_run {
        println!("  first     this is the first run");
    }
    println!("  settings  {} · {}", path.display(), state);
    for problem in &problems {
        println!("            line {}: {}", problem.line, problem.message);
    }
    println!("  language  {language}");
    println!("  endpoint  {endpoint}");

    // AX permission.
    match AxPermission::check_with_prompt(true) {
        AxPermission::Trusted => println!("  ax        trusted"),
        AxPermission::NotTrusted => {
            println!("  ax        NOT TRUSTED");
            println!("            open System Settings > Privacy & Security > Accessibility");
            return std::process::ExitCode::from(1);
        }
    }

    // Translator.
    let mut translator = if let Some(ref m) = model {
        OllamaTranslator::new(&endpoint, m)
    } else {
        OllamaTranslator::choosing(&endpoint)
    };
    match translator.resolve_model() {
        Ok(m) => println!("  model     {m}"),
        Err(e) => {
            println!(
                "  model     {} · {}",
                e.user_message(),
                e.remedy()
                    .unwrap_or_else(|| "install Ollama and pull a model".to_owned())
            );
        }
    }

    // Overlay.
    let overlay = match Overlay::start() {
        Some(o) => {
            println!("  overlay   ready");
            o
        }
        None => {
            println!("  overlay   FAILED to create");
            return std::process::ExitCode::from(1);
        }
    };

    // Acquirer.
    let acquirer = MacTextAcquirer::new();
    println!("  acquire   MacTextAcquirer ready");

    // Hover machine.
    let dwell = Duration::from_millis(flags.dwell_ms.unwrap_or(settings.dwell_ms));
    let hover_config = HoverConfig {
        dwell,
        ..HoverConfig::default()
    };
    let mut machine = HoverMachine::new(hover_config);
    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    println!("  dwell     {}ms", dwell.as_millis());

    println!();
    println!("  Lens is running. Rest the pointer on a sentence.");
    println!("  Press Ctrl+C to quit.");

    // Product loop.
    let poll = Duration::from_millis(POINTER_POLL_MS);
    let start = Instant::now();

    // Channel for Ctrl+C handling.
    let (tx, rx) = mpsc::channel::<()>();
    spawn_ctrlc_handler(tx);

    loop {
        if rx.try_recv().is_ok() {
            break;
        }

        let now = Instant::now();
        let point = mouse_position();
        let ts = now.duration_since(start);

        let sample = PointerSample::moved(point, ts);
        let event = machine.observe(sample);

        match event {
            Some(HoverEvent::Settled { generation, point }) => {
                let ctx = AcquireContext::new(generation);
                match acquirer.acquire(point, &ctx) {
                    Ok(target) => {
                        let sentence = target.segmented.sentence_text().to_owned();
                        if sentence.is_empty() {
                            overlay.hide();
                            continue;
                        }

                        // Language decision.
                        match decide(&sentence, &language) {
                            lens_core::language::Decision::AlreadyInTarget => {
                                // Show a card saying the text is already readable.
                                let card = Card {
                                    original: sentence.clone(),
                                    body: lens_core::card::CardBody::Problem(
                                        "already in your language · Ctrl+Alt+D to explain"
                                            .to_owned(),
                                    ),
                                    footer: String::new(),
                                };
                                show_card(&overlay, &card, &target);
                            }
                            _ => {
                                // Check cache first.
                                let request = QuickRequest::new(&sentence, &language);
                                let model_name = translator.model();
                                let key = CacheKey::of(&request, model_name, 1);
                                if let Some(cached) = cache.get(&key) {
                                    let card = Card::translated(&sentence, &cached);
                                    show_card(&overlay, &card, &target);
                                    continue;
                                }

                                // Translate.
                                match translator.quick(&request) {
                                    Ok(translation) => {
                                        cache.insert(key, translation.clone());
                                        let card = Card::translated(&sentence, &translation);
                                        show_card(&overlay, &card, &target);
                                    }
                                    Err(e) => {
                                        let card = Card {
                                            original: sentence,
                                            body: lens_core::card::CardBody::Problem(
                                                e.user_message().to_owned(),
                                            ),
                                            footer: String::new(),
                                        };
                                        show_card(&overlay, &card, &target);
                                    }
                                }
                            }
                        }
                    }
                    Err(AcquireError::Unavailable) => {
                        eprintln!("  AX permission lost. Re-enable in System Settings.");
                        break;
                    }
                    Err(_) => {
                        overlay.hide();
                    }
                }
            }
            Some(HoverEvent::Left { .. }) => {
                overlay.hide();
            }
            None => {}
        }

        std::thread::sleep(poll);
    }

    println!("\n  Lens stopped.");
    std::process::ExitCode::SUCCESS
}

/// Show a card on the overlay at the position derived from the text target.
fn show_card(overlay: &Overlay, card: &Card, target: &lens_core::acquire::TextTarget) {
    let anchor = match target.anchor() {
        Some(a) => a,
        None => return,
    };

    // Convert from physical pixels to logical points for the overlay.
    let scale = overlay::display_scale_at(
        anchor.x as f64 / 2.0,
        anchor.bottom() as f64 / 2.0,
    );
    let scale = if scale > 0.0 { scale } else { 2.0 };

    let x = anchor.x as f64 / scale;
    let y = anchor.bottom() as f64 / scale + 4.0;
    let width = (anchor.width as f64 / scale).max(200.0).min(600.0);
    let height = 80.0;

    let source = &card.original;
    let translation = match &card.body {
        lens_core::card::CardBody::Translation(t) => t.clone(),
        lens_core::card::CardBody::Problem(p) => p.clone(),
        lens_core::card::CardBody::Waiting(w) => w.clone(),
        lens_core::card::CardBody::Analysis { .. } => "(analysis)".to_owned(),
    };

    overlay.show_text(source, &translation, x, y, width, height);
}

/// Spawn a thread that sends on the channel when Ctrl+C is pressed.
fn spawn_ctrlc_handler(tx: mpsc::Sender<()>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let received = Arc::new(AtomicBool::new(false));
    let received_clone = received.clone();

    // Install SIGINT handler via raw FFI (no libc crate dependency).
    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    }
    const SIGINT: i32 = 2;

    // Use a global to bridge the signal handler to our closure.
    static SIGNAL_RECEIVED: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();
    let _ = SIGNAL_RECEIVED.set(received_clone);

    extern "C" fn sigint_handler(_sig: i32) {
        if let Some(flag) = SIGNAL_RECEIVED.get() {
            flag.store(true, Ordering::SeqCst);
        }
    }

    // SAFETY: signal() is a standard POSIX call. Our handler only sets an
    // AtomicBool, which is async-signal-safe.
    unsafe { signal(SIGINT, sigint_handler) };

    std::thread::spawn(move || {
        loop {
            if received.load(Ordering::SeqCst) {
                let _ = tx.send(());
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}
