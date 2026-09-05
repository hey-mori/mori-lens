//! macOS product entry point.
//!
//! Architecture mirrors Windows:
//! - Main thread: shared state → native components → Tauri build → spawn worker → Tauri run
//! - Worker thread: product loop (pointer, hover, acquisition, translation, overlay)
//!
//! ```text
//! pointer settles → AX acquisition → local translation → native card
//!                                    → OCR fallback (if enabled) → card
//! ```
//!
//! Included from `main.rs` behind `#[cfg(target_os = "macos")]`.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::analyse::{Analyser, DeepRequest, Profile};
use lens_core::cache::{AnalysisCache, CacheKey, TranslationCache};
use lens_core::card::{Card, CardBody};
use lens_core::geometry::ScreenPoint;
use lens_core::hover::{HoverConfig, HoverEvent, HoverMachine, PointerSample};
use lens_core::language::decide;
use lens_core::policy::AppPolicy;
use lens_core::session::OcrMode;
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_macos::{
    clipboard, hotkey, overlay, AxPermission, Lifecycle, MacTextAcquirer, MenuBar, OcrTextAcquirer,
    Overlay, ProductCommand, SingleInstance,
};
use lens_translate::OllamaTranslator;

use crate::detail_view::{self, SharedSavePath};
use crate::library_service::{self, SharedLibrary};
use crate::settings_service::{self, SharedSettings};
use crate::tauri_app::{self, AppHandle, SharedDiagnostics, SharedRecent};

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
    let scale = overlay::display_scale_at_physical(x, y);
    let scale = if scale > 0.0 { scale } else { 1.0 };
    ScreenPoint::new((x * scale).round() as i32, (y * scale).round() as i32)
}

/// Convert physical screen pixels to logical points for the overlay.
fn physical_to_logical(point: ScreenPoint) -> (f64, f64, f64) {
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

    // Data directory: ~/Library/Application Support/Mori Lens
    let data_dir = {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        std::path::PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Mori Lens")
    };
    let _ = std::fs::create_dir_all(&data_dir);

    // Shared settings (via settings_service — same as Windows).
    let shared_settings: SharedSettings = Arc::new(Mutex::new(
        settings_service::load_or_migrate(&data_dir),
    ));

    // Shared library (via library_service — same as Windows).
    let shared_library: SharedLibrary = Arc::new(Mutex::new(library_service::open(&data_dir)));
    println!("  library   {:?}", shared_library.lock().unwrap().path());

    // Shared detail, recent.
    let (shared_detail, explain_flag) = detail_view::new_shared();
    let shared_save_path: SharedSavePath = Arc::new(data_dir.join("saved.md"));
    let shared_recent: SharedRecent = Arc::new(Mutex::new(lens_core::recent::SessionRecent::new()));

    // Build Tauri app (must happen on main thread before spawning worker).
    let (tauri_handle, shared_diagnostics, tauri_app) = match tauri_app::build(
        shared_settings.clone(),
        shared_library.clone(),
        shared_detail.clone(),
        shared_recent.clone(),
        explain_flag.clone(),
        shared_save_path,
    ) {
        Ok(triple) => triple,
        Err(error) => {
            eprintln!("Tauri initialization failed: {error}");
            return std::process::ExitCode::from(1);
        }
    };
    println!("  tauri     running");

    // Print startup banner.
    {
        let settings = shared_settings.lock().unwrap().current().clone();
        println!("Mori Lens · macOS");
        println!("  settings  {:?}", data_dir);
        println!("  language  {}", settings.language);
        println!("  endpoint  {}", settings.endpoint);
    }

    // AX permission — not fatal.
    let mut ax_trusted = match AxPermission::check_with_prompt(true) {
        AxPermission::Trusted => {
            println!("  ax        trusted");
            true
        }
        AxPermission::NotTrusted => {
            println!("  ax        NOT TRUSTED (hover translation unavailable)");
            false
        }
    };

    // Screen Recording — independent.
    let screen_recording = lens_platform_macos::capture::is_available();
    println!(
        "  screen    {}",
        if screen_recording { "available" } else { "NOT AVAILABLE" }
    );

    // Native components (created on main thread, moved to worker).
    let overlay = Overlay::start();
    if overlay.is_some() {
        println!("  overlay   ready");
    }
    let menubar = MenuBar::create();
    if menubar.is_some() {
        println!("  menubar   ready");
    }
    let hotkeys_available = if ax_trusted { hotkey::install() } else { false };
    if hotkeys_available {
        println!("  hotkeys   Cmd+Opt+L/D/C/S/O/Q registered");
    }
    let lifecycle = Lifecycle::start();
    if lifecycle.is_some() {
        println!("  lifecycle observing wake + display changes");
    }

    // Acquirers.
    let ax_acquirer = MacTextAcquirer::new();
    let ocr_acquirer = if screen_recording {
        Some(OcrTextAcquirer::new())
    } else {
        None
    };

    // Read current settings for the product loop.
    let (language, endpoint, model, dwell_ms, ocr_mode, policy) = {
        let s = shared_settings.lock().unwrap().current().clone();
        let p = s.policy();
        let lang = flags.language.unwrap_or(s.language);
        let ep = flags.endpoint.unwrap_or(s.endpoint);
        let m = flags.model.or(s.model);
        let d = flags.dwell_ms.unwrap_or(s.dwell_ms);
        let o = flags
            .ocr
            .as_deref()
            .and_then(OcrMode::named)
            .or_else(|| OcrMode::named(&s.ocr))
            .unwrap_or(OcrMode::Manual);
        (lang, ep, m, d, o, p)
    };
    let mut policy = policy;
    policy.deny_self("lens");

    println!("  dwell     {}ms", dwell_ms);
    println!();
    println!("  Lens is running. Rest the pointer on a sentence.");
    println!("  Press Ctrl+C or Cmd+Opt+Q to quit.");

    // Spawn product loop on worker thread.
    let loop_tauri = tauri_handle.clone();
    let loop_diagnostics = shared_diagnostics.clone();
    let loop_settings = shared_settings.clone();
    let loop_library = shared_library.clone();

    std::thread::spawn(move || {
        product_loop(
            loop_tauri,
            loop_diagnostics,
            loop_settings,
            loop_library,
            overlay,
            menubar,
            lifecycle,
            ax_acquirer,
            ocr_acquirer,
            ax_trusted,
            screen_recording,
            hotkeys_available,
            language,
            endpoint,
            model,
            dwell_ms,
            ocr_mode,
            policy,
        );
        // Tell Tauri to exit when the product loop is done.
        // (We can't call this from inside the loop because we need to
        // consume the handle.)
    });

    // Run Tauri on main thread. This blocks until the event loop exits.
    tauri_app::run(tauri_app);

    std::process::ExitCode::SUCCESS
}

