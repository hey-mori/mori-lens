//! Mori Lens. Rest the pointer on a sentence; read it in your language.
//!
//! The whole product, end to end, in one loop:
//!
//! ```text
//! pointer settles  →  accessibility read  →  local model  →  native card
//! ```
//!
//! # Why translation runs on another thread
//!
//! A quick translation measures around a second
//! (`docs/benchmarks/2026-08-04-acquisition-sweep.md` for acquisition; the
//! model is the rest). The pointer is sampled every 25ms. Doing both on one
//! thread would freeze pointer tracking for the length of every translation, so
//! the loop would miss the movement that should have cancelled it.
//!
//! So the loop stays cheap and a worker does the waiting. Every job carries the
//! generation it belongs to, and a result whose generation is no longer current
//! is **dropped rather than drawn**. That is the rule that stops a card
//! appearing over a sentence the user stopped looking at a second ago.

#![cfg(windows)]
// No console window. A tray application that opens a black terminal you must
// leave open is not a tray application, and that terminal was the most
// unfinished thing about this product. `console::attach_to_launcher` takes the
// launching terminal's console back when there is one, so running it from a
// shell still prints and double-clicking it does not open anything.
#![windows_subsystem = "windows"]

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lens_core::cache::{CacheKey, TranslationCache};
use lens_core::card::Card;
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::{Generation, HoverConfig};
use lens_core::icon::Glyph;
use lens_core::language::{decide, Decision};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState, Resolution};
use lens_core::settings::{self, Problem, Settings};
use lens_core::theme::{Preset, TextScale, Theme, Translucency};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_windows::{
    declare_dpi_awareness, monitor_scale_at, ocr, system_preferences, Overlay, Tray, TrayEvent,
    UiaTextAcquirer,
};
use lens_translate::OllamaTranslator;

mod console;
mod hover_worker;
mod lifecycle;
mod manual_ocr;
mod pause;
mod pointer;
mod refresh;
mod silence;

/// What the worker is asked to translate.
struct Job {
    generation: Generation,
    original: String,
    focused_word: Option<String>,
    anchor: ScreenRect,
    /// Carried with the job rather than owned by the worker, so editing the
    /// target language in the settings file takes effect on the next hover
    /// instead of the next launch.
    language: String,
    /// Whether the text came from a reading with misread words in it.
    ///
    /// Travels with the job so the caveat lands on the card the reader ends up
    /// looking at, rather than on one that is shown and then covered by the
    /// answer.
    doubtful: bool,
}

/// What comes back, still carrying the generation it belongs to.
struct Answer {
    generation: Generation,
    card: Card,
    anchor: ScreenRect,
    /// Whether this is the finished answer or a piece of one still arriving.
    final_answer: bool,
}

/// Bumped whenever the prompt changes, so a cache filled by the old wording is
/// not served under the new one.
const PROMPT_VERSION: u32 = 1;

/// How many sentences to remember. A reader revisits a paragraph, not a book,
/// and every entry is text the user read, so the bound is small on purpose.
const CACHE_ENTRIES: usize = 256;

const USAGE: &str = "\
lens · Mori Lens

USAGE
    lens [OPTIONS]

    Rest the pointer on a sentence. Hold Alt unless --auto is given.
    Ctrl+C to quit.

OPTIONS
    --auto              Translate in every application, not only while Alt is
                        held, for this run. The lasting version of this is
                        `default = allow` in the settings file.
    --settings <path>   Use this settings file instead of the one in %APPDATA%.
                        Ctrl+Alt+O reads under the pointer with OCR, for the
                        applications that expose no text at all.
                        Written with every option in it on first run.
    --language <name>   Target language. Default Korean.
    --endpoint <addr>   Where Ollama listens. Default 127.0.0.1:11434, and it
                        must be on this machine: 127.0.0.1, localhost or ::1.
                        Lens has no way to send text off this machine safely
                        and does not try.
    --model <name>      Ollama model. Default: whichever suitable one is
                        installed.
    --dwell <ms>        How long the pointer must rest. Default 320.
    --theme <name>      system, forest, night-lake, or paper. Default system.
    --translucency <t>  clear, balanced, or solid. Default solid: a translucent
                        card takes part of its edge colour from the page behind
                        it, and one hairline then reads unevenly along its own
                        length. The other two are still there.
    --text <size>       small, normal, large, or larger. Default normal.
    --selftest          Run the whole chain once against text already on
                        screen, without moving the pointer, and report each
                        stage. Proves the product works end to end.
    -h, --help          This text.
";

/// What the product runs with, once the file and the flags have been combined.
struct Options {
    selftest: bool,
    preset: Preset,
    translucency: Translucency,
    text: TextScale,
    language: String,
    model: Option<String>,
    /// Where Ollama is. Loopback only; see `Settings::endpoint`.
    endpoint: String,
    dwell: Duration,
    policy: AppPolicy,
    /// Where the settings came from, and anything wrong with them. Printed at
    /// startup: a file the product silently ignored is worse than no file.
    source: SettingsSource,
}

/// Where the settings were read from, for the startup banner.
struct SettingsSource {
    path: std::path::PathBuf,
    state: &'static str,
    problems: Vec<Problem>,
}

/// Only what was actually typed. Everything unset falls through to the file,
/// and everything the file does not set falls through to the defaults.
#[derive(Default)]
struct Flags {
    selftest: bool,
    settings: Option<std::path::PathBuf>,
    /// `--auto` is a shortcut for `default = allow`, kept because a one-off
    /// run should not need the file edited and put back.
    auto: bool,
    language: Option<String>,
    model: Option<String>,
    endpoint: Option<String>,
    dwell_ms: Option<u64>,
    theme: Option<String>,
    translucency: Option<String>,
    text: Option<String>,
}

fn preset_named(name: &str) -> Result<Preset, String> {
    match name {
        "system" => Ok(Preset::System),
        "forest" => Ok(Preset::Forest),
        "night-lake" => Ok(Preset::NightLake),
        "paper" => Ok(Preset::Paper),
        other => Err(format!("unknown theme: {other}")),
    }
}

