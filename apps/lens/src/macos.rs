//! macOS product entry point.
//!
//! The full product loop on macOS:
//!
//! ```text
//! pointer settles → AX acquisition → local translation → native card
//!                                    → OCR fallback (if enabled) → card
//! ```
//!
//! Included from `main.rs` behind `#[cfg(target_os = "macos")]`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::analyse::{Analyser, DeepRequest, Profile};
use lens_core::cache::{AnalysisCache, CacheKey, TranslationCache};
use lens_core::card::{Card, CardBody};
use lens_core::geometry::ScreenPoint;
use lens_core::hover::{HoverConfig, HoverEvent, HoverMachine, PointerSample};
use lens_core::language::decide;
use lens_core::policy::AppTier;
use lens_core::session::OcrMode;
use lens_core::settings::{self, Problem, Settings};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_macos::{
    clipboard, hotkey, overlay, AxPermission, Lifecycle, MacTextAcquirer, MenuBar, OcrTextAcquirer,
    Overlay, ProductCommand, SingleInstance,
};
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
    --auto              Translate in every application (default = allow).
    --ocr <mode>        off, manual, or automatic. Default manual.
    -V, --version       Print the version.
    -h, --help          This text.
";

const POINTER_POLL_MS: u64 = 25;
const CACHE_ENTRIES: usize = 256;
const ANALYSIS_ENTRIES: usize = 32;
const DEEP_PROMPT_VERSION: u32 = 1;

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
    ocr: Option<String>,
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
            "--ocr" => flags.ocr = args.next(),
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

/// Get the current mouse position in physical screen pixels (top-left origin).
fn mouse_position() -> ScreenPoint {
    let (x, y) = match overlay::mouse_position() {
        Some(pos) => pos,
        None => return ScreenPoint::new(0, 0),
    };
    let scale = overlay::display_scale_at(x, y);
    let scale = if scale > 0.0 { scale } else { 1.0 };
    ScreenPoint::new((x * scale).round() as i32, (y * scale).round() as i32)
}

/// Convert physical screen pixels to logical points for the overlay,
/// using the display topology at the anchor position.
fn physical_to_logical(point: ScreenPoint) -> (f64, f64, f64) {
    // Use physical pixel coordinates for display lookup — this correctly
    // handles mixed-scale multi-monitor setups where logical point (x,y)
    // might fall on a different display than physical pixel (x,y).
    let scale = overlay::display_scale_at_physical(point.x as f64, point.y as f64);
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let x = point.x as f64 / scale;
    let y = point.y as f64 / scale;
    (x, y, scale)
}