/// The product loop. Runs on a worker thread.
fn product_loop(
    tauri_handle: AppHandle,
    _shared_diagnostics: SharedDiagnostics,
    _shared_settings: SharedSettings,
    shared_library: SharedLibrary,
    overlay: Option<Overlay>,
    menubar: Option<MenuBar>,
    lifecycle: Option<Lifecycle>,
    mut ax_acquirer: MacTextAcquirer,
    ocr_acquirer: Option<OcrTextAcquirer>,
    mut ax_trusted: bool,
    _screen_recording: bool,
    hotkeys_available: bool,
    language: String,
    endpoint: String,
    model: Option<String>,
    dwell_ms: u64,
    ocr_mode: OcrMode,
    policy: AppPolicy,
) {
    // Translator.
    let mut translator = if let Some(ref m) = model {
        OllamaTranslator::new(&endpoint, m)
    } else {
        OllamaTranslator::choosing(&endpoint)
    };
    let _ = translator.resolve_model();

    // Hover machine.
    let dwell = Duration::from_millis(dwell_ms);
    let hover_config = HoverConfig {
        dwell,
        ..HoverConfig::default()
    };
    let mut machine = HoverMachine::new(hover_config);
    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let mut analysis_cache = AnalysisCache::new(ANALYSIS_ENTRIES);
    let mut current_card: Option<(Card, String)> = None;
    let mut current_generation: u64 = 0;
    let mut paused = false;
    let mut quit_requested = false;

    // Ctrl+C handler.
    let (tx, rx) = mpsc::channel::<()>();
    spawn_ctrlc_handler(tx);

    let poll = Duration::from_millis(POINTER_POLL_MS);
    let start = Instant::now();

    loop {
        // Drain commands from menu bar and global hotkeys.
        let mut pending_commands: Vec<ProductCommand> = Vec::new();

        if let Some(ref mb) = menubar {
            if mb.poll_quit() { pending_commands.push(ProductCommand::Quit); }
            if mb.poll_pause_toggle() { pending_commands.push(ProductCommand::PauseToggle); }
            if mb.poll_deep() { pending_commands.push(ProductCommand::Deep); }
            if mb.poll_copy() { pending_commands.push(ProductCommand::Copy); }
            if mb.poll_save() { pending_commands.push(ProductCommand::Save); }
            if mb.poll_ocr() { pending_commands.push(ProductCommand::ManualOcr); }
            if mb.poll_open_settings() { pending_commands.push(ProductCommand::OpenSettings); }
            if mb.poll_open_library() { pending_commands.push(ProductCommand::OpenLibrary); }
        }

        if hotkeys_available {
            loop {
                let cmd = hotkey::poll();
                if cmd == ProductCommand::None { break; }
                pending_commands.push(cmd);
            }
        }

        // Dispatch commands.
        for cmd in pending_commands {
            match cmd {
                ProductCommand::Quit => { quit_requested = true; }
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
                        execute_save(card, sentence, &shared_library);
                    }
                }
                ProductCommand::ManualOcr => {
                    if let Some(ref ocr) = ocr_acquirer {
                        if let Some(ref o) = overlay { o.hide(); }
                        let point = mouse_position();
                        current_generation += 1;
                        let gen = lens_core::hover::Generation(current_generation);
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
                ProductCommand::OpenSettings => {
                    tauri_handle.show_window();
                }
                ProductCommand::OpenLibrary => {
                    tauri_handle.show_window();
                }
                ProductCommand::None => {}
            }
        }

        // Check quit.
        if quit_requested || rx.try_recv().is_ok() {
            tauri_handle.exit();
            break;
        }

        if paused {
            std::thread::sleep(poll);
            continue;
        }

        // Lifecycle: wake / display change.
        if let Some(ref lc) = lifecycle {
            if lc.poll_wake() {
                ax_acquirer.refresh_displays();
                ax_trusted = match AxPermission::check() {
                    AxPermission::Trusted => true,
                    AxPermission::NotTrusted => false,
                };
            }
            if lc.poll_display_change() {
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
                // Policy check: get process name BEFORE any content acquisition.
                // This is a lightweight metadata lookup — no text is read.
                let process_name = ax_acquirer.process_name_at(point);
                if !policy.allows(process_name.as_deref(), false) {
                    // Denied by policy: skip both AX and OCR.
                    if let Some(ref o) = overlay { o.hide(); }
                    continue;
                }

                // AX acquisition first.
                let acquire_result = if ax_trusted {
                    let ctx = AcquireContext::new(generation);
                    ax_acquirer.acquire(point, &ctx)
                } else {
                    Err(AcquireError::Unavailable)
                };

                // OCR fallback.
                let (target, _from_ocr) = match acquire_result {
                    Ok(target) => (target, false),
                    Err(e) if e.ocr_can_help() && ocr_mode != OcrMode::Off => {
                        if let Some(ref ocr) = ocr_acquirer {
                            let ctx = AcquireContext::new(generation);
                            match ocr.acquire(point, &ctx) {
                                Ok(target) => (target, true),
                                Err(_) => {
                                    if let Some(ref o) = overlay { o.hide(); }
                                    continue;
                                }
                            }
                        } else {
                            if let Some(ref o) = overlay { o.hide(); }
                            continue;
                        }
                    }
                    Err(_) => {
                        if let Some(ref o) = overlay { o.hide(); }
                        continue;
                    }
                };

                let sentence = target.segmented.sentence_text().to_owned();
                if sentence.is_empty() {
                    if let Some(ref o) = overlay { o.hide(); }
                    continue;
                }

                // Language decision + translation.
                match decide(&sentence, &language) {
                    lens_core::language::Decision::AlreadyInTarget => {
                        let card = Card {
                            original: sentence.clone(),
                            body: CardBody::Problem("already in your language".to_owned()),
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
                if let Some(ref o) = overlay { o.hide(); }
                current_card = None;
            }
            None => {}
        }

        std::thread::sleep(poll);
    }
}

// -- Helper functions -------------------------------------------------------

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
        let analysis_key = CacheKey::for_analysis(&deep_req, translator.model(), DEEP_PROMPT_VERSION);
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

/// Save using the canonical Library API (same as Windows).
fn execute_save(card: &Card, sentence: &str, shared_library: &SharedLibrary) {
    let answer = match &card.body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Analysis { translation, rows, .. } => {
            let mut text = String::new();
            if let Some(t) = translation { text.push_str(t); text.push('\n'); }
            text.push_str(&CardBody::rows_as_text(rows));
            text
        }
        CardBody::Problem(p) => p.clone(),
        CardBody::Waiting(w) => w.clone(),
    };
    let kind = match &card.body {
        CardBody::Analysis { .. } => "analysis",
        _ => "translation",
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let item = lens_storage::Item {
        id: 0,
        original: sentence.to_owned(),
        answer,
        caveat: None,
        app: None,
        created_at: now,
        kind: kind.to_owned(),
        source_language: None,
        target_language: None,
        model: None,
        note: None,
        tags: None,
        review_state: None,
        next_review_at: None,
        review_count: None,
    };
    let lib = shared_library.lock().unwrap();
    match lib.library().insert(&item) {
        Ok(id) => eprintln!("  save      saved as library item #{id}"),
        Err(e) => eprintln!("  save      library insert failed: {e}"),
    }
}

fn show_card(overlay: &Option<Overlay>, card: &Card, target: &lens_core::acquire::TextTarget) {
    let Some(overlay) = overlay else { return };
    let anchor = match target.anchor() {
        Some(a) => a,
        None => return,
    };
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

fn card_body_to_text(body: &CardBody) -> String {
    match body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Problem(p) => p.clone(),
        CardBody::Waiting(w) => w.clone(),
        CardBody::Analysis { translation, rows, .. } => {
            let mut text = String::new();
            if let Some(t) = translation { text.push_str(t); text.push('\n'); }
            text.push_str(&CardBody::rows_as_text(rows));
            text
        }
    }
}

fn spawn_ctrlc_handler(tx: mpsc::Sender<()>) {
    use std::sync::atomic::{AtomicBool, Ordering};
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

    // SAFETY: signal() is a standard POSIX call.
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
