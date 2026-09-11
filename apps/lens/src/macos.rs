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

// Style lints that would require significant refactoring to fix.
// The current code is correct; these are readability preferences.
#![allow(clippy::while_let_loop, clippy::single_match, clippy::manual_clamp)]

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::dispatch_carrier::{DeepJob, QuickJob, TransResult};

use lens_core::acquire::{AcquireContext, AcquireError, AcquisitionSource, TextAcquirer};
use lens_core::analyse::Analyser;
use lens_core::cache::{AnalysisCache, CacheKey, TranslationCache};
use lens_core::card::{Card, CardBody};
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::{HoverConfig, HoverEvent, HoverMachine, PointerSample};
use lens_core::language::decide;
use lens_core::policy::AppPolicy;
use lens_core::session::OcrMode;
use lens_core::tally::Tally;
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_macos::{
    clipboard, hotkey, overlay, AxPermission, Lifecycle, MacTextAcquirer, MenuBar, OcrTextAcquirer,
    Overlay, ProductCommand, SingleInstance,
};
use lens_translate::OllamaTranslator;

use crate::ai_runtime::AiRuntimeConfig;
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
const QUICK_PROMPT_VERSION: u32 = 1;
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
        let mut value = |name: &str| args.next().ok_or(format!("{name} needs a value"));
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
            "--settings" => flags.settings = Some(value("--settings")?.into()),
            "--language" => flags.language = Some(value("--language")?),
            "--model" => flags.model = Some(value("--model")?),
            "--endpoint" => flags.endpoint = Some(value("--endpoint")?),
            "--ocr" => flags.ocr = Some(value("--ocr")?),
            "--dwell" => {
                let ms = value("--dwell")?
                    .parse()
                    .map_err(|_| "--dwell needs milliseconds".to_string())?;
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

    // Data directory: derive from --settings flag if provided, otherwise use default.
    // This matches Windows behavior where data_dir = settings.parent().
    let (data_dir, settings_path) = if let Some(ref custom_path) = flags.settings {
        let dir = custom_path
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                std::path::PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Mori Lens")
            });
        (dir, custom_path.clone())
    } else {
        let data_dir = {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Mori Lens")
        };
        (data_dir.clone(), data_dir.join("settings.toml"))
    };
    let _ = std::fs::create_dir_all(&data_dir);

    // Shared settings (via settings_service — same as Windows).
    let shared_settings: SharedSettings = Arc::new(Mutex::new(settings_service::load_or_migrate(
        &settings_path,
    )));

    // Shared library (via library_service — same as Windows).
    let shared_library: SharedLibrary = Arc::new(Mutex::new(library_service::open(&data_dir)));
    {
        let lib = shared_library.lock().unwrap();
        match lib.backing() {
            library_service::LibraryBacking::Persistent(_) => {
                println!("  library   {:?} (persistent)", lib.path());
            }
            library_service::LibraryBacking::InMemoryFallback { reason, .. } => {
                println!("  library   in-memory fallback: {reason}");
            }
        }
    }

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
        shared_save_path.clone(),
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
        let guard = shared_settings.lock().unwrap();
        let settings = guard.current().clone();
        let ai_config = AiRuntimeConfig::from_schema(guard.schema());
        drop(guard);
        println!("Mori Lens · macOS");
        println!("  settings  {:?}", data_dir);
        println!("  language  {}", settings.language);
        // ADR-0005: Quick and Deep have separate engines.
        println!("  quick     {}", ai_config.quick.endpoint);
        println!("  deep      {}", ai_config.deep.endpoint);
    }

    // AX permission — not fatal. Check without prompt on startup to avoid
    // SIGTRAP when the accessibility framework encounters an error during
    // the permission prompt flow. The product degrades gracefully: no AX
    // acquisition, no hotkeys, but overlay/menubar/settings still work.
    let ax_trusted = match AxPermission::check() {
        AxPermission::Trusted => {
            println!("  ax        trusted");
            true
        }
        AxPermission::NotTrusted => {
            println!("  ax        NOT TRUSTED (hover translation unavailable)");
            println!("             Grant Accessibility permission in System Settings,");
            println!("             then restart Mori Lens.");
            false
        }
    };

    // Screen Recording — independent.
    let screen_recording = lens_platform_macos::capture::is_available();
    println!(
        "  screen    {}",
        if screen_recording {
            "available"
        } else {
            "NOT AVAILABLE"
        }
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
    // Always create the OCR acquirer. Permission is checked at use time,
    // not at startup. This lets the user grant Screen Recording permission
    // after launch and use OCR without restarting (once granted, the next
    // OCR attempt rechecks via CGPreflightScreenCaptureAccess).
    let ocr_acquirer = Some(OcrTextAcquirer::new());

    // Read current settings for the product loop.
    // ADR-0005: Quick and Deep have separate engines. Build from Schema,
    // not from the flat Settings view that only carries the quick lane.
    let (language, ai_config, dwell_ms, ocr_mode, policy, hover_scope) = {
        let guard = shared_settings.lock().unwrap();
        let s = guard.current().clone();
        let mut ai_config = AiRuntimeConfig::from_schema(guard.schema());
        drop(guard);
        let mut p = s.policy();
        // --auto sets the default policy tier to Allow, matching Windows behavior.
        if flags.auto {
            p.set_default(lens_core::policy::AppTier::Allow);
        }
        let lang = flags.language.unwrap_or(s.language);
        // CLI --endpoint applies to both Quick and Deep for backwards compatibility.
        // A user who needs separate endpoints edits the settings file.
        if let Some(endpoint) = flags.endpoint {
            ai_config.quick.endpoint = endpoint.clone();
            ai_config.deep.endpoint = endpoint;
        }
        // CLI --model applies to Quick only (the default path). Deep is always
        // configured via the settings file.
        if let Some(model) = flags.model {
            ai_config.quick.model = Some(model);
        }
        let d = flags.dwell_ms.unwrap_or(s.dwell_ms);
        let o = flags
            .ocr
            .as_deref()
            .and_then(OcrMode::named)
            .or_else(|| OcrMode::named(&s.ocr))
            .unwrap_or(OcrMode::Manual);
        (lang, ai_config, d, o, p, s.hover_scope)
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
    let loop_recent = shared_recent.clone();

    std::thread::spawn(move || {
        product_loop(
            loop_tauri,
            loop_diagnostics,
            loop_settings,
            loop_library,
            loop_recent,
            shared_save_path.clone(),
            overlay,
            menubar,
            lifecycle,
            ax_acquirer,
            ocr_acquirer,
            ax_trusted,
            screen_recording,
            hotkeys_available,
            language,
            ai_config,
            dwell_ms,
            ocr_mode,
            policy,
            hover_scope,
            flags.auto,
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
#[allow(clippy::too_many_arguments)]
fn product_loop(
    tauri_handle: AppHandle,
    shared_diagnostics: SharedDiagnostics,
    shared_settings: SharedSettings,
    shared_library: SharedLibrary,
    shared_recent: SharedRecent,
    shared_save_path: SharedSavePath,
    overlay: Option<Overlay>,
    menubar: Option<MenuBar>,
    lifecycle: Option<Lifecycle>,
    mut ax_acquirer: MacTextAcquirer,
    ocr_acquirer: Option<OcrTextAcquirer>,
    mut ax_trusted: bool,
    _screen_recording: bool,
    hotkeys_available: bool,
    mut language: String,
    mut ai_config: AiRuntimeConfig,
    mut dwell_ms: u64,
    mut ocr_mode: OcrMode,
    mut policy: AppPolicy,
    mut hover_scope: String,
    auto: bool,
) {
    // Prune old history entries on startup, if history retention is configured.
    // Matches the Windows startup path (win.rs).
    {
        let settings = shared_settings.lock().unwrap();
        let current = settings.current();
        if current.history && current.history_days > 0 {
            let retention_secs = current.history_days as i64 * 86400;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let lib = shared_library.lock().unwrap();
            match lib.library().history_prune(now, retention_secs) {
                Ok(pruned) if pruned > 0 => {
                    println!(
                        "  history   pruned {pruned} entries older than {} days",
                        current.history_days
                    );
                }
                Ok(_) => {}
                Err(e) => eprintln!("  history   prune failed: {e}"),
            }
        }
    }

    let (quick_tx, quick_rx): (
        std::sync::mpsc::Sender<QuickJob>,
        std::sync::mpsc::Receiver<QuickJob>,
    ) = mpsc::channel();
    let (deep_tx, deep_rx): (
        std::sync::mpsc::Sender<DeepJob>,
        std::sync::mpsc::Receiver<DeepJob>,
    ) = mpsc::channel();
    let (result_tx, result_rx): (
        std::sync::mpsc::Sender<TransResult>,
        std::sync::mpsc::Receiver<TransResult>,
    ) = mpsc::channel();
    // Config update channels: (endpoint, model) sent when settings change.
    let (quick_config_tx, quick_config_rx) = mpsc::channel::<(String, Option<String>)>();
    let (deep_config_tx, deep_config_rx) = mpsc::channel::<(String, Option<String>)>();

    // Shared cache between product loop and translation threads.
    let cache: Arc<Mutex<TranslationCache>> =
        Arc::new(Mutex::new(TranslationCache::new(CACHE_ENTRIES)));

    // Quick lane: dedicated thread for hover translations.
    // Drains pending jobs and keeps only the latest (stale coalescing).
    // This ensures Deep never blocks Quick hover latency.
    // ADR-0005: Quick translator uses the quick lane's own config.
    let quick_translator = if let Some(ref m) = ai_config.quick.model {
        OllamaTranslator::new(&ai_config.quick.endpoint, m)
    } else {
        OllamaTranslator::choosing(&ai_config.quick.endpoint)
    };
    let quick_cache = cache.clone();
    let quick_result_tx = result_tx.clone();
    std::thread::spawn(move || {
        let mut translator = quick_translator;
        let _ = translator.resolve_model();
        while let Ok(job) = quick_rx.recv() {
            // Check for config updates (non-blocking) BEFORE any cache operation.
            // After this drain, translator.model() is authoritative for the
            // cache key: no stale model name can reach cache lookup or insert.
            while let Ok((ep, m)) = quick_config_rx.try_recv() {
                translator = match m {
                    Some(ref model) => OllamaTranslator::new(&ep, model),
                    None => OllamaTranslator::choosing(&ep),
                };
                let _ = translator.resolve_model();
            }
            // Drain pending Quick jobs — keep only the latest.
            let mut current = job;
            loop {
                match quick_rx.try_recv() {
                    Ok(newer) => current = newer,
                    Err(_) => break,
                }
            }
            // Use the production seam: into_result derives target_language
            // from the job's request, ensuring dispatch-time provenance.
            let QuickJob::Quick { ref request, .. } = current;
            // Cache key is computed here, where translator.model() is
            // authoritative. The hover loop never builds cache keys;
            // it cannot know the resolved model without a race.
            let key = CacheKey::of(request, translator.model(), QUICK_PROMPT_VERSION);
            // Check cache before paying for translation.
            {
                if let Ok(mut c) = quick_cache.lock() {
                    if let Some(cached) = c.get(&key) {
                        let _ = quick_result_tx.send(current.into_result(Ok(cached)));
                        continue;
                    }
                }
            }
            let result = translator.quick(request);
            if let Ok(ref t) = result {
                if let Ok(mut c) = quick_cache.lock() {
                    c.insert(key, t.clone());
                }
            }
            let _ = quick_result_tx
                .send(current.into_result(result.map_err(|e| e.user_message().to_owned())));
        }
    });

    // Deep lane: dedicated thread for deep analysis.
    // Single-flight: processes one Deep at a time, in order.
    // ADR-0005: Deep translator uses the deep lane's own config.
    let deep_translator = if let Some(ref m) = ai_config.deep.model {
        OllamaTranslator::new(&ai_config.deep.endpoint, m)
    } else {
        OllamaTranslator::choosing(&ai_config.deep.endpoint)
    };
    let deep_result_tx = result_tx;
    std::thread::spawn(move || {
        let mut translator = deep_translator;
        let _ = translator.resolve_model();
        // Deep analysis cache — separate from translation cache per ADR-0005.
        let mut explanations = AnalysisCache::new(ANALYSIS_ENTRIES);
        while let Ok(job) = deep_rx.recv() {
            // Check for config updates (non-blocking).
            while let Ok((ep, m)) = deep_config_rx.try_recv() {
                translator = match m {
                    Some(ref model) => OllamaTranslator::new(&ep, model),
                    None => OllamaTranslator::choosing(&ep),
                };
                let _ = translator.resolve_model();
            }
            // Use the production seam: build_request creates the DeepRequest
            // from the job's fields, and into_result produces TransResult
            // with the job's dispatch-time target_language.
            let (deep_req, _result_word) = job.build_request();
            // Cache key uses model + prompt version for ADR-0005 separation.
            let key = CacheKey::for_analysis(&deep_req, translator.model(), DEEP_PROMPT_VERSION);
            // Check cache first — avoid re-generating the same analysis.
            if let Some(hit) = explanations.get(&key) {
                let _ = deep_result_tx.send(job.into_result(Ok(hit)));
                continue;
            }
            let result = translator.deep(&deep_req);
            // Cache successful results for future reuse.
            if let Ok(ref analysis) = result {
                explanations.insert(key, analysis.clone());
            }
            let _ = deep_result_tx
                .send(job.into_result(result.map_err(|e| e.user_message().to_owned())));
        }
    });

    // Hover machine.
    let dwell = Duration::from_millis(dwell_ms);
    let hover_config = HoverConfig {
        dwell,
        ..HoverConfig::default()
    };
    let mut machine = HoverMachine::new(hover_config);
    let mut current_card: Option<(Card, String, String)> = None;
    let mut current_focused_word: Option<String> = None;
    let mut current_app: Option<String> = None;
    let mut pending_app: Option<String> = None;
    let mut current_anchor = ScreenRect::new(0, 0, 0, 0);
    let mut current_generation: u64 = 0;
    let mut paused = false;
    // Track whether we have already requested Screen Recording permission
    // during this process. Avoids showing the system dialog more than once.
    let mut screen_recording_requested = false;
    let mut quit_requested = false;
    let mut tally = Tally::default();
    let mut settings_gen: u64 = 0;

    // Ctrl+C handler.
    let (tx, rx) = mpsc::channel::<()>();
    spawn_ctrlc_handler(tx);

    let poll = Duration::from_millis(POINTER_POLL_MS);
    let start = Instant::now();

    loop {
        // Receive translation results (non-blocking).
        while let Ok(result) = result_rx.try_recv() {
            match result {
                TransResult::Quick {
                    sentence,
                    display_text,
                    result,
                    from_pixels,
                    app,
                    focused_word,
                    generation,
                    anchor,
                    target_language,
                } => {
                    if generation != current_generation {
                        continue;
                    }
                    // Use the app/focus from the job itself, not global pending state.
                    current_app = app;
                    current_focused_word = focused_word;
                    match result {
                        Ok(translation) => {
                            let mut card = Card::translated(&display_text, &translation);
                            if from_pixels {
                                card = card.read_from_the_screen();
                            }
                            show_card_at(
                                &overlay,
                                &card,
                                &anchor,
                                PresentationKind::Reading,
                                app.as_deref(),
                                &target_language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                            current_card = Some((card, sentence, target_language));
                            tally.card();
                        }
                        Err(msg) => {
                            let card = Card {
                                original: display_text.clone(),
                                body: CardBody::Problem(msg),
                                footer: String::new(),
                                metadata: None,
                            };
                            show_card_at(
                                &overlay,
                                &card,
                                &anchor,
                                PresentationKind::Feedback,
                                None,
                                &target_language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                            current_card = Some((card, sentence, target_language));
                            tally.card();
                        }
                    }
                }
                TransResult::Deep {
                    sentence,
                    result,
                    app,
                    focused_word,
                    generation,
                    anchor,
                    target_language,
                } => {
                    if generation != current_generation {
                        continue;
                    }
                    // Use the app/focus from the job itself, not global pending state.
                    current_app = app;
                    current_focused_word = focused_word;
                    match result {
                        Ok(analysis) => {
                            let card = Card::analysed(&sentence, &analysis);
                            show_card_at(
                                &overlay,
                                &card,
                                &anchor,
                                PresentationKind::Reading,
                                app.as_deref(),
                                &target_language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                            current_card = Some((card, sentence, target_language));
                            tally.card();
                        }
                        Err(msg) => {
                            let card = Card {
                                original: sentence.clone(),
                                body: CardBody::Problem(format!("analysis failed: {}", msg)),
                                footer: String::new(),
                                metadata: None,
                            };
                            show_card_at(
                                &overlay,
                                &card,
                                &anchor,
                                PresentationKind::Feedback,
                                None,
                                &target_language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                            current_card = Some((card, sentence, target_language));
                            tally.card();
                        }
                    }
                }
            }
        }

        // Push diagnostics snapshot to the shared state so the webview
        // can read fresh data when the user opens diagnostics.
        update_diagnostics(
            &shared_diagnostics,
            &tally,
            &ocr_mode,
            &language,
            &ai_config,
        );

        // Drain commands from menu bar and global hotkeys.
        let mut pending_commands: Vec<ProductCommand> = Vec::new();

        if let Some(ref mb) = menubar {
            if mb.poll_quit() {
                pending_commands.push(ProductCommand::Quit);
            }
            if mb.poll_pause_toggle() {
                pending_commands.push(ProductCommand::PauseToggle);
            }
            if mb.poll_deep() {
                pending_commands.push(ProductCommand::Deep);
            }
            if mb.poll_copy() {
                pending_commands.push(ProductCommand::Copy);
            }
            if mb.poll_save() {
                pending_commands.push(ProductCommand::Save);
            }
            if mb.poll_ocr() {
                pending_commands.push(ProductCommand::ManualOcr);
            }
            if mb.poll_open_settings() {
                pending_commands.push(ProductCommand::OpenSettings);
            }
            if mb.poll_open_library() {
                pending_commands.push(ProductCommand::OpenLibrary);
            }
        }

        if hotkeys_available {
            loop {
                let cmd = hotkey::poll();
                if cmd == ProductCommand::None {
                    break;
                }
                pending_commands.push(cmd);
            }
        }

        // Dispatch commands. Quit and PauseToggle work even when paused;
        // all other commands are blocked (matching Windows behavior).
        for cmd in pending_commands {
            if paused && !cmd.works_when_paused() {
                continue;
            }
            match cmd {
                ProductCommand::Quit => {
                    quit_requested = true;
                }
                ProductCommand::PauseToggle => {
                    paused = !paused;
                    if let Some(ref o) = overlay {
                        if paused {
                            o.hide();
                        }
                    }
                    if let Some(ref mb) = menubar {
                        mb.set_title(if paused { "Lens ⏸" } else { "Lens" });
                    }
                }
                ProductCommand::Deep => {
                    if let Some((ref card, ref sentence, ref _target_lang)) = current_card {
                        // Extract the on-screen translation if the current card
                        // is a finished translation. This grounds the deep
                        // analysis in what the reader is actually looking at
                        // (ADR-0005), matching Windows behavior.
                        let translation = match &card.body {
                            lens_core::card::CardBody::Translation(text) => Some(text.clone()),
                            _ => None,
                        };

                        // Read current profile and language from live settings.
                        let (target_language, profile) = {
                            let s = shared_settings.lock().unwrap().current().clone();
                            let p =
                                lens_core::analyse::Profile::named(&s.profile).unwrap_or_default();
                            (language.clone(), p)
                        };

                        current_generation += 1;
                        let _ = deep_tx.send(DeepJob::Deep {
                            sentence: sentence.clone(),
                            translation,
                            target_language,
                            profile,
                            app: current_app.clone(),
                            focused_word: current_focused_word.clone(),
                            generation: current_generation,
                            anchor: current_anchor,
                        });
                    }
                }
                ProductCommand::Copy => {
                    if let Some((ref card, ref _sentence, ref _target_lang)) = current_card {
                        if let Some(text) = card.for_the_clipboard() {
                            if !clipboard::put_text(&text) {
                                eprintln!("copy  the clipboard would not open");
                                let err_card = Card::not_readable("클립보드를 열지 못했습니다");
                                show_card_at(
                                    &overlay,
                                    &err_card,
                                    &current_anchor,
                                    PresentationKind::Feedback,
                                    None,
                                    &language,
                                    &shared_recent,
                                    &shared_library,
                                    &shared_settings,
                                );
                                continue;
                            }
                            // Visual feedback: mark the card as copied,
                            // matching Windows behavior.
                            let copied_card = card.clone().copied();
                            show_card_at(
                                &overlay,
                                &copied_card,
                                &current_anchor,
                                PresentationKind::Feedback,
                                None,
                                &language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                        }
                    }
                }
                ProductCommand::Save => {
                    if let Some((ref card, ref _sentence, ref target_lang)) = current_card {
                        let lib_guard = shared_library.lock().unwrap();
                        execute_save(
                            card,
                            current_app.as_deref(),
                            &shared_save_path,
                            target_lang,
                            lib_guard.library(),
                        );
                    }
                }
                ProductCommand::ManualOcr => {
                    if let Some(ref ocr) = ocr_acquirer {
                        if let Some(ref o) = overlay {
                            o.hide();
                        }
                        let point = mouse_position();
                        // Check Screen Recording permission before OCR.
                        // Manual OCR is an explicit user action, so this is
                        // the right moment to request permission if needed.
                        if !lens_platform_macos::capture::is_available() {
                            if !screen_recording_requested {
                                lens_platform_macos::capture::request();
                                screen_recording_requested = true;
                            }
                            let card = Card {
                                original: String::new(),
                                body: CardBody::Problem(
                                    "Screen Recording permission required. Grant in System Settings, then restart Mori Lens.".to_owned(),
                                ),
                                footer: String::new(),
                                metadata: None,
                            };
                            show_card_at(
                                &overlay,
                                &card,
                                &current_anchor,
                                PresentationKind::Feedback,
                                None,
                                &language,
                                &shared_recent,
                                &shared_library,
                                &shared_settings,
                            );
                            continue;
                        }
                        current_generation += 1;
                        let gen = lens_core::hover::Generation(current_generation);
                        let ctx = AcquireContext::new(gen);
                        match ocr.acquire(point, &ctx) {
                            Ok(target) => {
                                let sentence = target.segmented.sentence_text().to_owned();
                                if !sentence.is_empty() {
                                    let request = QuickRequest::new(&sentence, &language);
                                    let request = if let Some(word) = target.segmented.word_text() {
                                        request.with_focus(word)
                                    } else {
                                        request
                                    };
                                    current_generation += 1;
                                    if let Some(a) = target.anchor() {
                                        current_anchor = a;
                                    }
                                    let _ = quick_tx.send(QuickJob::Quick {
                                        request,
                                        sentence: sentence.clone(),
                                        display_text: sentence.clone(),
                                        from_pixels: true,
                                        app: target.app.process_name.clone(),
                                        focused_word: target
                                            .segmented
                                            .word_text()
                                            .map(str::to_owned),
                                        generation: current_generation,
                                        anchor: current_anchor,
                                    });
                                    // Manual OCR: use the app from the OCR target itself,
                                    // not pending_app from a previous hover.
                                    current_app = target.app.process_name.clone();
                                    current_focused_word =
                                        target.segmented.word_text().map(str::to_owned);
                                    current_card = Some((
                                        Card {
                                            original: sentence.clone(),
                                            body: CardBody::Waiting("reading...".to_owned()),
                                            footer: String::new(),
                                            metadata: None,
                                        },
                                        sentence,
                                        language.clone(),
                                    ));
                                }
                            }
                            Err(refusal) => {
                                eprintln!("ocr   no reading [{}]", refusal.reason());
                                let card = Card::not_readable(refusal.reason());
                                show_card_at(
                                    &overlay,
                                    &card,
                                    &current_anchor,
                                    PresentationKind::Feedback,
                                    None,
                                    &language,
                                    &shared_recent,
                                    &shared_library,
                                    &shared_settings,
                                );
                            }
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
            // Explicit shutdown cleanup before exiting.
            // Order: stop accepting input → hide UI → release model.
            if hotkeys_available {
                hotkey::remove();
            }
            if let Some(ref o) = overlay {
                o.hide();
            }
            // Release the Ollama model so it doesn't stay resident after exit.
            // Best-effort: if Ollama is unreachable this is silently skipped.
            // ADR-0005: release the quick lane's model (the primary hover path).
            let release_translator = if let Some(ref m) = ai_config.quick.model {
                OllamaTranslator::new(&ai_config.quick.endpoint, m)
            } else {
                OllamaTranslator::choosing(&ai_config.quick.endpoint)
            };
            release_translator.release();

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

        // Live settings: detect changes from Settings UI.
        // Check generation first (cheap u64 comparison) to avoid cloning
        // the entire Settings struct on every 25ms tick when nothing changed.
        {
            let guard = shared_settings.lock().unwrap();
            let gen = guard.generation();
            if gen == settings_gen {
                drop(guard);
            } else {
                settings_gen = gen;
                let s = guard.current().clone();
                // ADR-0005: read per-lane config from the schema.
                let new_ai_config = AiRuntimeConfig::from_schema(guard.schema());
                drop(guard);

                let new_lang = s.language.clone();
                let new_dwell = s.dwell_ms;
                let new_ocr = OcrMode::named(&s.ocr).unwrap_or(OcrMode::Manual);
                let new_policy = {
                    let mut p = s.policy();
                    // --auto must be re-applied on every reload, matching Windows behavior.
                    if auto {
                        p.set_default(lens_core::policy::AppTier::Allow);
                    }
                    p.deny_self("lens");
                    p
                };

                // ADR-0005: compare quick and deep independently.
                // Each lane gets its own config update only when its
                // endpoint or model changed.
                if new_ai_config.quick != ai_config.quick {
                    let _ = quick_config_tx.send((
                        new_ai_config.quick.endpoint.clone(),
                        new_ai_config.quick.model.clone(),
                    ));
                }
                if new_ai_config.deep != ai_config.deep {
                    let _ = deep_config_tx.send((
                        new_ai_config.deep.endpoint.clone(),
                        new_ai_config.deep.model.clone(),
                    ));
                }
                ai_config = new_ai_config;
                // Language change → affects translation target for new requests.
                if new_lang != language {
                    language = new_lang;
                }
                // Dwell change → rebuild hover config.
                if new_dwell != dwell_ms {
                    dwell_ms = new_dwell;
                    machine = HoverMachine::new(HoverConfig {
                        dwell: Duration::from_millis(dwell_ms),
                        ..HoverConfig::default()
                    });
                }
                // OCR mode change → affects whether OCR fallback runs.
                if new_ocr != ocr_mode {
                    ocr_mode = new_ocr;
                }
                // Policy change → Allow→Deny must take effect before next acquisition.
                if new_policy != policy {
                    policy = new_policy;
                }
                // Hover scope change → affects whether the card shows a word or sentence.
                if s.hover_scope != hover_scope {
                    hover_scope = s.hover_scope.clone();
                }
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
                // Pass the real Option/Alt modifier state so Modifier tier works correctly.
                if !policy.allows(process_name.as_deref(), overlay::option_held()) {
                    // Denied by policy: skip both AX and OCR.
                    if let Some(ref o) = overlay {
                        o.hide();
                    }
                    continue;
                }
                // Carry app identity alongside the acquisition.
                // current_app is updated only when current_card is set,
                // so Save always matches the card's actual source.
                pending_app = process_name.clone();

                // AX acquisition first.
                let acquire_result = if ax_trusted {
                    let ctx = AcquireContext::new(generation);
                    ax_acquirer.acquire(point, &ctx)
                } else {
                    Err(AcquireError::Unavailable)
                };

                // OCR fallback — only in Automatic mode.
                // Manual mode requires explicit user OCR request (ProductCommand::ManualOcr).
                // Off mode never runs OCR. This matches the core OcrMode contract
                // and Windows behavior (win.rs:2028).
                let target = match acquire_result {
                    Ok(target) => target,
                    Err(e) if e.ocr_can_help() && ocr_mode == OcrMode::Automatic => {
                        if let Some(ref ocr) = ocr_acquirer {
                            // Check Screen Recording permission before OCR.
                            // If not granted, request once and show a card
                            // explaining the user must restart after granting.
                            if !lens_platform_macos::capture::is_available() {
                                if !screen_recording_requested {
                                    lens_platform_macos::capture::request();
                                    screen_recording_requested = true;
                                }
                                let card = Card {
                                    original: String::new(),
                                    body: CardBody::Problem(
                                        "Screen Recording permission required. Grant in System Settings, then restart Mori Lens.".to_owned(),
                                    ),
                                    footer: String::new(),
                                };
                                show_card_at(
                                    &overlay,
                                    &card,
                                    &ScreenRect::new(point.x, point.y, 1, 1),
                                    PresentationKind::Feedback,
                                    None,
                                    &language,
                                    &shared_recent,
                                    &shared_library,
                                    &shared_settings,
                                );
                                continue;
                            }
                            let ctx = AcquireContext::new(generation);
                            match ocr.acquire(point, &ctx) {
                                Ok(target) => target,
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
                // Derive OCR provenance from the acquisition source.
                // AX accessibility → from_pixels = false
                // OCR (manual or automatic fallback) → from_pixels = true
                let from_pixels = target.source == AcquisitionSource::Ocr;
                // Carry the focused word through to Deep analysis,
                // matching Windows behavior (win.rs:2017).
                current_focused_word = target.segmented.word_text().map(str::to_owned);
                if sentence.is_empty() {
                    if let Some(ref o) = overlay {
                        o.hide();
                    }
                    continue;
                }

                // Scope: which text the card shows. The model always
                // receives the sentence. Ctrl toggles the setting's default.
                let word_scope = match hover_scope.as_str() {
                    "word" => !overlay::ctrl_held(),
                    _ => overlay::ctrl_held(),
                };
                let display_text = if word_scope {
                    target
                        .segmented
                        .word_text()
                        .map(str::trim)
                        .filter(|w| !w.is_empty())
                        .unwrap_or(&sentence)
                        .to_owned()
                } else {
                    sentence.clone()
                };

                // Language decision + translation.
                match decide(&sentence, &language) {
                    lens_core::language::Decision::AlreadyInTarget => {
                        let card = Card {
                            original: display_text.clone(),
                            body: CardBody::Problem("already in your language".to_owned()),
                            footer: String::new(),
                            metadata: None,
                        };
                        show_card(
                            &overlay,
                            &card,
                            &target,
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &shared_library,
                            &shared_settings,
                        );
                        current_app = pending_app.clone();
                        current_card = Some((card, sentence, language.clone()));
                    }
                    _ => {
                        let mut request = QuickRequest::new(&sentence, &language);
                        if let Some(word) = target.segmented.word_text() {
                            request = request.with_focus(word);
                        }

                        // Dispatch async translation job.
                        current_generation += 1;
                        if let Some(a) = target.anchor() {
                            current_anchor = a;
                        }
                        let _ = quick_tx.send(QuickJob::Quick {
                            request,
                            sentence: sentence.clone(),
                            display_text: display_text.clone(),
                            from_pixels,
                            app: pending_app.clone(),
                            focused_word: current_focused_word.clone(),
                            generation: current_generation,
                            anchor: current_anchor,
                        });
                        // Store target for when result arrives.
                        current_app = pending_app.clone();
                        current_card = Some((
                            Card {
                                original: display_text,
                                body: CardBody::Waiting("translating...".to_owned()),
                                footer: String::new(),
                                metadata: None,
                            },
                            sentence,
                            language.clone(),
                        ));
                    }
                }
            }
            Some(HoverEvent::Left { .. }) => {
                if let Some(ref o) = overlay {
                    o.hide();
                }
                current_card = None;
                // Invalidate any in-flight translation results.
                // Without this, a late-arriving result would re-show the card
                // after the pointer already left.
                current_generation = current_generation.saturating_add(1);
            }
            None => {}
        }

        std::thread::sleep(poll);
    }
}

// -- Helper functions -------------------------------------------------------

/// Save using the shared cross-platform save policy.
///
/// Delegates to `keeping::save_card()` which enforces eligibility (only
/// Translation/Analysis), populates metadata, writes both saved.md and
/// library.db, and serializes concurrent saves.
///
/// The model name comes from the card's metadata (set at construction time
/// from the actual translation result), not from the current configuration.
fn execute_save(
    card: &Card,
    app: Option<&str>,
    save_path: &std::path::Path,
    language: &str,
    library: &lens_storage::Library,
) {
    match crate::keeping::save_card(card, app, save_path, Some(language.to_owned()), library) {
        Ok(crate::keeping::StructuredSave::Inserted(id)) => {
            eprintln!("  save      saved as library item #{id}")
        }
        Ok(crate::keeping::StructuredSave::Duplicate(_existing_id)) => {
            eprintln!("  save      file saved, already in library (duplicate)")
        }
        Ok(crate::keeping::StructuredSave::Failed(_)) => {
            eprintln!("  save      file saved, library database write failed")
        }
        Err(crate::keeping::NotKept::NothingToKeep) => {
            eprintln!("  save      nothing to keep (waiting/problem card refused)")
        }
        Err(e) => eprintln!("  save      failed: {}", e.user_message()),
    }
}

/// Whether a card is a new reading or a feedback card.
///
/// Reading cards (translation/analysis results) are recorded to session-recent
/// and persistent history. Feedback cards (copy/save confirmations, error
/// messages, waiting indicators) are not.
enum PresentationKind {
    Reading,
    Feedback,
}

fn show_card(
    overlay: &Option<Overlay>,
    card: &Card,
    target: &lens_core::acquire::TextTarget,
    kind: PresentationKind,
    source_app: Option<&str>,
    target_language: &str,
    shared_recent: &SharedRecent,
    shared_library: &SharedLibrary,
    shared_settings: &SharedSettings,
) {
    let anchor = match target.anchor() {
        Some(a) => a,
        None => return,
    };
    show_card_at(
        &overlay,
        card,
        &anchor,
        kind,
        source_app,
        target_language,
        shared_recent,
        shared_library,
        shared_settings,
    );
}

fn show_card_at(
    overlay: &Option<Overlay>,
    card: &Card,
    anchor: &ScreenRect,
    kind: PresentationKind,
    source_app: Option<&str>,
    target_language: &str,
    shared_recent: &SharedRecent,
    shared_library: &SharedLibrary,
    shared_settings: &SharedSettings,
) {
    let Some(overlay) = overlay else { return };
    let (x, y, _scale) = physical_to_logical(ScreenPoint::new(anchor.x, anchor.bottom() + 8));
    let width = {
        let (w, _, _) = physical_to_logical(ScreenPoint::new(anchor.width, 0));
        w.max(200.0).min(600.0)
    };
    let height = 80.0;
    overlay.show_text(
        &card.original,
        &card_body_to_text(&card.body),
        x,
        y,
        width,
        height,
    );
    match kind {
        PresentationKind::Reading => {
            record_presented_reading(
                card,
                source_app,
                target_language,
                shared_recent,
                shared_library,
                shared_settings,
            );
        }
        PresentationKind::Feedback => {}
    }
}

/// Record a presented reading to session-recent and persistent history.
///
/// Called when `PresentationKind::Reading`. Feedback cards should not call this.
fn record_presented_reading(
    card: &Card,
    source_app: Option<&str>,
    target_language: &str,
    recent: &SharedRecent,
    library: &SharedLibrary,
    settings: &SharedSettings,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let app_name = source_app.map(str::to_owned);
    let answer_text = match &card.body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Analysis {
            translation: Some(t),
            ..
        } => t.clone(),
        _ => String::new(),
    };
    if !answer_text.is_empty() {
        let recent_card = lens_core::recent::RecentCard {
            original: card.original.clone(),
            answer: answer_text.clone(),
            shown_at: now,
            app: app_name.clone(),
        };
        recent.lock().unwrap().record(recent_card);
    }

    // Record to persistent history (ADR-0014 Tier 3).
    {
        let current = settings.lock().unwrap().current().clone();
        if current.history {
            let excluded = app_name
                .as_ref()
                .map(|name| {
                    current
                        .history_excluded
                        .iter()
                        .any(|ex| name.to_lowercase().contains(&ex.to_lowercase()))
                })
                .unwrap_or(false);
            if !excluded && !answer_text.is_empty() {
                let kind = match &card.body {
                    CardBody::Analysis { .. } => "analysis",
                    _ => "translation",
                };
                let meta = card.metadata();
                let provenance = match meta.map(|m| m.provenance) {
                    Some(lens_core::card::CardProvenance::Ocr) => "ocr",
                    _ => "accessibility",
                };
                let provider_scope = match meta.and_then(|m| m.provider_scope) {
                    Some(lens_core::card::CardProviderScope::External) => "external",
                    Some(lens_core::card::CardProviderScope::Local) => "local",
                    None => "unknown",
                };
                let model = meta.and_then(|m| m.model.clone());
                let stored_app = if current.history_store_app {
                    app_name
                } else {
                    None
                };
                let entry = lens_storage::HistoryEntry {
                    id: 0,
                    original: card.original.clone(),
                    answer: answer_text,
                    kind: kind.to_owned(),
                    source_language: None,
                    target_language: Some(target_language.to_owned()),
                    model,
                    app: stored_app,
                    provenance: provenance.to_owned(),
                    provider_scope: provider_scope.to_owned(),
                    created_at: now,
                };
                let lib = library.lock().unwrap();
                if let Err(e) = lib.library().record_history(&entry) {
                    eprintln!("  history   record failed: {e}");
                }
            }
        }
    }
}

fn card_body_to_text(body: &CardBody) -> String {
    match body {
        CardBody::Translation(t) => t.clone(),
        CardBody::Problem(p) => p.clone(),
        CardBody::Waiting(w) => w.clone(),
        CardBody::Analysis {
            translation, rows, ..
        } => {
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

    std::thread::spawn(move || loop {
        if received.load(Ordering::SeqCst) {
            let _ = tx.send(());
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    });
}

/// Update the shared diagnostics snapshot from the product loop.
fn update_diagnostics(
    shared: &SharedDiagnostics,
    tally: &Tally,
    ocr_mode: &OcrMode,
    language: &str,
    ai_config: &AiRuntimeConfig,
) {
    let mut snapshot = shared.lock().unwrap();
    snapshot.build = format!("{} macOS", env!("CARGO_PKG_VERSION"));
    // ADR-0005: Quick and Deep have separate engines.
    snapshot.configuration = vec![
        ("language".into(), language.into()),
        (
            "quick".into(),
            format!(
                "{} · {}",
                ai_config.quick.endpoint,
                ai_config.quick.model.as_deref().unwrap_or("(auto)"),
            ),
        ),
        (
            "deep".into(),
            format!(
                "{} · {}",
                ai_config.deep.endpoint,
                ai_config.deep.model.as_deref().unwrap_or("(auto)"),
            ),
        ),
    ];
    snapshot.cards = tally.cards();
    snapshot.settled_hovers = tally.settled();
    snapshot.refusals = tally
        .refusals()
        .into_iter()
        .map(|(reason, count)| (reason.to_owned(), count))
        .collect();
    snapshot.ocr_mode = format!("{ocr_mode:?}");
}