pub fn main() -> std::process::ExitCode {
    let flags = match parse_flags() {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{msg}\n\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };

    // Single instance.
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
    let language = flags.language.unwrap_or_else(|| settings.language.clone());
    let endpoint = flags.endpoint.unwrap_or_else(|| settings.endpoint.clone());
    let model = flags.model.or_else(|| settings.model.clone());

    // Policy: --auto sets default tier to Allow.
    // TODO: integrate policy into the hover loop for per-app deny/allow.
    let mut _policy = settings.policy();
    if flags.auto {
        _policy.set_default(AppTier::Allow);
    }

    // OCR mode.
    let ocr_mode = flags
        .ocr
        .as_deref()
        .and_then(OcrMode::named)
        .or_else(|| OcrMode::named(&settings.ocr))
        .unwrap_or(OcrMode::Manual);

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
    println!("  ocr       {}", ocr_mode.as_str());

    // AX permission — not fatal. Degrade gracefully.
    let mut ax_trusted = match AxPermission::check_with_prompt(true) {
        AxPermission::Trusted => {
            println!("  ax        trusted");
            true
        }
        AxPermission::NotTrusted => {
            println!("  ax        NOT TRUSTED (hover translation unavailable)");
            println!("            open System Settings > Privacy & Security > Accessibility");
            false
        }
    };

    // Screen Recording permission — independent from AX.
    let screen_recording = lens_platform_macos::capture::is_available();
    println!(
        "  screen    {}",
        if screen_recording {
            "available"
        } else {
            "NOT AVAILABLE (OCR unavailable)"
        }
    );

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

    // Overlay — not fatal.
    let overlay = match Overlay::start() {
        Some(o) => {
            println!("  overlay   ready");
            Some(o)
        }
        None => {
            println!("  overlay   FAILED (cards will not be shown)");
            None
        }
    };

    // Acquirers.
    let mut ax_acquirer = MacTextAcquirer::new();
    let ocr_acquirer = if screen_recording {
        Some(OcrTextAcquirer::new())
    } else {
        None
    };
    println!("  acquire   AX ready{}", if ocr_acquirer.is_some() { ", OCR ready" } else { "" });

    // Lifecycle observer: sleep/wake, display changes.
    let lifecycle = Lifecycle::start();
    if lifecycle.is_some() {
        println!("  lifecycle observing wake + display changes");
    }

    // Hover machine.
    let dwell = Duration::from_millis(flags.dwell_ms.unwrap_or(settings.dwell_ms));
    let hover_config = HoverConfig {
        dwell,
        ..HoverConfig::default()
    };
    let mut machine = HoverMachine::new(hover_config);
    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let mut analysis_cache = AnalysisCache::new(ANALYSIS_ENTRIES);
    let mut current_card: Option<(Card, String)> = None; // (card, sentence)

    // Menu bar — not fatal.
    let menubar = match MenuBar::create() {
        Some(mb) => {
            println!("  menubar   ready");
            Some(mb)
        }
        None => {
            println!("  menubar   unavailable (Ctrl+C to quit)");
            None
        }
    };

    // Global hotkeys — requires AX permission (already requested).
    let hotkeys_available = if ax_trusted {
        hotkey::install()
    } else {
        false
    };
    if hotkeys_available {
        println!("  hotkeys   Cmd+Opt+L/D/C/S/O/Q registered");
    }

    println!("  dwell     {}ms", dwell.as_millis());
    println!();

    if !ax_trusted && ocr_acquirer.is_none() {
        println!("  No accessibility or screen recording permission.");
        println!("  Lens cannot read any text. Grant permissions and restart.");
        // Keep running so the menu bar is available for Quit.
    } else {
        println!("  Lens is running. Rest the pointer on a sentence.");
    }
    println!("  Press Ctrl+C to quit.");

    // Product loop.
    let poll = Duration::from_millis(POINTER_POLL_MS);
    let start = Instant::now();
    let mut paused = false;
    let mut quit_requested = false;

    let (tx, rx) = mpsc::channel::<()>();
    spawn_ctrlc_handler(tx);

    loop {
        // Process AppKit events (menu actions, lifecycle notifications,
        // overlay rendering). Must be called every iteration to keep the
        // native UI responsive while the polling loop owns the main thread.
        overlay::process_appkit_events();

        // Drain commands from both menu bar and global hotkeys into a
        // unified dispatch.
        let mut pending_commands: Vec<ProductCommand> = Vec::new();

        // Menu bar commands.
        if let Some(ref mb) = menubar {
            if mb.poll_quit() { pending_commands.push(ProductCommand::Quit); }
            if mb.poll_pause_toggle() { pending_commands.push(ProductCommand::PauseToggle); }
            if mb.poll_deep() { pending_commands.push(ProductCommand::Deep); }
            if mb.poll_copy() { pending_commands.push(ProductCommand::Copy); }
            if mb.poll_save() { pending_commands.push(ProductCommand::Save); }
            if mb.poll_ocr() { pending_commands.push(ProductCommand::ManualOcr); }
        }

        // Global hotkey commands.
        loop {
            let cmd = hotkey::poll();
            if cmd == ProductCommand::None { break; }
            pending_commands.push(cmd);
        }

        // Dispatch all pending commands.
        for cmd in pending_commands {
            match cmd {
                ProductCommand::Quit => {
                    quit_requested = true;
                }
                ProductCommand::PauseToggle => {
                    paused = !paused;
                    if let Some(ref o) = overlay {
                        if paused { o.hide(); }
                    }
                    if let Some(ref mb) = menubar {
                        mb.set_title(if paused { "Lens ⏸" } else { "Lens" });
                    }
                }
                ProductCommand::Deep => {
                    execute_deep(&mut translator, &current_card, &language, &overlay, &mut analysis_cache);
                }
                ProductCommand::Copy => {
                    if let Some((ref card, _)) = current_card {
                        if let Some(text) = card.for_the_clipboard() {
                            clipboard::put_text(&text);
                        }
                    }
                }
                ProductCommand::Save => {
                    if let Some((ref card, ref sentence)) = current_card {
                        execute_save(card, sentence);
                    }
                }
                ProductCommand::ManualOcr => {
                    if let Some(ref ocr) = ocr_acquirer {
                        if let Some(ref o) = overlay { o.hide(); }
                        let point = mouse_position();
                        let gen = lens_core::hover::Generation(0);
                        let ctx = AcquireContext::new(gen);
                        match ocr.acquire(point, &ctx) {
                            Ok(target) => {
                                let sentence = target.segmented.sentence_text().to_owned();
                                if !sentence.is_empty() {
                                    let request = QuickRequest::new(&sentence, &language);
                                    match translator.quick(&request) {
                                        Ok(translation) => {
                                            let card = Card::translated(&sentence, &translation);
                                            show_card(&overlay, &card, &target);
                                            current_card = Some((card, sentence));
                                        }
                                        Err(e) => {
                                            let card = Card {
                                                original: sentence.clone(),
                                                body: CardBody::Problem(e.user_message().to_owned()),
                                                footer: String::new(),
                                            };
                                            show_card(&overlay, &card, &target);
                                            current_card = Some((card, sentence));
                                        }
                                    }
                                }
                            }
                            Err(_) => {}
                        }
                    }
                }
                ProductCommand::None => {}
            }
        }

        // Check quit from commands or Ctrl+C.
        if quit_requested || rx.try_recv().is_ok() { break; }

        if paused {
            std::thread::sleep(poll);
            continue;
        }

        // Lifecycle: wake / display change.
        if let Some(ref lc) = lifecycle {
            if lc.poll_wake() {
                // After wake: refresh display topology and re-check AX permission.
                ax_acquirer.refresh_displays();
                ax_trusted = match AxPermission::check() {
                    AxPermission::Trusted => true,
                    AxPermission::NotTrusted => {
                        println!("  ax        permission lost after wake");
                        false
                    }
                };
            }
            if lc.poll_display_change() {
                // Display configuration changed: refresh topology.
                ax_acquirer.refresh_displays();
            }
        }

        // Pointer sampling.
        let now = Instant::now();
        let point = mouse_position();
        let ts = now.duration_since(start);
        let sample = PointerSample::moved(point, ts);
        let event = machine.observe(sample);

        match event {
            Some(HoverEvent::Settled { generation, point }) => {
                // Try AX acquisition first (accessibility-first contract).
                let acquire_result = if ax_trusted {
                    let ctx = AcquireContext::new(generation);
                    ax_acquirer.acquire(point, &ctx)
                } else {
                    Err(AcquireError::Unavailable)
                };

                // OCR fallback: only when AX failed and OCR is enabled.
                let (target, _from_ocr) = match acquire_result {
                    Ok(target) => (target, false),
                    Err(e) if e.ocr_can_help() && ocr_mode != OcrMode::Off => {
                        if let Some(ref ocr) = ocr_acquirer {
                            let ctx = AcquireContext::new(generation);
                            match ocr.acquire(point, &ctx) {
                                Ok(target) => (target, true),
                                Err(_) => {
                                    if let Some(ref o) = overlay {
                                        o.hide();
                                    }
                                    continue;
                                }
                            }
                        } else {
                            if let Some(ref o) = overlay {
                                o.hide();
                            }
                            continue;
                        }
                    }
                    Err(_) => {
                        if let Some(ref o) = overlay {
                            o.hide();
                        }
                        continue;
                    }
                };

                let sentence = target.segmented.sentence_text().to_owned();
                if sentence.is_empty() {
                    if let Some(ref o) = overlay {
                        o.hide();
                    }
                    continue;
                }

                // Language decision.
                match decide(&sentence, &language) {
                    lens_core::language::Decision::AlreadyInTarget => {
                        let card = Card {
                            original: sentence.clone(),
                            body: CardBody::Problem(
                                "already in your language".to_owned(),
                            ),
                            footer: String::new(),
                        };
                        show_card(&overlay, &card, &target);
                        current_card = Some((card, sentence));
                    }
                    _ => {
                        let request = QuickRequest::new(&sentence, &language);
                        let model_name = translator.model();
                        let key = CacheKey::of(&request, model_name, 1);

                        if let Some(cached) = cache.get(&key) {
                            let card = Card::translated(&sentence, &cached);
                            show_card(&overlay, &card, &target);
                            current_card = Some((card, sentence));
                            continue;
                        }

                        match translator.quick(&request) {
                            Ok(translation) => {
                                cache.insert(key, translation.clone());
                                let card = Card::translated(&sentence, &translation);
                                show_card(&overlay, &card, &target);
                                current_card = Some((card, sentence));
                            }
                            Err(e) => {
                                let card = Card {
                                    original: sentence.clone(),
                                    body: CardBody::Problem(e.user_message().to_owned()),
                                    footer: String::new(),
                                };
                                show_card(&overlay, &card, &target);
                                current_card = Some((card, sentence));
                            }
                        }
                    }
                }
            }
            Some(HoverEvent::Left { .. }) => {
                if let Some(ref o) = overlay {
                    o.hide();
                }
            }
            None => {}
        }

        std::thread::sleep(poll);
    }

    println!("\n  Lens stopped.");
    std::process::ExitCode::SUCCESS
}

