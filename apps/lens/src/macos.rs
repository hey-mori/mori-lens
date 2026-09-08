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

use lens_core::acquire::{AcquireContext, AcquireError, TextAcquirer};
use lens_core::analyse::Analyser;
use lens_core::cache::{CacheKey, TranslationCache};
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
    let shared_settings: SharedSettings =
        Arc::new(Mutex::new(settings_service::load_or_migrate(&data_dir)));

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
        let settings = shared_settings.lock().unwrap().current().clone();
        println!("Mori Lens · macOS");
        println!("  settings  {:?}", data_dir);
        println!("  language  {}", settings.language);
        println!("  endpoint  {}", settings.endpoint);
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
#[allow(clippy::too_many_arguments)]
fn product_loop(
    tauri_handle: AppHandle,
    shared_diagnostics: SharedDiagnostics,
    shared_settings: SharedSettings,
    _shared_library: SharedLibrary,
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
    mut endpoint: String,
    mut model: Option<String>,
    mut dwell_ms: u64,
    mut ocr_mode: OcrMode,
    mut policy: AppPolicy,
) {
    // Translation job types for async dispatch.
    enum QuickJob {
        Quick {
            request: QuickRequest,
            sentence: String,
            from_pixels: bool,
            generation: u64,
            key: CacheKey,
            anchor: ScreenRect,
        },
    }
    enum DeepJob {
        Deep {
            sentence: String,
            translation: Option<String>,
            target_language: String,
            profile: lens_core::analyse::Profile,
            focused_word: Option<String>,
            generation: u64,
            anchor: ScreenRect,
        },
    }
    enum TransResult {
        Quick {
            sentence: String,
            result: Result<lens_core::translate::QuickTranslation, String>,
            from_pixels: bool,
            generation: u64,
            key: CacheKey,
            anchor: ScreenRect,
        },
        Deep {
            sentence: String,
            result: Result<lens_core::analyse::DeepAnalysis, String>,
            generation: u64,
            anchor: ScreenRect,
        },
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
    let quick_translator = if let Some(ref m) = model {
        OllamaTranslator::new(&endpoint, m)
    } else {
        OllamaTranslator::choosing(&endpoint)
    };
    let quick_cache = cache.clone();
    let quick_result_tx = result_tx.clone();
    std::thread::spawn(move || {
        let mut translator = quick_translator;
        let _ = translator.resolve_model();
        while let Ok(job) = quick_rx.recv() {
            // Check for config updates (non-blocking).
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
            match current {
                QuickJob::Quick {
                    request,
                    sentence,
                    from_pixels,
                    generation,
                    key,
                    anchor,
                } => {
                    let result = translator.quick(&request);
                    if let Ok(ref t) = result {
                        if let Ok(mut c) = quick_cache.lock() {
                            c.insert(key.clone(), t.clone());
                        }
                    }
                    let _ = quick_result_tx.send(TransResult::Quick {
                        sentence,
                        result: result.map_err(|e| e.user_message().to_owned()),
                        from_pixels,
                        generation,
                        key,
                        anchor,
                    });
                }
            }
        }
    });

    // Deep lane: dedicated thread for deep analysis.
    // Single-flight: processes one Deep at a time, in order.
    let deep_translator = if let Some(ref m) = model {
        OllamaTranslator::new(&endpoint, m)
    } else {
        OllamaTranslator::choosing(&endpoint)
    };
    let deep_result_tx = result_tx;
    std::thread::spawn(move || {
        let mut translator = deep_translator;
        let _ = translator.resolve_model();
        while let Ok(job) = deep_rx.recv() {
            // Check for config updates (non-blocking).
            while let Ok((ep, m)) = deep_config_rx.try_recv() {
                translator = match m {
                    Some(ref model) => OllamaTranslator::new(&ep, model),
                    None => OllamaTranslator::choosing(&ep),
                };
                let _ = translator.resolve_model();
            }
            match job {
                DeepJob::Deep {
                    sentence,
                    translation,
                    target_language,
                    profile,
                    focused_word,
                    generation,
                    anchor,
                } => {
                    let deep_req = lens_core::analyse::DeepRequest {
                        sentence: sentence.clone(),
                        word: focused_word,
                        target_language,
                        translation,
                        profile,
                    };
                    let result = translator.deep(&deep_req);
                    let _ = deep_result_tx.send(TransResult::Deep {
                        sentence,
                        result: result.map_err(|e| e.user_message().to_owned()),
                        generation,
                        anchor,
                    });
                }
            }
        }
    });

    // Hover machine.
    let dwell = Duration::from_millis(dwell_ms);
    let hover_config = HoverConfig {
        dwell,
        ..HoverConfig::default()
    };
    let mut machine = HoverMachine::new(hover_config);
    let mut current_card: Option<(Card, String)> = None;
    let mut current_focused_word: Option<String> = None;
    let mut current_app: Option<String> = None;
    let mut pending_app: Option<String> = None;
    let mut current_anchor = ScreenRect::new(0, 0, 0, 0);
    let mut current_generation: u64 = 0;
    let mut paused = false;
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
                    result,
                    from_pixels,
                    generation,
                    key: _,
                    anchor,
                } => {
                    if generation != current_generation {
                        continue;
                    }
                    // Sync app provenance with the card about to be created.
                    current_app = pending_app.clone();
                    match result {
                        Ok(translation) => {
                            let mut card = Card::translated(&sentence, &translation);
                            if from_pixels {
                                card = card.read_from_the_screen();
                            }
                            show_card_at(&overlay, &card, &anchor);
                            current_card = Some((card, sentence));
                            tally.card();
                        }
                        Err(msg) => {
                            let card = Card {
                                original: sentence.clone(),
                                body: CardBody::Problem(msg),
                                footer: String::new(),
                            };
                            show_card_at(&overlay, &card, &anchor);
                            current_card = Some((card, sentence));
                            tally.card();
                        }
                    }
                }
                TransResult::Deep {
                    sentence,
                    result,
                    generation,
                    anchor,
                } => {
                    if generation != current_generation {
                        continue;
                    }
                    current_app = pending_app.clone();
                    match result {
                        Ok(analysis) => {
                            let card = Card::analysed(&sentence, &analysis);
                            show_card_at(&overlay, &card, &anchor);
                            current_card = Some((card, sentence));
                            tally.card();
                        }
                        Err(msg) => {
                            let card = Card {
                                original: sentence.clone(),
                                body: CardBody::Problem(format!("analysis failed: {}", msg)),
                                footer: String::new(),
                            };
                            show_card_at(&overlay, &card, &anchor);
                            current_card = Some((card, sentence));
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
            &endpoint,
            &model,
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
                    if let Some((ref card, ref sentence)) = current_card {
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
                            focused_word: current_focused_word.clone(),
                            generation: current_generation,
                            anchor: current_anchor,
                        });
                    }
                }
                ProductCommand::Copy => {
                    if let Some((ref card, _)) = current_card {
                        if let Some(text) = card.for_the_clipboard() {
                            if !clipboard::put_text(&text) {
                                eprintln!("copy  the clipboard would not open");
                                let err_card = Card::not_readable("클립보드를 열지 못했습니다");
                                show_card_at(&overlay, &err_card, &current_anchor);
                                continue;
                            }
                            // Visual feedback: mark the card as copied,
                            // matching Windows behavior.
                            let copied_card = card.clone().copied();
                            show_card_at(&overlay, &copied_card, &current_anchor);
                        }
                    }
                }
                ProductCommand::Save => {
                    if let Some((ref card, ref _sentence)) = current_card {
                        execute_save(
                            card,
                            current_app.as_deref(),
                            &shared_save_path,
                            &language,
                            &model,
                        );
                    }
                }
                ProductCommand::ManualOcr => {
                    if let Some(ref ocr) = ocr_acquirer {
                        if let Some(ref o) = overlay {
                            o.hide();
                        }
                        let point = mouse_position();
                        current_generation += 1;
                        let gen = lens_core::hover::Generation(current_generation);
                        let ctx = AcquireContext::new(gen);
                        match ocr.acquire(point, &ctx) {
                            Ok(target) => {
                                let sentence = target.segmented.sentence_text().to_owned();
                                if !sentence.is_empty() {
                                    let request = QuickRequest::new(&sentence, &language);
                                    let key =
                                        CacheKey::of(&request, model.as_deref().unwrap_or(""), 1);
                                    current_generation += 1;
                                    if let Some(a) = target.anchor() {
                                        current_anchor = a;
                                    }
                                    let _ = quick_tx.send(QuickJob::Quick {
                                        request,
                                        sentence: sentence.clone(),
                                        from_pixels: false,
                                        generation: current_generation,
                                        key,
                                        anchor: current_anchor,
                                    });
                                    current_app = pending_app.clone();
                                    current_card = Some((
                                        Card {
                                            original: sentence.clone(),
                                            body: CardBody::Waiting("reading...".to_owned()),
                                            footer: String::new(),
                                        },
                                        sentence,
                                    ));
                                }
                            }
                            Err(refusal) => {
                                eprintln!("ocr   no reading [{}]", refusal.reason());
                                let card = Card::not_readable(refusal.reason());
                                show_card_at(&overlay, &card, &current_anchor);
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
            let release_translator = if let Some(ref m) = model {
                OllamaTranslator::new(&endpoint, m)
            } else {
                OllamaTranslator::choosing(&endpoint)
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
                drop(guard);

                let new_lang = s.language.clone();
                let new_ep = s.endpoint.clone();
                let new_model = s.model.clone();
                let new_dwell = s.dwell_ms;
                let new_ocr = OcrMode::named(&s.ocr).unwrap_or(OcrMode::Manual);
                let new_policy = {
                    let mut p = s.policy();
                    p.deny_self("lens");
                    p
                };

                // Endpoint/model change → rebuild translators on both lanes.
                if new_ep != endpoint || new_model != model {
                    let _ = quick_config_tx.send((new_ep.clone(), new_model.clone()));
                    let _ = deep_config_tx.send((new_ep.clone(), new_model.clone()));
                    endpoint = new_ep;
                    model = new_model;
                }
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
                let (target, _from_ocr) = match acquire_result {
                    Ok(target) => (target, false),
                    Err(e) if e.ocr_can_help() && ocr_mode == OcrMode::Automatic => {
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
                // Carry the focused word through to Deep analysis,
                // matching Windows behavior (win.rs:2017).
                current_focused_word = target.segmented.word_text().map(str::to_owned);
                if sentence.is_empty() {
                    if let Some(ref o) = overlay {
                        o.hide();
                    }
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
                        current_app = pending_app.clone();
                        current_card = Some((card, sentence));
                    }
                    _ => {
                        let request = QuickRequest::new(&sentence, &language);
                        let key = {
                            let _c = cache.lock().unwrap();
                            CacheKey::of(&request, model.as_deref().unwrap_or(""), 1)
                        };

                        // Check cache (lock briefly).
                        {
                            let mut c = cache.lock().unwrap();
                            if let Some(cached) = c.get(&key) {
                                let card = Card::translated(&sentence, &cached);
                                show_card(&overlay, &card, &target);
                                current_app = pending_app.clone();
                                current_card = Some((card, sentence));
                                continue;
                            }
                        }

                        // Dispatch async translation job.
                        current_generation += 1;
                        if let Some(a) = target.anchor() {
                            current_anchor = a;
                        }
                        let _ = quick_tx.send(QuickJob::Quick {
                            request,
                            sentence: sentence.clone(),
                            from_pixels: true,
                            generation: current_generation,
                            key,
                            anchor: current_anchor,
                        });
                        // Store target for when result arrives.
                        current_app = pending_app.clone();
                        current_card = Some((
                            Card {
                                original: sentence.clone(),
                                body: CardBody::Waiting("translating...".to_owned()),
                                footer: String::new(),
                            },
                            sentence,
                        ));
                    }
                }
            }
            Some(HoverEvent::Left { .. }) => {
                if let Some(ref o) = overlay {
                    o.hide();
                }
                current_card = None;
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
fn execute_save(
    card: &Card,
    app: Option<&str>,
    save_path: &std::path::Path,
    language: &str,
    model: &Option<String>,
) {
    match crate::keeping::save_card(
        card,
        app,
        save_path,
        Some(language.to_owned()),
        model.clone(),
    ) {
        Ok(Some(id)) => eprintln!("  save      saved as library item #{id}"),
        Ok(None) => eprintln!("  save      nothing to keep (duplicate)"),
        Err(crate::keeping::NotKept::NothingToKeep) => {
            eprintln!("  save      nothing to keep (waiting/problem card refused)")
        }
        Err(e) => eprintln!("  save      failed: {}", e.user_message()),
    }
}

fn show_card(overlay: &Option<Overlay>, card: &Card, target: &lens_core::acquire::TextTarget) {
    let anchor = match target.anchor() {
        Some(a) => a,
        None => return,
    };
    show_card_at(overlay, card, &anchor);
}

fn show_card_at(overlay: &Option<Overlay>, card: &Card, anchor: &ScreenRect) {
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
    endpoint: &str,
    model: &Option<String>,
) {
    let mut snapshot = shared.lock().unwrap();
    snapshot.build = format!("{} macOS", env!("CARGO_PKG_VERSION"));
    snapshot.configuration = vec![
        ("language".into(), language.into()),
        ("endpoint".into(), endpoint.into()),
        (
            "model".into(),
            model.clone().unwrap_or_else(|| "(auto)".into()),
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