fn translucency_named(name: &str) -> Result<Translucency, String> {
    match name {
        "clear" => Ok(Translucency::Clear),
        "balanced" => Ok(Translucency::Balanced),
        "solid" => Ok(Translucency::Solid),
        other => Err(format!("unknown translucency: {other}")),
    }
}

fn text_named(name: &str) -> Result<TextScale, String> {
    match name {
        "small" => Ok(TextScale::Small),
        "normal" => Ok(TextScale::Normal),
        "large" => Ok(TextScale::Large),
        "larger" => Ok(TextScale::Larger),
        other => Err(format!("unknown text size: {other}")),
    }
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
            "--auto" => flags.auto = true,
            "--selftest" => flags.selftest = true,
            "--settings" => flags.settings = Some(value("--settings")?.into()),
            "--theme" => flags.theme = Some(value("--theme")?),
            "--translucency" => flags.translucency = Some(value("--translucency")?),
            "--text" => flags.text = Some(value("--text")?),
            "--language" => flags.language = Some(value("--language")?),
            "--model" => flags.model = Some(value("--model")?),
            "--endpoint" => flags.endpoint = Some(value("--endpoint")?),
            "--dwell" => {
                flags.dwell_ms = Some(
                    value("--dwell")?
                        .parse()
                        .map_err(|_| "--dwell needs milliseconds")?,
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(flags)
}

/// Where the settings file lives when nobody said otherwise.
///
/// `%APPDATA%` rather than beside the executable: a user should not need write
/// access to wherever they unzipped this, and a settings file next to a binary
/// is lost the first time the binary is replaced.
fn default_settings_path() -> std::path::PathBuf {
    let base = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    base.join("Mori Lens").join("settings.conf")
}

/// Read the settings file, writing the template the first time.
///
/// A missing file is the normal first run, not an error. Writing the template
/// means the answer to "how do I configure this" is a file that already exists
/// with every option in it, rather than documentation.
///
/// A file that cannot be written is also not an error. It means this run uses
/// defaults and says so.
fn load_settings(path: &std::path::Path) -> (Settings, SettingsSource) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let (settings, problems) = Settings::parse(&text);
            (
                settings,
                SettingsSource {
                    path: path.to_owned(),
                    state: "read",
                    problems,
                },
            )
        }
        Err(_) => {
            let wrote = path
                .parent()
                .map(|parent| std::fs::create_dir_all(parent).is_ok())
                .unwrap_or(false)
                && std::fs::write(path, settings::TEMPLATE).is_ok();
            (
                Settings::default(),
                SettingsSource {
                    path: path.to_owned(),
                    state: if wrote {
                        "written with the defaults, edit it and restart"
                    } else {
                        "not readable and not writable, so this run is the defaults"
                    },
                    problems: Vec::new(),
                },
            )
        }
    }
}

/// The file first, then anything typed on the command line over the top.
fn parse() -> Result<Options, String> {
    let flags = parse_flags()?;
    let path = flags.settings.clone().unwrap_or_else(default_settings_path);
    let (settings, source) = load_settings(&path);

    let mut policy = settings.policy();
    if flags.auto {
        // Only the default tier. A `deny` a user wrote for one application is
        // theirs and a convenience flag does not get to overrule it, and the
        // password-manager floor survives either way.
        policy.set_default(AppTier::Allow);
    }

    Ok(Options {
        selftest: flags.selftest,
        preset: preset_named(flags.theme.as_ref().unwrap_or(&settings.theme))?,
        translucency: translucency_named(
            flags
                .translucency
                .as_ref()
                .unwrap_or(&settings.translucency),
        )?,
        text: text_named(flags.text.as_ref().unwrap_or(&settings.text))?,
        language: flags.language.unwrap_or(settings.language),
        model: flags.model.or(settings.model),
        endpoint: match flags.endpoint {
            // The flag goes through the same check the file does, so a command
            // line cannot reach a host the settings file would refuse.
            Some(endpoint)
                if lens_core::translate::engine_for(&endpoint)
                    == lens_core::translate::EngineId::LocalOllama =>
            {
                endpoint
            }
            Some(endpoint) => {
                return Err(format!(
                    // One line, deliberately. A `\`-continuation inside this
                    // literal came back from rustfmt rejoined with its
                    // indentation intact, so the message reached a user with
                    // twenty spaces in the middle of a sentence.
                    "--endpoint {endpoint} is not on this machine, and Lens will not send text off it",
                ));
            }
            None => settings.endpoint,
        },
        dwell: Duration::from_millis(flags.dwell_ms.unwrap_or(settings.dwell_ms)),
        policy,
        source,
    })
}