/// Build a minimal TextTarget for card display when the original target
/// is not available (e.g. Deep Analysis on a cached card).
fn make_dummy_target(sentence: &str) -> lens_core::acquire::TextTarget {
    lens_core::acquire::TextTarget {
        app: Default::default(),
        source: lens_core::acquire::AcquisitionSource::Accessibility,
        provider: Default::default(),
        segmented: lens_core::text::segment(sentence, 0),
        word_bounds: vec![],
        context_bounds: vec![],
    }
}

/// Execute Deep Analysis on the current card's sentence.
fn execute_deep(
    translator: &mut OllamaTranslator,
    current_card: &Option<(Card, String)>,
    language: &str,
    overlay: &Option<Overlay>,
    analysis_cache: &mut AnalysisCache,
) {
    if let Some((ref card, ref sentence)) = current_card {
        let deep_req = DeepRequest {
            sentence: sentence.clone(),
            word: None,
            target_language: language.to_owned(),
            translation: match &card.body {
                CardBody::Translation(t) => Some(t.clone()),
                _ => None,
            },
            profile: Profile::default(),
        };
        let analysis_key = CacheKey::for_analysis(
            &deep_req,
            translator.model(),
            DEEP_PROMPT_VERSION,
        );
        if let Some(cached) = analysis_cache.get(&analysis_key) {
            let analysis_card = Card::analysed(sentence, &cached);
            show_card(overlay, &analysis_card, &make_dummy_target(sentence));
            return;
        }
        match translator.deep(&deep_req) {
            Ok(analysis) => {
                analysis_cache.insert(analysis_key, analysis.clone());
                let analysis_card = Card::analysed(sentence, &analysis);
                show_card(overlay, &analysis_card, &make_dummy_target(sentence));
            }
            Err(e) => {
                let err_card = Card {
                    original: sentence.clone(),
                    body: CardBody::Problem(format!("analysis failed: {}", e.user_message())),
                    footer: String::new(),
                };
                show_card(overlay, &err_card, &make_dummy_target(sentence));
            }
        }
    }
}

/// Execute Save: append the current card content to saved.md.
fn execute_save(card: &Card, sentence: &str) {
    let save_path = default_settings_path()
        .parent()
        .map(|p| p.join("saved.md"))
        .unwrap_or_else(|| std::path::PathBuf::from("saved.md"));
    let content = card.for_the_clipboard().unwrap_or_else(|| sentence.to_owned());
    let entry = format!("\n---\n\n{}\n", content);
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&save_path)
        .and_then(|mut f| {
            use std::io::Write;
            f.write_all(entry.as_bytes())
        });
}

/// Show a card on the overlay using the tested coordinate helpers.
fn show_card(overlay: &Option<Overlay>, card: &Card, target: &lens_core::acquire::TextTarget) {
    let Some(overlay) = overlay else { return };
    let anchor = match target.anchor() {
        Some(a) => a,
        None => return,
    };

    // Use the canonical coordinate conversion: physical pixels → logical
    // points via the display scale at the anchor position.
    let (x, y, _scale) = physical_to_logical(ScreenPoint::new(anchor.x, anchor.bottom() + 8));
    let width = {
        let (w, _, _) = physical_to_logical(ScreenPoint::new(anchor.width, 0));
        w.max(200.0).min(600.0)
    };
    let height = 80.0;

    let source = &card.original;
    let body_text = card_body_to_text(&card.body);

    overlay.show_text(source, &body_text, x, y, width, height);
}

/// Convert a CardBody to displayable text for the overlay.
fn card_body_to_text(body: &CardBody) -> String {
    match body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Problem(p) => p.clone(),
        CardBody::Waiting(w) => w.clone(),
        CardBody::Analysis {
            translation, rows, ..
        } => {
            // Render analysis rows as text. If there's a translation, show
            // it first, then the analysis rows.
            let mut text = String::new();
            if let Some(t) = translation {
                text.push_str(t);
                text.push('\n');
            }
            text.push_str(&CardBody::rows_as_text(rows));
            text
        }
    }
}

fn spawn_ctrlc_handler(tx: mpsc::Sender<()>) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let received = Arc::new(AtomicBool::new(false));
    let received_clone = received.clone();

    extern "C" {
        fn signal(sig: i32, handler: extern "C" fn(i32)) -> usize;
    }
    const SIGINT: i32 = 2;

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