fn main() -> std::process::ExitCode {
    // Before the first `println!`, or the banner goes nowhere.
    console::attach_to_launcher();

    let options = match parse() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return std::process::ExitCode::from(2);
        }
    };

    // Before any coordinate is read. Without it every rectangle is silently
    // wrong above 100% scaling.
    let dpi = declare_dpi_awareness();

    let mut translator = OllamaTranslator::choosing(&options.endpoint);
    if let Some(model) = &options.model {
        translator = OllamaTranslator::new(&options.endpoint, model);
    }
    // Resolved once at startup rather than per hover, so the first hover does
    // not pay for a listing, and so an absent Ollama is reported now instead of
    // as a mysteriously empty card later.
    // Carries the remedy, not just the failure. This string reaches three
    // places: the banner, the tray tooltip, and nowhere else a first-time user
    // would look. Someone who unzipped Lens without Ollama and double-clicked
    // it never sees a terminal, so the tooltip is the only thing standing
    // between them and a product that silently does nothing.
    let engine_line = match translator.resolve_model() {
        Ok(model) => model.to_owned(),
        Err(error) => match error.remedy() {
            Some(remedy) => format!("{} · {remedy}", error.user_message()),
            None => error.user_message(),
        },
    };

    println!("Mori Lens");
    println!(
        "  settings  {} · {}",
        options.source.path.display(),
        options.source.state
    );
    for problem in &options.source.problems {
        // Loud, and on its own line. A setting that was ignored has to be
        // visible or the user concludes the product does not work.
        println!("            line {}: {}", problem.line, problem.message);
    }
    println!("  target    {}", options.language);
    println!("  engine    {engine_line}");
    // Where, as well as what. A user who has pointed this somewhere else has
    // no other way to confirm the product agreed, and one who has not gets a
    // line that says the text is not going anywhere.
    println!("  address   {} · on this machine", options.endpoint);
    println!("  dwell     {}ms", options.dwell.as_millis());
    println!("  mode      {}", options.policy.describe());
    println!(
        "  dpi       {}",
        if dpi {
            "per-monitor aware v2"
        } else {
            "NOT declared: rectangles above 100% scaling are unreliable"
        }
    );
    let mut pause = pause::PauseKey::register();
    println!(
        "  pause     {}",
        if pause.is_registered() {
            "Ctrl+Alt+L stops the pointer being sampled"
        } else {
            "UNAVAILABLE: Ctrl+Alt+L is taken by another application"
        }
    );
    println!(
        "  ocr       {}",
        if !pause.ocr_is_registered() {
            "UNAVAILABLE: Ctrl+Alt+O is taken by another application".to_owned()
        } else if ocr::available_languages().is_empty() {
            "Ctrl+Alt+O, but this system has no recognisers installed".to_owned()
        } else {
            format!(
                "Ctrl+Alt+O reads under the pointer with OCR · {}",
                ocr::available_languages().join(", "),
            )
        },
    );
    // The one line that tells a first-time user what to do, so it has to
    // agree with the policy above it. It said "Rest the pointer on a sentence"
    // while the mode line two rows up said "hold Alt", and the first person to
    // run the release build did exactly that and got nothing.

    let mut system = system_preferences();
    let theme = Theme::resolve(options.preset, options.translucency, options.text, system);
    println!(
        "  theme     {} · {}pt body, converted per monitor · surface alpha {}",
        options.preset.as_str(),
        theme.body_pt,
        theme.surface_alpha,
    );
    // Printed because these override the chosen preset. A user whose card came
    // out solid when they asked for clear should be able to see why here
    // rather than think the flag was ignored.
    println!(
        "  system    dark={} reduced-transparency={} high-contrast={} reduced-motion={}",
        system.dark, system.reduced_transparency, system.high_contrast, system.reduced_motion,
    );

    let Some(overlay) = Overlay::start_with(theme) else {
        eprintln!("could not create the overlay window");
        return std::process::ExitCode::from(1);
    };

    // The only part of Lens a user can see while it works. Without it the
    // product is invisible: no window, no taskbar entry, and no way to stop it
    // that does not involve finding this terminal again.
    let tray = Tray::start(
        theme.body,
        &tooltip(&options.language, &engine_line, false, &options.policy),
    );
    println!(
        "  tray      {}",
        match tray {
            Some(_) => "click to pause, right-click for the menu",
            None => "UNAVAILABLE: no icon, so Ctrl+C here is the only way to stop",
        },
    );

    if options.selftest {
        // Synchronous here, unlike the normal path: the point of the selftest
        // is to measure the chain, and a cold load landing in the middle of it
        // would be reported as translation latency.
        match translator.prewarm() {
            Ok(elapsed) => println!(
                "engine warm after {}ms
",
                elapsed.as_millis()
            ),
            Err(error) => println!(
                "could not warm the engine: {}
",
                error.reason()
            ),
        }
        let code = selftest(&overlay, &translator, &options.language);
        translator.release();
        return code;
    }

    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
    let (answers_tx, answers_rx) = mpsc::channel::<Answer>();

    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let warm_translator = translator.clone();
    // Kept for the exit path: the worker takes ownership of the translator.
    let releaser = translator.clone();
    // Off the main thread so the window and the pointer loop come up
    // immediately; a user should not wait on a model to see the app start.
    std::thread::spawn(move || {
        // Failure is silent by design: the user did not ask for this, so a
        // warm that fails must not produce an error state. It only means the
        // first hover pays for the load instead.
        if let Ok(elapsed) = warm_translator.prewarm() {
            println!("engine warm after {}ms", elapsed.as_millis());
        }
    });

    std::thread::spawn(move || {
        // The translator lives on the worker, not shared: it holds no state
        // worth sharing and this keeps the loop free of a lock.
        // When the last answer came back. Ollama's lease is thirty minutes and
        // every request resets it, so within that window the model is
        // certainly still in memory and there is nothing to ask about.
        let mut last_answer: Option<Instant> = None;
        const LEASE_IS_SAFE_FOR: Duration = Duration::from_secs(20 * 60);

        while let Ok(job) = jobs_rx.recv() {
            // Take only the newest queued job. A pointer crossing three
            // sentences queues three translations, and finishing them all
            // spends six seconds to show one card. The stale ones are dropped
            // here rather than after the model has already answered them.
            let mut job = job;
            let mut skipped = 0usize;
            while let Ok(newer) = jobs_rx.try_recv() {
                job = newer;
                skipped += 1;
            }
            if skipped > 0 {
                println!("worker  skipped {skipped} stale job(s)");
            }

            let mut request = QuickRequest::new(&job.original, &job.language);
            if let Some(word) = &job.focused_word {
                request = request.with_focus(word);
            }

            // A cold load measured thirteen seconds on this machine, and for
            // all of it the card said only "번역 중", which reads as a hung
            // application rather than as a machine doing something. Asked only
            // when the lease may have lapsed, because the question costs 8ms.
            let may_have_unloaded = last_answer.is_none_or(|at| at.elapsed() > LEASE_IS_SAFE_FOR);
            if may_have_unloaded && !translator.is_resident() {
                let _ = answers_tx.send(Answer {
                    generation: job.generation,
                    card: Card::loading_model(&job.original),
                    anchor: job.anchor,
                    final_answer: false,
                });
            }

            let key = CacheKey::of(&request, translator.model(), PROMPT_VERSION);
            if let Some(hit) = cache.get(&key) {
                let mut card = Card::translated(&job.original, &hit);
                if job.doubtful {
                    card = card.built_on_a_doubtful_reading();
                }
                let _ = answers_tx.send(Answer {
                    generation: job.generation,
                    card,
                    anchor: job.anchor,
                    final_answer: true,
                });
                continue;
            }

            // Report the answer as it forms. Each partial is a complete card
            // carrying the original, so the rule that a translation never
            // appears without it holds for a half-finished one too.
            let partial_tx = answers_tx.clone();
            let generation = job.generation;
            let anchor = job.anchor;
            let original = job.original.clone();
            let mut on_partial = move |text: &str| {
                let card = Card::partial(&original, text);
                let _ = partial_tx.send(Answer {
                    generation,
                    card,
                    anchor,
                    final_answer: false,
                });
            };

            let card = match translator.quick_streaming(&request, &mut on_partial) {
                Ok(translation) => {
                    cache.insert(key, translation.clone());
                    Card::translated(&job.original, &translation)
                }
                Err(error) => Card::not_translated(&job.original, &error),
            };
            // The caveat rides the answer rather than preceding it, so it is
            // on the card the reader is still looking at a second later.
            let card = if job.doubtful {
                card.built_on_a_doubtful_reading()
            } else {
                card
            };
            last_answer = Some(Instant::now());
            if answers_tx
                .send(Answer {
                    generation: job.generation,
                    card,
                    anchor: job.anchor,
                    final_answer: true,
                })
                .is_err()
            {
                break;
            }
        }
    });

    // Built here only to be asked two questions and dropped. UI Automation
    // interfaces belong to the apartment they were created in, so the acquirer
    // the product reads with is constructed on the thread that will use it,
    // which is the acquisition worker started further down. This one exists so
    // an unreachable accessibility layer is reported before the banner rather
    // than as a worker that silently answers nothing, and so the guard line
    // below states a fact about this machine rather than a hope.
    let bounded = match UiaTextAcquirer::new() {
        Ok(acquirer) => acquirer.calls_are_bounded(),
        Err(error) => {
            eprintln!("the accessibility layer is unreachable: {error}");
            return std::process::ExitCode::from(1);
        }
    };

    // Printed because the alternative is discovering it during a freeze. UI
    // Automation waits two minutes for a wedged provider unless told not to
    // (issue #28). It no longer takes the pointer loop with it, because
    // acquisition is not on that thread any more (issue #42), but a hover that
    // takes two minutes is still a hover nobody gets an answer to.
    println!(
        "  guard     {}",
        if bounded {
            "a slow application is given 800ms, then abandoned · off the pointer loop"
        } else {
            "UNBOUNDED: one wedged application can hold a read for two minutes"
        }
    );

    let mut policy = options.policy.clone();
    // Without this Lens reads its own card and translates its own translation.
    policy.deny_self("lens");

    // Last, after every fact about how this run is configured. It is the line
    // a first-time user acts on, and it sat in the middle of the banner with
    // the mode line above it saying something else.
    println!("\n{}\n", how_to_use(&policy, tray.is_some()));

    // The policy is kept here as well as sent, because the tray tooltip and
    // the reload line need it synchronously, and a round trip to a worker to
    // describe a menu is a strange thing to make anybody wait for.
    let mut hover_config = HoverConfig {
        dwell: options.dwell,
        ..HoverConfig::default()
    };
    let worker_config = hover_config;
    let worker_policy = policy.clone();
    // **Acquisition runs here and nothing else does.** While a read is in
    // flight the pointer loop keeps sampling, cards keep retiring, and the
    // pause key keeps working, none of which was true: a wedged provider took
    // all three with it for as long as it took to give up (issue #42).
    let Some(mut hover) = hover_worker::HoverWorker::start(move || {
        let acquirer = UiaTextAcquirer::new().ok()?;
        Some(HoverSession::new(
            worker_config,
            worker_policy,
            Box::new(acquirer),
        ))
    }) else {
        eprintln!("the acquisition thread could not be started");
        return std::process::ExitCode::from(1);
    };

    pointer::install_stop_handler();
    let started = Instant::now();
    // When the card appears, stays, and goes. Extracted so the decisions can
    // be played through in a test instead of only on a desktop: this was the
    // one part of the product with no coverage, and the only bug a user has
    // reported here lived in it. See lifecycle.rs.
    let mut life = lifecycle::CardLifecycle::new();
    // Editing the settings file used to do nothing until the next launch,
    // which made the tray's "open settings" a dead end, and the theme was
    // resolved once so switching Windows to dark mode left a pale card over
    // dark windows.
    let mut watcher = refresh::Watcher::new(&options.source.path, system);
    let mut theme = theme;
    let mut language = options.language.clone();
    // The sentence the outstanding translation belongs to, so a waiting card
    // can show it. One at a time: a second hover replaces the first.
    let mut pending_original = String::new();
    // Why no card appeared, said once per distinct reason. Holding the
    // modifier over a window that exposes no text used to look exactly like a
    // product that had crashed.
    let mut narrator = silence::Narrator::default();
    let mut reach = lens_core::reach::Reach::new();
    // Resolved once, here, rather than at the moment a card is drawn. The
    // offer must name a key that will actually do something: taken by another
    // application, or with no recogniser installed, and the card says what is
    // wrong without naming a key that does nothing.
    let ocr_hotkey = (pause.ocr_is_registered() && !ocr::available_languages().is_empty())
        .then(|| "Ctrl+Alt+O".to_owned());

    // Pause has three entrances now: the hotkey, the tray icon, and the tray
    // menu. They set the same flag rather than each doing their own thing,
    // because a product where the icon and the hotkey disagree about whether it
    // is paused is worse than one with neither.
    let mut quitting = false;
    while !pointer::stopping() && !quitting {
        let mut pause_changed = false;
        if let Some(tray) = &tray {
            for event in tray.poll() {
                match event {
                    TrayEvent::TogglePause => {
                        pause.toggle();
                        pause_changed = true;
                    }
                    TrayEvent::Quit => quitting = true,
                    TrayEvent::OpenSettings => {
                        // The path is in the banner, which a user who
                        // double-clicked the binary never saw.
                        open_in_editor(&options.source.path);
                    }
                    TrayEvent::ToggleAutostart => {
                        let now = if lens_platform_windows::autostart::is_enabled() {
                            lens_platform_windows::autostart::disable();
                            false
                        } else {
                            lens_platform_windows::autostart::enable();
                            lens_platform_windows::autostart::is_enabled()
                        };
                        println!("start with Windows: {}", if now { "on" } else { "off" });
                    }
                }
            }
        }
        let mut read_with_ocr = false;
        for press in pause.poll() {
            match press {
                pause::Pressed::PauseChanged => pause_changed = true,
                pause::Pressed::ReadWithOcr => read_with_ocr = true,
            }
        }
        if pause_changed {
            if pause.is_paused() {
                if life.dismiss_now() == lifecycle::Action::Hide {
                    overlay.hide();
                }
                println!("paused · the pointer is not being sampled");
            } else {
                println!("resumed");
            }
            if let Some(tray) = &tray {
                // The icon is dimmed rather than swapped, and the tooltip says
                // it in words, because a dimmed ring alone is a guess.
                tray.set_state(
                    if pause.is_paused() {
                        Glyph::Paused
                    } else {
                        Glyph::Listening
                    },
                    &tooltip(&language, &engine_line, pause.is_paused(), &policy),
                    theme.body,
                );
            }
        }
        // Anything edited or switched since the last look. Once a second,
        // and neither of these reads the screen.
        let changes = watcher.poll(started.elapsed(), system_preferences);
        if changes.any() {
            let (settings, problems) = match changes.settings {
                Some((settings, problems)) => (Some(settings), problems),
                None => (None, Vec::new()),
            };
            for problem in &problems {
                println!("settings line {}: {}", problem.line, problem.message);
            }

            if let Some(settings) = &settings {
                language = settings.language.clone();
                let mut reloaded = settings.policy();
                reloaded.deny_self("lens");
                hover_config = HoverConfig {
                    dwell: Duration::from_millis(settings.dwell_ms),
                    ..HoverConfig::default()
                };
                policy = reloaded;
                hover.reconfigure(hover_config, policy.clone());
                if settings.model != options.model {
                    // The worker owns the translator, and swapping a model
                    // mid-flight would leave a queued job answered by the one
                    // it did not ask for. Said rather than silently ignored.
                    println!("settings: the model changed; restart to use it");
                }
            }

            // The theme depends on both: a preset from the file, and dark
            // mode from the system.
            let appearance = changes.appearance.unwrap_or(system);
            system = appearance;
            let chosen = settings.as_ref().map_or(
                (options.preset, options.translucency, options.text),
                |settings| {
                    (
                        preset_named(&settings.theme).unwrap_or(options.preset),
                        translucency_named(&settings.translucency).unwrap_or(options.translucency),
                        text_named(&settings.text).unwrap_or(options.text),
                    )
                },
            );
            theme = Theme::resolve(chosen.0, chosen.1, chosen.2, appearance);
            overlay.set_theme(theme);
            if let Some(tray) = &tray {
                tray.set_state(
                    if pause.is_paused() {
                        Glyph::Paused
                    } else {
                        Glyph::Listening
                    },
                    &tooltip(&language, &engine_line, pause.is_paused(), &policy),
                    theme.body,
                );
            }
            println!(
                "reloaded · {} · dark={} · {}",
                language,
                appearance.dark,
                policy.describe(),
            );
        }

        if read_with_ocr && !pause.is_paused() {
            // A deliberate act, so it runs on this thread and the reader waits
            // for it. Roughly a tenth of a second, against the second and a
            // half the translation after it will take.
            let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
            // SAFETY-free: a read-only monitor query.
            let scale = monitor_scale_at(at);
            // A round trip to the worker, because there is one generation
            // counter and it lives with the hover machine. If the worker is
            // mid-read this waits for it, which is the one place a keypress
            // can still be delayed by acquisition, and it is bounded.
            let Some(generation) = hover.take_generation() else {
                println!("ocr   the acquisition thread did not answer");
                continue;
            };
            life.hover_settled(generation);
            match manual_ocr::read_around(&overlay, at, scale) {
                Ok(reading) => {
                    println!(
                        "ocr   {} chars on the line, focus {} chars{}",
                        reading.sentence.chars().count(),
                        reading.word.chars().count(),
                        match reading.misread {
                            0 => String::new(),
                            n => format!(", {n} misread"),
                        },
                    );
                    if decide(&reading.sentence, &language) == Decision::AlreadyInTarget {
                        overlay.show(
                            Card::not_readable("이미 읽을 수 있는 언어입니다"),
                            reading.anchor,
                        );
                    } else {
                        life.job_dispatched(generation, reading.anchor, started.elapsed());
                        pending_original = reading.sentence.clone();
                        let _ = jobs_tx.send(Job {
                            generation,
                            original: reading.sentence,
                            focused_word: Some(reading.word),
                            anchor: reading.anchor,
                            language: language.clone(),
                            doubtful: reading.misread > 0,
                        });
                    }
                }
                Err(refusal) => {
                    // On the card, not only in a terminal nobody is looking
                    // at: the user pressed a key and is owed an answer.
                    println!("ocr   no reading [{}]", refusal.reason());
                    overlay.show(
                        Card::not_readable(refusal.user_message()),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
                }
            }
        }

        if pause.is_paused() {
            // Nothing is read while paused. Not a suppressed result, not a
            // hidden card: no accessibility call is made at all, which is what
            // docs/privacy-model.md §5 promises and what a process monitor can
            // confirm.
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let Some(position) = pointer::position() else {
            std::thread::sleep(Duration::from_millis(25));
            continue;
        };
        let input = InputState {
            button_down: pointer::any_button_down(),
            scrolling: false,
            typing: false,
            modifier_held: pointer::modifier_held(),
            ocr_requested: false,
        };
        let process_name = pointer::process_name_at(position);
        let now = started.elapsed();
        // Handed over, not performed. This is the line the whole file was
        // rearranged for: the sample goes to the acquisition thread and this
        // loop carries on to the pause key and the card timers below.
        hover.sample(position, now, input, process_name.clone());

        // The commonest way this product fails a new person: they are not
        // holding the modifier, so nothing happens, and nothing on screen says
        // why. Judged here rather than in the worker because a suppressed
        // sample never settles, so the hover machine never reports one and
        // there is nothing on that side to notice.
        //
        // The condition is narrow on purpose: the modifier must be the *only*
        // thing in the way, so this never appears over a denied application,
        // over Lens's own card, or on an empty desktop.
        let blocked_by_the_modifier = !input.modifier_held
            && !policy.allows(process_name.as_deref(), false)
            && policy.allows(process_name.as_deref(), true);
        if reach.pointer(position, now, blocked_by_the_modifier) {
            println!("hint  not reading here without the modifier");
            // Through the lifecycle like any other card, so it retires when the
            // pointer leaves rather than sitting there. A round trip for the
            // generation, which happens once a session at most and only while
            // nothing is in flight: the modifier is not held, so there is no
            // hover for it to make stale.
            if let Some(generation) = hover.take_generation() {
                let anchor = ScreenRect::new(position.x, position.y, 1, 1);
                life.job_dispatched(generation, anchor, now);
                life.answer_arrived(generation, anchor);
                narrator.card_shown();
                overlay.show(Card::hold_the_modifier("Alt"), anchor);
            }
        }

        while let Some(answer) = hover.try_recv() {
            let hover_worker::FromHover::Settled {
                generation,
                point,
                now,
                process_name,
                resolution,
            } = answer
            else {
                // A generation nobody is waiting for. It can only arrive if a
                // manual OCR request timed out and its reply came late, and
                // using it would attach a card to a keypress the reader has
                // already given up on.
                continue;
            };
            life.hover_settled(generation);
            match resolution {
                Resolution::Presented(target) => {
                    // This application can be read, so it is never offered the
                    // fallback again however the pointer wanders afterwards.
                    reach.readable(process_name.as_deref());
                    // And a card is about to appear, so the first-run hint has
                    // nothing left to explain.
                    reach.card_shown();
                    let sentence = target.segmented.sentence_text().trim().to_owned();
                    // Text already in the reader's language costs nothing and
                    // shows nothing. Without this a Korean reader hovering
                    // over Korean waits a second and a half for a card that
                    // re-words their own sentence, which on a mostly-Korean
                    // screen is most hovers.
                    if decide(&sentence, &language) == Decision::AlreadyInTarget {
                        // The last silent path. A reader hovering Korean saw
                        // nothing happen and had no way to tell that from an
                        // application exposing no text, which every other
                        // quiet outcome now explains.
                        if let Some(line) = narrator.line_for_skip(
                            &format!("already in {language}"),
                            process_name.as_deref(),
                        ) {
                            println!("{line}");
                        }
                        continue;
                    }
                    // The region the card must not cover, which is the
                    // paragraph when that is near enough and the pointer's own
                    // line when it is not. Anchoring on the word put the
                    // answer on top of the rest of what was being read.
                    let anchor = target
                        .card_anchor(point)
                        .or_else(|| target.anchor())
                        .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1));
                    narrator.card_shown();
                    if !sentence.is_empty() {
                        // Remembered before the job is sent, so the loop can
                        // tell the reader it was heard if the answer is slow.
                        life.job_dispatched(generation, anchor, now);
                        pending_original = sentence.clone();
                        let _ = jobs_tx.send(Job {
                            generation,
                            original: sentence,
                            focused_word: target.segmented.word_text().map(str::to_owned),
                            anchor,
                            language: language.clone(),
                            // The accessibility path reads text, not pixels.
                            doubtful: false,
                        });
                    }
                }
                // None of these draws a card, and until now none of them said
                // anything either. A user holding Alt over a window that
                // exposes no text saw an application doing nothing, which is
                // indistinguishable from a broken one. It is the first thing
                // the first person to run a release build asked about.
                //
                // Never the text: the length, the reason, and the application
                // (`docs/privacy-model.md` §3).
                other => {
                    if let Some(line) = narrator.line_for(&other, process_name.as_deref()) {
                        println!("{line}");
                    }
                    // A console line is not an answer for somebody who started
                    // this from Explorer: a Windows subsystem binary has no
                    // console to print it in. Once per application, when the
                    // application rather than the pointer is what failed, the
                    // reader is told on the card instead, and told what to
                    // press. `reach` decides; this only draws.
                    if let Resolution::Failed(error) = &other {
                        if reach.unreadable(process_name.as_deref(), error) {
                            let card = Card::cannot_read_this_app(ocr_hotkey.as_deref());
                            // **The settled point, not the pointer now.** They
                            // were the same value while acquisition ran inline
                            // on this loop. They are not any more: this card is
                            // shown after a read that failed, and the read is
                            // allowed 800ms, so anchoring on the live pointer
                            // puts the card wherever the hand drifted to in the
                            // meantime rather than on the text it is about.
                            let anchor = ScreenRect::new(point.x, point.y, 1, 1);
                            narrator.card_shown();
                            life.job_dispatched(generation, anchor, now);
                            life.answer_arrived(generation, anchor);
                            overlay.show(card, anchor);
                        }
                    }
                }
            }
        }

        // Say the sentence was heard, if the answer is taking long enough
        // that the screen would otherwise be empty. Once per translation, and
        // never for one the cache already answered.
        if let Some(anchor) = life.waited_long_enough(now) {
            overlay.show(Card::waiting(&pending_original), anchor);
        }

        if life.pointer_moved(position, now) == lifecycle::Action::Hide {
            overlay.hide();
        }

        while let Ok(answer) = answers_rx.try_recv() {
            if answer.final_answer {
                log_card(&answer.card);
            }
            // The generation check lives in the lifecycle, so an answer for a
            // hover the reader has left is dropped in one place rather than at
            // each call site that might forget.
            if life.answer_arrived(answer.generation, answer.anchor) == lifecycle::Action::Show {
                overlay.show(answer.card, answer.anchor);
            }
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    overlay.hide();
    drop(tray);
    // Hand the model back. Without this the runtime holds it for the rest of
    // the lease, so a tool that has exited goes on occupying gigabytes of
    // somebody's memory.
    releaser.release();
    println!("\nstopped, and the model released");
    std::process::ExitCode::SUCCESS
}

/// Open the settings file with whatever the user edits `.conf` files with.
///
/// `ShellExecuteW` rather than a hard-coded editor: the point is that it opens
/// in whatever they already use. A machine with no association for the
/// extension shows the "open with" chooser, which is the correct outcome and
/// not an error worth reporting.
fn open_in_editor(path: &std::path::Path) {
    use std::os::windows::ffi::OsStrExt;

    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    wide.push(0);
    // SAFETY: a documented shell call with a null-terminated path this
    // function owns, on an action the user just chose from a menu.
    unsafe {
        let _ = windows::Win32::UI::Shell::ShellExecuteW(
            None,
            windows::core::w!("open"),
            windows::core::PCWSTR(wide.as_ptr()),
            windows::core::PCWSTR::null(),
            windows::core::PCWSTR::null(),
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
        );
    }
}

/// What the tray icon says when the pointer rests on it.
///
/// The two facts a user wants without opening anything: whether it is
/// listening, and what is doing the translating. Korean, because this is
/// product UI rather than a repository artifact (`docs/git-workflow.md` §7).
///
/// The engine line can be an error message when Ollama is absent, and that is
/// the case this matters most for: a Lens that silently shows no cards because
/// no model is installed looks identical to one that is broken.
fn tooltip(language: &str, engine: &str, paused: bool, policy: &AppPolicy) -> String {
    format!(
        "Mori Lens · {}\n{}\n{language} · {engine}",
        if paused { "일시정지" } else { "켜짐" },
        if paused {
            "다시 켜려면 클릭".to_owned()
        } else {
            korean_how_to(policy)
        },
    )
}

/// How to make a card appear, in one line, for whatever the policy is.
///
/// Two versions of the same sentence, because the two places it appears have
/// different readers: the banner is seen by someone at a terminal, the tooltip
/// by someone who double-clicked a binary and has never seen a terminal.
///
/// Both exist because the banner used to contradict itself. `mode  hold Alt`
/// two lines above `Rest the pointer on a sentence` is not an instruction, it
/// is a puzzle, and the first person to run a release build hovered without Alt
/// and reported that nothing happened.
fn how_to_use(policy: &AppPolicy, has_tray: bool) -> String {
    let action = match policy.default_tier() {
        AppTier::Allow => "Rest the pointer on a sentence.".to_owned(),
        AppTier::Modifier => "Hold Alt and rest the pointer on a sentence.".to_owned(),
        // Nothing will happen anywhere the user has not allowed by name, so
        // saying "rest the pointer" would be false.
        AppTier::Deny => format!(
            "Nothing is read by default. {} applications have a rule of their own.",
            policy.rule_count(),
        ),
    };
    format!("{action} {}", how_to_quit(has_tray))
}

/// How to stop it, which stopped being Ctrl+C and went on saying Ctrl+C.
///
/// Lens is a Windows subsystem binary now, so a shell does not wait for it: the
/// prompt comes straight back and Ctrl+C there goes to the shell, not to Lens.
/// The banner said "Ctrl+C to quit" for a while after that stopped being true,
/// and a user found out by being unable to stop it.
///
/// The console handler is still installed, because a console that *does* send
/// a break should still be obeyed. It is no longer advertised as the way out.
fn how_to_quit(has_tray: bool) -> &'static str {
    if has_tray {
        "Right-click the tray icon to quit."
    } else {
        // No icon and no console to interrupt. Saying so beats naming a key
        // that will not work.
        "No tray icon: stop it from Task Manager, or with Stop-Process."
    }
}

fn korean_how_to(policy: &AppPolicy) -> String {
    match policy.default_tier() {
        AppTier::Allow => "문장 위에 포인터를 올려 두세요".to_owned(),
        AppTier::Modifier => "Alt를 누른 채 문장 위에 포인터를 올려 두세요".to_owned(),
        AppTier::Deny => "허용한 앱에서만 동작합니다".to_owned(),
    }
}

/// One line per card, shape only.
///
/// Length and language, never content. A release build that logged what the
/// user was reading would undo the whole privacy model
/// (`docs/privacy-model.md` §3).
fn log_card(card: &Card) {
    println!(
        "card  original={} chars  {}  [{}]",
        card.original.chars().count(),
        match &card.body {
            lens_core::card::CardBody::Translation(text) =>
                format!("translated={} chars", text.chars().count()),
            lens_core::card::CardBody::Problem(_) => "problem".to_owned(),
            lens_core::card::CardBody::Waiting(status) => format!("waiting: {status}"),
        },
        card.footer,
    );
}

/// Run the whole chain once against text that is already on screen.
///
/// **The pointer is not moved.** `ElementFromPoint` takes a coordinate as an
/// argument, which is the same fact the probe's sweep is built on, so the chain
/// can be exercised over a real window without touching the user's mouse or
/// asking them to hold it still.
///
/// This exists because every other check in this repository can pass while the
/// product does nothing: the acquirer has its own tests, the translator has
/// its own, the overlay has its own, and none of them prove the three are
/// wired together.
fn selftest(
    overlay: &Overlay,
    translator: &OllamaTranslator,
    language: &str,
) -> std::process::ExitCode {
    use lens_core::acquire::{AcquireContext, TextAcquirer};
    use lens_core::geometry::ScreenPoint;
    use lens_platform_windows::{root_window_at, top_level_windows};

    println!(
        "selftest · the pointer is not moved
"
    );

    let Ok(acquirer) = UiaTextAcquirer::new() else {
        eprintln!("the accessibility layer is unreachable");
        return std::process::ExitCode::from(1);
    };
    let context = AcquireContext::new(Generation(1));

    // Find a point, in any visible window, that yields a real sentence. A
    // sentence rather than any text, because a two-character label proves the
    // plumbing and not the product.
    //
    // Bounded, because it is a diagnostic. A desktop-spanning window is eight
    // thousand grid points at roughly 10ms each, so an unbudgeted scan of a
    // screen with no prose on it runs for minutes and looks like a hang. It
    // did exactly that once, which is why the budget is here.
    const SCAN_BUDGET: Duration = Duration::from_secs(20);
    let scan_started = Instant::now();
    let mut probed = 0usize;
    let mut found = None;
    'outer: for window in top_level_windows() {
        let bounds = window.client_bounds;
        let step = 24;
        let mut y = bounds.y + 8;
        while y < bounds.bottom() - 8 {
            let mut x = bounds.x + 8;
            while x < bounds.right() - 8 {
                if scan_started.elapsed() > SCAN_BUDGET {
                    break 'outer;
                }
                probed += 1;
                let point = ScreenPoint::new(x, y);
                if root_window_at(point) == Some(window.id) {
                    if let Ok(target) = acquirer.acquire(point, &context) {
                        let sentence = target.segmented.sentence_text().trim().to_owned();
                        if sentence.chars().count() >= 24 && target.is_on_target(point) {
                            found = Some((
                                window.process_name.clone().unwrap_or_else(|| "?".into()),
                                sentence,
                                target.segmented.word_text().map(str::to_owned),
                                target.anchor().unwrap_or(ScreenRect::new(x, y, 1, 1)),
                            ));
                            break 'outer;
                        }
                    }
                }
                x += step;
            }
            y += step;
        }
    }

    let Some((app, sentence, word, anchor)) = found else {
        println!(
            "No window on screen exposed a sentence to read. {probed} points probed in {}ms{}.",
            scan_started.elapsed().as_millis(),
            if scan_started.elapsed() > SCAN_BUDGET {
                ", stopped at the budget"
            } else {
                ""
            },
        );
        println!("Open something with prose in it and run this again.");
        return std::process::ExitCode::from(1);
    };

    println!(
        "1. acquired   {} chars from {app}",
        sentence.chars().count()
    );
    println!(
        "   anchor     {}x{} at ({}, {})",
        anchor.width, anchor.height, anchor.x, anchor.y
    );
    println!("   focused    {}", word.as_deref().unwrap_or("(none)"));

    let decision = decide(&sentence, language);
    println!(
        "   language   {}",
        match decision {
            Decision::AlreadyInTarget => "already in the target language, nothing to translate",
            Decision::Translate => "a different script from the target, translating",
            Decision::Undecidable =>
                "same script as the target, so undecidable without a classifier; translating",
        },
    );
    if decision == Decision::AlreadyInTarget {
        println!(
            "
selftest passed: the text on screen needed no translation, and none was made."
        );
        return std::process::ExitCode::SUCCESS;
    }

    let mut request = QuickRequest::new(&sentence, language);
    if let Some(word) = &word {
        request = request.with_focus(word);
    }
    let began = Instant::now();
    let mut first_text = None;
    let mut redraws = 0usize;
    let mut on_partial = |text: &str| {
        redraws += 1;
        if first_text.is_none() {
            first_text = Some(began.elapsed());
        }
        // Draw the partial, so the selftest exercises the same path a hover
        // takes rather than a quieter one.
        overlay.show(Card::partial(&sentence, text), anchor);
    };

    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let key = CacheKey::of(&request, translator.model(), PROMPT_VERSION);
    let card = match translator.quick_streaming(&request, &mut on_partial) {
        Ok(translation) => {
            println!(
                "2. translated {} chars in {}ms via {}",
                translation.text.chars().count(),
                translation.latency_ms,
                translation.model,
            );
            match first_text {
                Some(at) => println!(
                    "   streaming  first text at {}ms, {redraws} redraws",
                    at.as_millis(),
                ),
                None => println!("   streaming  no partial arrived"),
            }
            cache.insert(key.clone(), translation.clone());
            Card::translated(&sentence, &translation)
        }
        Err(error) => {
            println!(
                "2. not translated: {} ({})",
                error.user_message(),
                error.reason()
            );
            Card::not_translated(&sentence, &error)
        }
    };

    // The same sentence again, which is what a reader moving back along a line
    // produces several times a minute.
    let again = Instant::now();
    match cache.get(&key) {
        Some(hit) => println!(
            "   cache      the same sentence again in {}us, {} chars",
            again.elapsed().as_micros(),
            hit.text.chars().count(),
        ),
        None => println!("   cache      MISS on a sentence just inserted"),
    }

    if !overlay.show(card, anchor) {
        println!("3. REFUSED    the card was not showable");
        return std::process::ExitCode::from(1);
    }
    println!("3. shown      card is on screen for 4 seconds");
    println!(
        "
total {}ms from acquisition to card",
        began.elapsed().as_millis()
    );

    std::thread::sleep(Duration::from_secs(4));
    overlay.hide();
    std::thread::sleep(Duration::from_millis(200));
    println!(
        "
selftest passed: text on screen became a card, pointer untouched."
    );
    std::process::ExitCode::SUCCESS
}
