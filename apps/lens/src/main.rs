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

use lens_core::analyse::{Analyser, DeepRequest, Profile};
use lens_core::cache::{AnalysisCache, CacheKey, TranslationCache};
use lens_core::card::Card;
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::{Generation, HoverConfig};
use lens_core::icon::Glyph;
use lens_core::language::{decide, Decision};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState, OcrMode, Resolution};
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
mod keeping;
mod lesson;
mod lifecycle;
mod manual_ocr;
mod pause;
mod pointer;
mod refresh;
mod silence;

/// What the worker is asked to do.
///
/// Two variants rather than a flag, because the two paths of
/// [ADR-0005](../../../docs/decisions/ADR-0005-two-speed-ai.md) share a worker
/// and nothing else: different prompt, different budget, different timeout. A
/// `Translate` carrying an optional explanation would be one struct describing
/// two requests, and the field that says which would be the thing to forget.
enum Job {
    Translate(Translate),
    /// Explain the sentence a card is already showing, because a reader
    /// pressed the key for it.
    Explain(Explain),
}

/// What the worker is asked to translate.
struct Translate {
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
    /// Whether the characters were recognised from pixels at all.
    ///
    /// Separate from `doubtful`, which is a shape heuristic over the words.
    /// This is the certain half: a card built on a screenshot and one built on
    /// a provider's own string looked identical, and the first is a guess at
    /// what the screen says.
    from_pixels: bool,
    /// Which application the text came out of.
    ///
    /// Carried only so the console line can name it. A reader's own session log
    /// showed 20 cards and 65 refusals across six applications, and the
    /// refusals named the application while the cards did not, so the log could
    /// say where Lens failed and not where it worked. That is the half the
    /// compatibility matrix is made of.
    app: Option<String>,
}

/// What the card on screen is showing, so the explain key has something to
/// explain.
///
/// Held rather than read back off the overlay, because the overlay stores a
/// `Card` and a card is display copy: pulling the sentence out of it would put
/// a parser between the reader's key and the model, and the parts here are the
/// parts that were sent in the first place.
struct Explainable {
    original: String,
    focused_word: Option<String>,
    anchor: ScreenRect,
    /// See [`Explain::translation`].
    translation: Option<String>,
    /// See [`Translate::app`].
    app: Option<String>,
}

/// What the worker is asked to explain.
///
/// Carries the translation as well as the source, because the deep path is
/// grounded on the answer already on the screen rather than deriving its own.
/// Sending only the source would have the model explain a sentence the reader
/// is not looking at, on the strength of a second translation nobody saw.
struct Explain {
    generation: Generation,
    original: String,
    /// See [`Translate::app`].
    app: Option<String>,
    /// `None` when the reader can already read the source, which is not a
    /// missing value: there is no translation and there should not be one.
    translation: Option<String>,
    focused_word: Option<String>,
    anchor: ScreenRect,
    language: String,
}

/// What comes back, still carrying the generation it belongs to.
struct Answer {
    generation: Generation,
    card: Card,
    anchor: ScreenRect,
    /// Whether this is the finished answer or a piece of one still arriving.
    final_answer: bool,
    /// The application the text came out of, for the console line. See
    /// [`Translate::app`].
    app: Option<String>,
}

/// Bumped whenever the quick prompt changes, so a cache filled by the old
/// wording is not served under the new one.
const PROMPT_VERSION: u32 = 1;

/// The same, for the deep prompt.
///
/// Its own number because the two prompts change independently, and one shared
/// between them would throw away every translation whenever a heading in the
/// analysis was reworded (ADR-0005).
const DEEP_PROMPT_VERSION: u32 = 1;

/// How many sentences to remember. A reader revisits a paragraph, not a book,
/// and every entry is text the user read, so the bound is small on purpose.
const CACHE_ENTRIES: usize = 256;

/// How many explanations to remember.
///
/// Far fewer than sentences. An explanation is asked for by pressing a key, so
/// a reader produces a handful in a session against hundreds of hovers, and
/// each entry is five lines rather than one.
const ANALYSIS_ENTRIES: usize = 32;

const USAGE: &str = "lens · Mori Lens

USAGE
    lens [OPTIONS]

    Rest the pointer on a sentence. Hold Alt unless --auto is given.
    Ctrl+Alt+D explains the sentence the card is showing.
    Ctrl+Alt+C copies it, Ctrl+Alt+S saves it beside your settings file.
    Ctrl+C to quit.

OPTIONS
    --auto              Translate in every application, not only while Alt is
                        held, for this run. The lasting version of this is
                        `default = allow` in the settings file.
    --settings <path>   Use this settings file instead of the one in %APPDATA%.
                        Written with every option in it on first run.
    --language <name>   Target language. Default Korean.
    --ocr <mode>        off, manual, or automatic. Default manual: OCR runs on
                        Ctrl+Alt+O and never on its own. Automatic reads the
                        pixels around the pointer whenever an application
                        exposes no text, with no keypress, which is what an
                        editor or a messenger needs to be readable by hovering.
                        In an editor that is nearly every hover: measured at
                        36% of one core while reading, against 0.16% idle.
                        Nothing is written to disk either way.
    -V, --version       Print the version and the commit this was built from.
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
    /// When the OCR fallback may run.
    ocr_mode: OcrMode,
    /// Who is reading, which decides what Ctrl+Alt+D spends its lines on.
    profile: Profile,
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
    /// This run created the settings file, so nobody has used this before.
    ///
    /// The only signal available. There is no install-time hook to write a
    /// marker from, and inventing a second file to record what the first one
    /// already implies would leave two things to disagree.
    first_run: bool,
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
    ocr: Option<String>,
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
            "--version" | "-V" => {
                println!(
                    "Mori Lens {} · build {}",
                    env!("CARGO_PKG_VERSION"),
                    build_stamp()
                );
                std::process::exit(0);
            }
            "--endpoint" => flags.endpoint = Some(value("--endpoint")?),
            "--ocr" => flags.ocr = Some(value("--ocr")?),
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
                    first_run: false,
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
                    // Only when it was actually written. A file that could not
                    // be created is written again next launch, and a welcome
                    // that returned every launch would be a nag.
                    first_run: wrote,
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
        ocr_mode: flags
            .ocr
            .as_deref()
            .and_then(OcrMode::named)
            .or_else(|| OcrMode::named(&settings.ocr))
            .unwrap_or(OcrMode::Manual),
        // The settings string is already validated by the parser, which reports
        // an unknown value rather than storing it. `unwrap_or_default` is the
        // second line of that and not the first.
        profile: Profile::named(&settings.profile).unwrap_or_default(),
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

    // **Before anything is claimed.** A second Lens used to run to the end:
    // two tray icons, two overlays racing for the same hover, two model leases,
    // and both hotkeys reported as taken by another application, which was a
    // sentence about Lens naming something else. Clicking the tray to pause
    // then paused one of them.
    //
    // Easy to do, because Lens opens no window on launch: somebody who
    // double-clicks it and sees nothing double-clicks it again.
    let Some(_only_instance) = lens_platform_windows::single::claim() else {
        eprintln!("Mori Lens is already running. Its icon is in the notification area.");
        // On a card too, because a binary started from Explorer has nowhere to
        // print that and the reader just double-clicked something and is
        // watching the screen. Briefly: this process exists to say one thing.
        if let Some(overlay) = Overlay::start() {
            let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
            overlay.show(Card::already_running(), ScreenRect::new(at.x, at.y, 1, 1));
            std::thread::sleep(Duration::from_millis(2600));
            overlay.hide();
        }
        return std::process::ExitCode::SUCCESS;
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
    // First, because everything under it is a measurement of *this* build and
    // a reader pasting the lines has no other way to say which one.
    println!("  build     {}", build_stamp());
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
    if options.ocr_mode == OcrMode::Automatic {
        // Said at startup, because a reader who turned this on in a settings
        // file weeks ago is not going to re-read the file, and 36% of a core
        // is the kind of thing a laptop notices before its owner does.
        // One line. A `\`-continuation inside a literal comes back from
        // rustfmt rejoined with its indentation intact, which has now reached
        // a user once and a banner once.
        println!("  cost      36% of one core while reading, measured 2026-08-06");
    }
    println!(
        "  ocr       {} · {}",
        options.ocr_mode.as_str(),
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
    // Printed even when it is the default, because a setting the product read
    // and did not mention is a setting a reader cannot tell it read.
    println!(
        "  profile   {} · {}",
        options.profile.as_str(),
        match options.profile {
            Profile::Reader => "every line that can be answered",
            Profile::Developer => "the unusual sense and the jargon, no parts of speech",
            Profile::Exam => "the clause pattern and what governs what",
        },
    );
    println!(
        "  pin       {}",
        if pause.pin_is_registered() {
            "Ctrl+Alt+P keeps the card in a window that stays until Esc"
        } else {
            "UNAVAILABLE: Ctrl+Alt+P is taken by another application"
        },
    );
    println!(
        "  explain   {}",
        if pause.explain_is_registered() {
            "Ctrl+Alt+D explains the sentence on the card: form, part of speech, structure"
        } else {
            "UNAVAILABLE: Ctrl+Alt+D is taken by another application"
        }
    );
    println!(
        "  copy      {}",
        if pause.copy_is_registered() {
            "Ctrl+Alt+C puts the card on the clipboard, with the sentence it explains"
        } else {
            "UNAVAILABLE: Ctrl+Alt+C is taken by another application"
        }
    );
    println!(
        "  keep      {}",
        if pause.keep_is_registered() {
            // The path, because a reader who saves something and cannot find
            // it has not saved it.
            format!(
                "Ctrl+Alt+S appends it to {}",
                keeping::path_beside(&options.source.path).display(),
            )
        } else {
            "UNAVAILABLE: Ctrl+Alt+S is taken by another application".to_owned()
        }
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
        &tooltip(
            &options.language,
            &engine_line,
            false,
            &options.policy,
            &pause,
        ),
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
        let code = selftest(&overlay, &translator, &options.language, options.profile);
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
    // Whether the model has finished loading, for the waiting card to say
    // which wait it is.
    //
    // Lens releases the lease when it exits, so **every launch** pays the load
    // again: 11 to 14 seconds measured on 2026-08-11 with the model already
    // pulled and nothing left to download. A hover that arrives during it took
    // 7.4 seconds, and the card said `번역 중` for all of it, which is what a
    // hung application says.
    let engine_warm = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let warmed = engine_warm.clone();
    std::thread::spawn(move || {
        // Failure is silent by design: the user did not ask for this, so a
        // warm that fails must not produce an error state. It only means the
        // first hover pays for the load instead.
        if let Ok(elapsed) = warm_translator.prewarm() {
            println!("engine warm after {}ms", elapsed.as_millis());
        }
        // **Set on failure too.** An absent Ollama answers a hover with a card
        // naming ollama.com, not with a wait, so a flag left false would leave
        // every later card claiming a model was loading that nothing was ever
        // going to load.
        warmed.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    // Copied out before the move, the same way `worker_ocr` is further down.
    let worker_profile = options.profile;
    std::thread::spawn(move || {
        // The translator lives on the worker, not shared: it holds no state
        // worth sharing and this keeps the loop free of a lock.
        // When the last answer came back. Ollama's lease is thirty minutes and
        // every request resets it, so within that window the model is
        // certainly still in memory and there is nothing to ask about.
        let mut last_answer: Option<Instant> = None;
        // Its own map and its own keys, per ADR-0005. Explaining the same
        // sentence twice was two four-second generations before this.
        let mut explanations = AnalysisCache::new(ANALYSIS_ENTRIES);
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

            let job = match job {
                Job::Translate(job) => job,
                Job::Explain(job) => {
                    // The deep path. Grounded on the translation already on the
                    // screen, so this is one focused generation rather than a
                    // second full one (ADR-0005).
                    let mut request = match &job.translation {
                        Some(translation) => {
                            DeepRequest::new(&job.original, translation, &job.language)
                        }
                        None => DeepRequest::about_what_they_can_read(&job.original, &job.language),
                    }
                    .for_reader(worker_profile);
                    if let Some(word) = &job.focused_word {
                        request = request.about_the_word(word);
                    }

                    let key =
                        CacheKey::for_analysis(&request, translator.model(), DEEP_PROMPT_VERSION);
                    if let Some(hit) = explanations.get(&key) {
                        println!("deep  {} lines from memory", hit.lines().len());
                        let _ = answers_tx.send(Answer {
                            generation: job.generation,
                            card: Card::analysed(&job.original, &hit),
                            anchor: job.anchor,
                            final_answer: true,
                            app: job.app.clone(),
                        });
                        continue;
                    }

                    let card = match translator.deep(&request) {
                        Ok(analysis) => {
                            println!(
                                "deep  {} lines in {}ms",
                                analysis.lines().len(),
                                analysis.latency_ms,
                            );
                            explanations.insert(key, analysis.clone());
                            Card::analysed(&job.original, &analysis)
                        }
                        Err(error) => {
                            println!("deep  no analysis [{}]", error.reason());
                            Card::not_readable(error.user_message())
                        }
                    };
                    last_answer = Some(Instant::now());
                    if answers_tx
                        .send(Answer {
                            generation: job.generation,
                            card,
                            anchor: job.anchor,
                            final_answer: true,
                            app: job.app.clone(),
                        })
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
            };

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
                    app: job.app.clone(),
                });
            }

            let key = CacheKey::of(&request, translator.model(), PROMPT_VERSION);
            if let Some(hit) = cache.get(&key) {
                let mut card = Card::translated(&job.original, &hit);
                if job.doubtful {
                    card = card.built_on_a_doubtful_reading();
                } else if job.from_pixels {
                    card = card.read_from_the_screen();
                }
                let _ = answers_tx.send(Answer {
                    generation: job.generation,
                    card,
                    anchor: job.anchor,
                    final_answer: true,
                    app: job.app.clone(),
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
            // Cloned out before the move, so the job stays whole for the
            // finished answer below.
            let partial_app = job.app.clone();
            let mut on_partial = move |text: &str| {
                let card = Card::partial(&original, text);
                let _ = partial_tx.send(Answer {
                    generation,
                    card,
                    anchor,
                    final_answer: false,
                    app: partial_app.clone(),
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
            } else if job.from_pixels {
                card.read_from_the_screen()
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
                    app: job.app.clone(),
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
    // **The first hover, arranged rather than hoped for.**
    //
    // A reader who has not learnt the gesture and a reader whose window is
    // unsupported see the same thing, which is nothing
    // (`docs/what-finished-means.md` §1). Held for the life of the process so
    // its window closes when Lens does; the reader closes it sooner with Esc.
    //
    // **The heading names the modifier when the reader's rules need one.** The
    // shipped default is `modifier`, so the first launch after this window was
    // built printed `mode  hold Alt` directly under `welcome  staged`: a
    // lesson telling somebody to rest the pointer, above a product that would
    // not have read it.
    let mut welcome = lens_core::welcome::show_on(options.source.first_run)
        .then(|| lens_core::welcome::lesson_for(&options.language))
        .flatten()
        // **Asked of the reader's own default, not of Lens's entry.**
        //
        // `deny_self` denies this process, so asking about `lens` answered
        // `Deny` and staged nothing at all. That rule is right and it is about
        // the card: the welcome is the other Lens window, and it hands its
        // sentence over rather than being read, so there is no translation of
        // a translation to prevent.
        .zip(lens_core::welcome::heading_for(
            policy.default_tier(),
            pointer::MODIFIER_NAME,
        ))
        .and_then(|(lesson, heading)| {
            lens_platform_windows::welcome::Welcome::open(lesson, theme, heading)
        });
    if let Some(welcome) = &welcome {
        println!(
            "welcome    staged, {} lines, close it with Esc",
            welcome.placement().lines.len(),
        );
    }
    let taught = welcome.as_ref().map(|welcome| welcome.placement());

    // **The one window `deny_self` must not cover.**
    //
    // While the lesson is up, Lens's own process is readable, and nothing else
    // of Lens's is reachable by a pointer to be read: the card is
    // `WS_EX_TRANSPARENT`, so `WindowFromPoint` passes straight through it
    // (ADR-0004). Restored the moment the reader closes the window, below,
    // rather than left off for the session, because the interactive card
    // ADR-0004 also describes would not be click-through and this would be the
    // rule that saved it.
    let mut worker_policy = policy.clone();
    if welcome.is_some() {
        worker_policy.set("lens", policy.default_tier());
    }
    // **Acquisition runs here and nothing else does.** While a read is in
    // flight the pointer loop keeps sampling, cards keep retiring, and the
    // pause key keeps working, none of which was true: a wedged provider took
    // all three with it for as long as it took to give up (issue #42).
    let worker_ocr = options.ocr_mode;
    // Hiding the card is the overlay's business, and the overlay lives on
    // another thread again, so what crosses is a request rather than a handle.
    let hide_for_ocr = overlay.hider();
    // **How often this photographs the screen, counted.** Issue #87 says the
    // automatic fall-through captures far more often than a reader would
    // guess, that the privacy model says what is captured and never how often,
    // and that the change it proposes must be measured first. Nothing counted,
    // so nothing could be measured. Reported once, on the way out.
    let capture_cost = std::sync::Arc::new(lens_platform_windows::CaptureCost::default());
    let worker_cost = capture_cost.clone();
    let Some(mut hover) = hover_worker::HoverWorker::start(move || {
        let acquirer = UiaTextAcquirer::new().ok()?;
        // The lesson answers first and only for its own window, so the staged
        // hover travels this path rather than a second one built beside it.
        // Absent on every run but the first, and then this is the acquirer.
        let acquirer: Box<dyn lens_core::acquire::TextAcquirer> = match taught {
            Some(placement) => Box::new(lesson::TaughtFirst::new(placement, Box::new(acquirer))),
            None => Box::new(acquirer),
        };
        let session = HoverSession::new(worker_config, worker_policy, acquirer);
        // **Only for `Automatic`.** Installing it for `Manual` would look
        // tidier and do nothing: the session's manual branch fires on
        // `InputState::ocr_requested`, which this product never sets, because
        // `Ctrl+Alt+O` is served by `manual_ocr` instead. That path is not
        // redundant, it carries things a `TextTarget` has no room for: the
        // count of misread words for the card's footer, and its own refusals
        // in the reader's language. Two paths, and the honest arrangement is
        // to say so rather than to install something that has no effect.
        Some(match worker_ocr {
            OcrMode::Automatic => session.with_ocr(
                Box::new(lens_platform_windows::OcrTextAcquirer::new(
                    Box::new(hide_for_ocr),
                    worker_cost,
                )),
                OcrMode::Automatic,
            ),
            // `Off` and `Manual` both leave the session without a capture
            // path. They differ in what `Ctrl+Alt+O` does, which is the app's
            // business rather than the session's.
            _ => session,
        })
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
    // Said once, the first time a hover lands on the staged lesson.
    let mut lesson_landed = false;
    let mut pending_original = String::new();
    // What the reader is looking at, filled in when a translation finishes.
    // Not cleared when the card is retired: somebody who presses the key a
    // moment after the card faded meant the sentence they were just reading,
    // and the anchor puts the answer back where it was.
    let saved_to = keeping::path_beside(&options.source.path);
    let mut explainable: Option<Explainable> = None;
    // The card the reader is looking at, so `Ctrl+Alt+C` has something to put
    // on the clipboard and can put the same card back with a mark on it. Held
    // here rather than read out of the overlay, which owns a `Card` for
    // drawing and is not a place to look things up.
    let mut on_screen: Option<(Card, ScreenRect)> = None;
    // **One at a time**, which ADR-0012 says and this enforces by holding one.
    // Opening a second closes the first, so the screen cannot fill with panels.
    let mut pinned: Option<lens_platform_windows::DetailPanel> = None;
    // The word and the region of the outstanding translation, so the pair
    // above can be completed when the answer comes back.
    let mut pending_word: Option<String> = None;
    let mut pending_anchor = ScreenRect::new(0, 0, 1, 1);
    // Why no card appeared, said once per distinct reason. Holding the
    // modifier over a window that exposes no text used to look exactly like a
    // product that had crashed.
    let mut narrator = silence::Narrator::default();
    let mut reach = lens_core::reach::Reach::new();
    // Whether the acquisition thread's death has already been reported, so it
    // is said once rather than on every tick of a loop that keeps running.
    let mut said_it_died = false;
    // Whether the translation worker is still there. A failed send is the only
    // notice it gives, and it is enough.
    let mut translating = true;
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
        let mut explain = false;
        let mut copy = false;
        let mut keep = false;
        let mut pin = false;
        for press in pause.poll() {
            match press {
                pause::Pressed::PauseChanged => pause_changed = true,
                pause::Pressed::ReadWithOcr => read_with_ocr = true,
                pause::Pressed::Explain => explain = true,
                pause::Pressed::Copy => copy = true,
                pause::Pressed::Keep => keep = true,
                pause::Pressed::Pin => pin = true,
            }
        }

        if keep && !pause.is_paused() {
            // The durable half of the copy key, and the first thing this
            // product writes to disk that came off a screen. What makes that
            // allowed is that a person pressed a key looking at the card, so
            // the rule is the same as the clipboard's: nothing here is
            // reachable from a hover (`docs/privacy-model.md` §1).
            let under = pointer::position().and_then(pointer::process_name_at);
            match on_screen.as_ref().and_then(|(card, anchor)| {
                card.kept_as(under.as_deref()).map(|entry| (entry, *anchor))
            }) {
                Some((entry, anchor)) => {
                    match keeping::append(&saved_to, &entry, std::time::SystemTime::now()) {
                        Ok(()) => {
                            // Lengths and lines, never the text
                            // (`docs/privacy-model.md` §3). The path is not
                            // secret and is the useful half.
                            println!(
                                "keep  {} + {} chars -> {}",
                                entry.original.chars().count(),
                                entry.answer.chars().count(),
                                saved_to.display(),
                            );
                            if let Some((card, anchor)) = on_screen.take() {
                                show_card(&overlay, &mut on_screen, card.kept(), anchor);
                            }
                        }
                        Err(refusal) => {
                            println!("keep  {}", refusal.reason());
                            show_card(
                                &overlay,
                                &mut on_screen,
                                Card::not_readable(refusal.user_message()),
                                anchor,
                            );
                        }
                    }
                }
                None => {
                    let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
                    println!("keep  {}", keeping::NotKept::NothingToKeep.reason());
                    show_card(
                        &overlay,
                        &mut on_screen,
                        Card::not_readable(keeping::NotKept::NothingToKeep.user_message()),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
                }
            }
        }

        if pin && !pause.is_paused() {
            // Opened by a key and by nothing else. There is no path from a
            // settled hover to this: `Detail::from_card` takes a card the reader
            // has already been shown, which is what keeps a dwell from becoming
            // a window.
            match on_screen
                .as_ref()
                .and_then(|(card, _)| lens_core::detail::Detail::from_card(card))
            {
                Some(detail) => {
                    let explained = detail.is_explained();
                    // Taken and dropped first, so the old window is closed and
                    // its thread joined before the new one takes focus, rather
                    // than the two fighting for the front. Written this way
                    // because an assignment reads as dead and this is not.
                    drop(pinned.take());
                    pinned = lens_platform_windows::DetailPanel::open(
                        detail,
                        theme,
                        lens_core::detail::Detail::nothing_explained_yet("Ctrl+Alt+D"),
                    );
                    println!(
                        "pin   {} · {}",
                        if pinned.is_some() { "opened" } else { "FAILED" },
                        if explained {
                            "with the explanation"
                        } else {
                            "translation only, Ctrl+Alt+D adds the rest"
                        },
                    );
                }
                None => println!("pin   nothing on the card worth pinning"),
            }
        }

        // A reader closed it with Esc, on its own thread, without telling this
        // loop. Asked rather than tracked, so the handle is dropped and its
        // thread joined rather than left running until the next pin.
        if pinned.as_ref().is_some_and(|panel| !panel.is_open()) {
            pinned = None;
        }

        if copy && !pause.is_paused() {
            // Whatever is on the card, not whatever was last translated: a
            // reader pressing this means the thing in front of them, which may
            // be an explanation rather than a translation.
            match on_screen
                .as_ref()
                .and_then(|(card, anchor)| card.for_the_clipboard().map(|text| (text, *anchor)))
            {
                Some((text, anchor)) => {
                    // Length and lines, never the text (`docs/privacy-model.md`
                    // §3). This is the one place in the product that writes
                    // what was read anywhere at all, and it does it because
                    // somebody pressed a key for it.
                    if lens_platform_windows::clipboard::put_text(&text) {
                        println!(
                            "copy  {} chars, {} lines",
                            text.chars().count(),
                            text.lines().count(),
                        );
                        if let Some((card, anchor)) = on_screen.take() {
                            show_card(&overlay, &mut on_screen, card.copied(), anchor);
                        }
                    } else {
                        // Another application is holding the clipboard, which
                        // is ordinary and passes. Said on the card because the
                        // reader pressed a key and would otherwise conclude
                        // the key does nothing.
                        println!("copy  the clipboard would not open");
                        show_card(
                            &overlay,
                            &mut on_screen,
                            Card::not_readable("클립보드를 열지 못했습니다"),
                            anchor,
                        );
                    }
                }
                None => {
                    let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
                    println!("copy  nothing on screen to copy");
                    show_card(
                        &overlay,
                        &mut on_screen,
                        Card::not_readable("복사할 내용이 아직 없습니다"),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
                }
            }
        }

        if explain && !pause.is_paused() {
            match &explainable {
                Some(showing) => {
                    // A generation of its own, from the hover machine, exactly
                    // as manual OCR takes one: the answer has to be
                    // cancellable and an answer with no generation cannot be.
                    match hover.take_generation() {
                        Some(generation) => {
                            life.hover_settled(generation);
                            life.job_dispatched(generation, showing.anchor, started.elapsed());
                            // Said before the model is asked. An analysis
                            // measured 2.6 to 4.9 seconds warm, and a card
                            // that sits unchanged for four seconds after a
                            // keypress reads as a key that did nothing.
                            show_card(
                                &overlay,
                                &mut on_screen,
                                Card::explaining(&showing.original),
                                showing.anchor,
                            );
                            translating &= jobs_tx
                                .send(Job::Explain(Explain {
                                    generation,
                                    original: showing.original.clone(),
                                    translation: showing.translation.clone(),
                                    focused_word: showing.focused_word.clone(),
                                    anchor: showing.anchor,
                                    language: language.clone(),
                                    app: showing.app.clone(),
                                }))
                                .is_ok();
                        }
                        None => println!("deep  the acquisition thread did not answer"),
                    }
                }
                // The key is not silent when there is nothing to explain. A
                // reader who presses it over an empty desktop is owed the
                // reason, and the console is not where they are looking.
                None => {
                    let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
                    println!("deep  nothing on screen to explain");
                    show_card(
                        &overlay,
                        &mut on_screen,
                        Card::not_readable("설명할 문장이 아직 없습니다"),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
                }
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
                    &tooltip(&language, &engine_line, pause.is_paused(), &policy, &pause),
                    theme.body,
                );
            }
        }
        // **Put `deny_self` back the moment the lesson is gone.**
        //
        // The reader closes the welcome with Esc, on its own thread, without
        // telling anybody, so this is asked rather than waited for. Until it
        // happens Lens's own process is readable, which is safe only while the
        // welcome is the one Lens window a pointer can land on.
        if welcome.as_ref().is_some_and(|welcome| !welcome.is_open()) {
            welcome = None;
            hover.reconfigure(hover_config, policy.clone());
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

                // **Everything that is read once, gathered.** The model and
                // the address belong to the translation worker, which owns
                // them for its life: swapping one mid-flight would leave a
                // queued job answered by the model it did not ask for. The OCR
                // mode decides whether the acquisition thread is built with a
                // capture path at all.
                //
                // Only the model was named here, and `ocr` and `endpoint` were
                // ignored in silence. A reader who edits a file, saves it, and
                // sees nothing concludes the file does not work.
                let mut stale: Vec<&str> = Vec::new();
                if settings.model != options.model {
                    stale.push("model");
                }
                if settings.endpoint != options.endpoint {
                    stale.push("endpoint");
                }
                if OcrMode::named(&settings.ocr) != Some(options.ocr_mode) {
                    stale.push("ocr");
                }
                if !stale.is_empty() {
                    let keys = stale.join(", ");
                    println!("settings: {keys} changed; restart to use it");
                    // And on a card, because a Windows subsystem binary
                    // started from Explorer has nowhere to print that line,
                    // and the reader is looking at the screen: they just saved
                    // a file and are waiting to see what it did.
                    let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
                    show_card(
                        &overlay,
                        &mut on_screen,
                        Card::needs_a_restart(&keys),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
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
                    &tooltip(&language, &engine_line, pause.is_paused(), &policy, &pause),
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
                        // Said differently now, because it is no longer the
                        // end of what Lens can do with this sentence. The card
                        // that reported a dead end now names the key that is
                        // not one.
                        explainable = Some(Explainable {
                            original: reading.sentence.clone(),
                            focused_word: Some(reading.word.clone()),
                            anchor: reading.anchor,
                            translation: None,
                            app: pointer::process_name_at(at),
                        });
                        show_card(
                            &overlay,
                            &mut on_screen,
                            Card::already_readable("Ctrl+Alt+D"),
                            reading.anchor,
                        );
                    } else {
                        life.job_dispatched(generation, reading.anchor, started.elapsed());
                        pending_original = reading.sentence.clone();
                        pending_word = Some(reading.word.clone());
                        pending_anchor = reading.anchor;
                        // A failed send means the translation worker has
                        // gone. Recorded rather than discarded, for the same
                        // reason the acquisition thread's death is: a product
                        // that keeps running while nothing answers is worse
                        // than one that says so.
                        translating &= jobs_tx
                            .send(Job::Translate(Translate {
                                generation,
                                original: reading.sentence,
                                focused_word: Some(reading.word),
                                anchor: reading.anchor,
                                language: language.clone(),
                                doubtful: reading.misread > 0,
                                app: pointer::process_name_at(at),
                                // Ctrl+Alt+O reads pixels and nothing else.
                                from_pixels: true,
                            }))
                            .is_ok();
                    }
                }
                Err(refusal) => {
                    // On the card, not only in a terminal nobody is looking
                    // at: the user pressed a key and is owed an answer.
                    println!("ocr   no reading [{}]", refusal.reason());
                    show_card(
                        &overlay,
                        &mut on_screen,
                        Card::not_readable(refusal.user_message()),
                        ScreenRect::new(at.x, at.y, 1, 1),
                    );
                }
            }
        }

        // Said once, and said everywhere a person might be looking. A worker
        // that has gone leaves the product running and reading nothing, and
        // until this the tray went on saying it was listening.
        if (hover.has_died() || !translating) && !said_it_died {
            said_it_died = true;
            eprintln!(
                "{} has stopped; restart Lens",
                if hover.has_died() {
                    "the acquisition thread"
                } else {
                    "the translation thread"
                },
            );
            if let Some(tray) = &tray {
                tray.set_state(
                    Glyph::Paused,
                    "Mori Lens · 읽기가 멈췄습니다. 다시 실행해 주세요",
                    theme.problem,
                );
            }
            let at = pointer::position().unwrap_or(ScreenPoint::new(0, 0));
            show_card(
                &overlay,
                &mut on_screen,
                Card::not_readable("읽기가 멈췄습니다 · 다시 실행해 주세요"),
                ScreenRect::new(at.x, at.y, 1, 1),
            );
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
                show_card(
                    &overlay,
                    &mut on_screen,
                    Card::hold_the_modifier(pointer::MODIFIER_NAME),
                    anchor,
                );
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
                    // **Say when the staged hover is the one that landed.**
                    //
                    // `AcquisitionSource::Lesson` was added so that no log
                    // could mistake a lesson for an acquisition, and then no
                    // log said anything at all. The first live run left the
                    // console unable to answer the only question the welcome
                    // exists to settle: a card carrying the lesson's own
                    // sentence appeared, and nothing distinguished a hover
                    // that reached it from a `Ctrl+Alt+O` that read it off the
                    // screen.
                    //
                    // Once, because it is the answer to a question rather than
                    // a running commentary.
                    if target.source == lens_core::acquire::AcquisitionSource::Lesson
                        && !lesson_landed
                    {
                        lesson_landed = true;
                        println!("lesson     a hover reached the welcome's own sentence");
                    }
                    let sentence = target.segmented.sentence_text().trim().to_owned();
                    // Text already in the reader's language costs nothing and
                    // shows nothing. Without this a Korean reader hovering
                    // over Korean waits a second and a half for a card that
                    // re-words their own sentence, which on a mostly-Korean
                    // screen is most hovers.
                    if decide(&sentence, &language) == Decision::AlreadyInTarget {
                        // No card, for the reason below: a Korean reader
                        // hovering Korean does not want it re-worded.
                        //
                        // **But it is remembered**, so `Ctrl+Alt+D` can
                        // explain it. A word somebody can read and does not
                        // know is the commonest thing on a technical screen,
                        // and until now this branch dropped the sentence on
                        // the floor and left the whole of the reader's own
                        // language outside the product.
                        let anchor = target
                            .card_anchor(point)
                            .or_else(|| target.anchor())
                            .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1));
                        explainable = Some(Explainable {
                            original: sentence,
                            focused_word: target.segmented.word_text().map(str::to_owned),
                            anchor,
                            translation: None,
                            app: target.app.process_name.clone(),
                        });
                        if let Some(line) = narrator.line_for_skip(
                            &format!("already in {language}, Ctrl+Alt+D explains it"),
                            process_name.as_deref(),
                        ) {
                            println!("{line}");
                        }
                        // Said once a session, on a card, because a release
                        // binary has no console and a capability nobody can
                        // discover is the silent product `reach` exists to
                        // prevent. Only when the key is registered: naming a
                        // chord another application is holding would be worse
                        // than saying nothing.
                        if pause.explain_is_registered() && reach.readable_without_translation() {
                            let anchor = explainable
                                .as_ref()
                                .map(|showing| showing.anchor)
                                .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1));
                            reach.card_shown();
                            show_card(
                                &overlay,
                                &mut on_screen,
                                Card::already_readable("Ctrl+Alt+D"),
                                anchor,
                            );
                        }
                        continue;
                    }
                    // A card is about to appear, so the first-run hint has
                    // nothing left to explain. **Moved below the branch
                    // above**, which returns without showing one: a reader
                    // whose screen is their own language had the modifier hint
                    // cancelled by hovers that drew nothing.
                    reach.card_shown();
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
                        pending_word = target.segmented.word_text().map(str::to_owned);
                        pending_anchor = anchor;
                        // A failed send means the translation worker has
                        // gone. Recorded rather than discarded, for the same
                        // reason the acquisition thread's death is: a product
                        // that keeps running while nothing answers is worse
                        // than one that says so.
                        translating &= jobs_tx
                            .send(Job::Translate(Translate {
                                generation,
                                original: sentence,
                                focused_word: target.segmented.word_text().map(str::to_owned),
                                anchor,
                                language: language.clone(),
                                app: target.app.process_name.clone(),
                                // No shape heuristic has run on this reading.
                                // The automatic fall-through does not count
                                // misread words the way `Ctrl+Alt+O` does, so
                                // the honest answer is that nobody looked.
                                doubtful: false,
                                // **Not always false.** This said "the
                                // accessibility path reads text, not pixels",
                                // and it stopped being true when `ocr =
                                // automatic` gave the session a capture path:
                                // a hover accessibility could not serve falls
                                // through to pixels and arrives here with a
                                // target the reader has no way to tell apart.
                                from_pixels: target.source
                                    == lens_core::acquire::AcquisitionSource::Ocr,
                            }))
                            .is_ok();
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
                            // Said, because a mechanism nobody can see in a log
                            // cannot be told apart from one that never fired. A
                            // reader's session showed 42 of these refusals and
                            // no way to know whether the offer had been made.
                            println!(
                                "offer  {} exposes no text; naming the OCR key, once for it",
                                process_name.as_deref().unwrap_or("that application"),
                            );
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
                            show_card(&overlay, &mut on_screen, card, anchor);
                        }
                    }
                }
            }
        }

        // Say the sentence was heard, if the answer is taking long enough
        // that the screen would otherwise be empty. Once per translation, and
        // never for one the cache already answered.
        if let Some(anchor) = life.waited_long_enough(now) {
            // Which wait it is. `Card::loading_model` and the sentence it
            // carries were written when the deep path landed and then never
            // called, so the one wait a reader cannot explain to themselves
            // was the one the card described wrongly.
            let card = if engine_warm.load(std::sync::atomic::Ordering::Relaxed) {
                Card::waiting(&pending_original)
            } else {
                Card::loading_model(&pending_original)
            };
            show_card(&overlay, &mut on_screen, card, anchor);
        }

        // With where the card is, so reading it does not dismiss it. The
        // distance rule measures the pointer against the text, and the card
        // sits below the text.
        if life.pointer_moved(position, now, lens_platform_windows::overlay::card_rect())
            == lifecycle::Action::Hide
        {
            overlay.hide();
        }

        while let Ok(answer) = answers_rx.try_recv() {
            if answer.final_answer {
                log_card(&answer.card, answer.app.as_deref());
                // The reason the panel exists. An explanation arrives and the
                // translation the reader was already reading stays where it is,
                // instead of them re-hovering to get one and then the other.
                //
                // Only when it is about the sentence on the panel: a hover that
                // moved on while the panel stayed open must not rewrite it.
                if let Some(panel) = pinned.as_ref() {
                    if let Some(fresh) = lens_core::detail::Detail::from_card(&answer.card) {
                        panel.update(fresh);
                    }
                }
                // A finished translation is a sentence the reader can now ask
                // about. Only a translation: an analysis must not become the
                // thing the next analysis explains, and a problem card has no
                // sentence behind it.
                if let lens_core::card::CardBody::Translation(text) = &answer.card.body {
                    explainable = Some(Explainable {
                        original: answer.card.original.clone(),
                        focused_word: pending_word.clone(),
                        anchor: pending_anchor,
                        translation: Some(text.clone()),
                        app: answer.app.clone(),
                    });
                }
            }
            // The generation check lives in the lifecycle, so an answer for a
            // hover the reader has left is dropped in one place rather than at
            // each call site that might forget.
            if life.answer_arrived(answer.generation, answer.anchor) == lifecycle::Action::Show {
                show_card(&overlay, &mut on_screen, answer.card, answer.anchor);
            }
        }

        std::thread::sleep(Duration::from_millis(25));
    }

    overlay.hide();
    drop(tray);
    // Hand the model back. Without this the runtime holds it for the rest of
    // the lease, so a tool that has exited goes on occupying gigabytes of
    // somebody's memory.
    if let Some(line) = capture_cost.summary() {
        println!("{line}");
    }
    releaser.release();
    println!("\nstopped, and the model released");
    std::process::ExitCode::SUCCESS
}

/// The commit this binary was built from, baked in by `build.rs`.
///
/// Printed on every run and by `--version`. A stale binary produces output that
/// looks entirely ordinary, and on 2026-08-06 a reader pasted a session log
/// that neither of us could date: a defect fixed an hour earlier was read as
/// still present because nothing in the lines said which build had produced
/// them. Comparing this against `git rev-parse --short HEAD` is the whole
/// defence, and it only works if the number is in front of somebody.
fn build_stamp() -> &'static str {
    env!("LENS_BUILD_SHA")
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
fn tooltip(
    language: &str,
    engine: &str,
    paused: bool,
    policy: &AppPolicy,
    keys: &pause::PauseKey,
) -> String {
    // **Second, not last.** The tooltip is a fixed 127 units and the engine
    // name is the longest thing in it, so anything appended after that is what
    // gets cut. A hotkey nobody can use is the exceptional line and the one
    // worth the space.
    //
    // It was in the banner only, which a reader who double-clicked the binary
    // has never seen. They would press the key, get nothing, and conclude the
    // product was broken, which is the failure this file keeps meeting.
    let trouble = hotkey_trouble(
        keys.is_registered(),
        keys.ocr_is_registered(),
        keys.explain_is_registered(),
        keys.copy_is_registered(),
        keys.keep_is_registered(),
    );
    format!(
        "Mori Lens · {}{trouble}
{}
{language} · {engine}",
        if paused { "일시정지" } else { "켜짐" },
        if paused {
            "다시 켜려면 클릭".to_owned()
        } else {
            korean_how_to(policy)
        },
    )
}

/// The tooltip line for a hotkey another application is holding, if any.
///
/// Separate so it can be tested: a `PauseKey` registers when it is built, so
/// a test cannot ask for one that failed without holding the combination
/// itself.
///
/// Empty when all three work, which is the ordinary case and must cost
/// nothing.
///
/// **One taken key is named; two or more are not.** The shell keeps 127 UTF-16
/// units and three chord names plus the sentence would not leave room for the
/// engine, so the generic form is what a reader gets when several are gone.
/// Naming one of three would be worse than naming none: it reads as a
/// complete account.
fn hotkey_trouble(
    pause_registered: bool,
    ocr_registered: bool,
    explain_registered: bool,
    copy_registered: bool,
    keep_registered: bool,
) -> String {
    let taken: Vec<&str> = [
        (pause_registered, "Ctrl+Alt+L"),
        (ocr_registered, "Ctrl+Alt+O"),
        (explain_registered, "Ctrl+Alt+D"),
        (copy_registered, "Ctrl+Alt+C"),
        (keep_registered, "Ctrl+Alt+S"),
    ]
    .into_iter()
    .filter_map(|(held, name)| (!held).then_some(name))
    .collect();

    if taken.is_empty() {
        return String::new();
    }
    let what = match taken.as_slice() {
        [only] => format!("{only}을 다른 앱이 쓰고 있습니다"),
        _ => "단축키를 다른 앱이 쓰고 있습니다".to_owned(),
    };
    // Pause is the privacy control. When it is one of the ones that is gone,
    // the line names the way that still works rather than only the way that
    // does not, because the tray icon does the same thing on a click.
    if pause_registered {
        format!(
            "
{what}"
        )
    } else {
        format!(
            "
{what} · 클릭해 일시정지"
        )
    }
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

/// Draw a card and remember that it is the one on the screen.
///
/// **One place, because there were fourteen.** `Ctrl+Alt+C` copies "the card",
/// and only the branch that received a finished translation was recording what
/// that was. Every other card the loop draws left the previous one recorded, so
/// a reader looking at `읽을 수 있는 언어라 번역하지 않았습니다` who pressed the
/// copy key got a translation of a different sentence, copied and redrawn at a
/// different anchor.
///
/// Nothing is recorded when the overlay refuses to draw. A card that was not
/// shown is not the card on the screen, and `Card::is_showable` refuses more
/// than one shape.
fn show_card(
    overlay: &Overlay,
    on_screen: &mut Option<(Card, ScreenRect)>,
    card: Card,
    anchor: ScreenRect,
) -> bool {
    let drawn = overlay.show(card.clone(), anchor);
    if drawn {
        *on_screen = Some((card, anchor));
    }
    drawn
}

/// One line per card, shape only.
///
/// Length and language, never content. A release build that logged what the
/// user was reading would undo the whole privacy model
/// (`docs/privacy-model.md` §3).
fn log_card(card: &Card, app: Option<&str>) {
    println!(
        "card  {}original={} chars  {}  [{}]",
        // Named, because the refusal lines already are and the cards were not.
        // A reader's own session showed 20 cards and 65 refusals over six
        // applications, and the log could say where Lens failed and not where
        // it worked, which is the half a compatibility matrix is made of.
        match app {
            Some(name) => format!("[{name}] "),
            None => String::new(),
        },
        card.original.chars().count(),
        match &card.body {
            lens_core::card::CardBody::Translation(text) =>
                format!("translated={} chars", text.chars().count()),
            lens_core::card::CardBody::Problem(_) => "problem".to_owned(),
            lens_core::card::CardBody::Waiting(status) => format!("waiting: {status}"),
            // Lines and characters, like everything else here. The headings
            // are this repository's own strings and the values are a model's
            // account of text off somebody's screen, so the values are counted
            // and not printed.
            lens_core::card::CardBody::Analysis {
                translation, rows, ..
            } => format!(
                "analysis={} rows, {} chars, translation={}",
                rows.len(),
                rows.iter()
                    .map(|(label, value)| label.chars().count() + value.chars().count())
                    .sum::<usize>(),
                // Its length, like every other reader's text in this line.
                translation
                    .as_deref()
                    .map(|text| format!("{} chars", text.chars().count()))
                    .unwrap_or_else(|| "none".to_owned()),
            ),
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
    profile: Profile,
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

    // Asked before the screen is scanned, because a selftest that spends
    // twenty seconds looking for a sentence and then says it passed without a
    // model having answered is a diagnostic saying something it did not test.
    //
    // It did exactly that: with no Ollama listening, it found a Korean
    // sentence, skipped the translation correctly, and printed
    // `selftest passed`. The reader's whole reason for running it was to find
    // out whether the thing works.
    if let Err(error) = translator.installed_models() {
        println!(
            "0. no engine  {} ({})",
            error.user_message(),
            error.reason()
        );
        if let Some(remedy) = error.remedy() {
            println!("              {remedy}");
        }
        println!("\nselftest FAILED: there is nothing to translate with, so nothing was tested.");
        return std::process::ExitCode::from(1);
    }
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

    /// How much of the budget one window may spend.
    ///
    /// **A quarter, because the first window used to spend all of it.** The
    /// grid steps 24px over a client area, so a 1440x900 window is 2,220
    /// points and a point costs 40 to 80ms on a loaded machine. The whole
    /// budget went on the first window in the list whether or not it was
    /// promising, and the diagnostic then told a reader with two editors and a
    /// browser open to "open something with prose in it".
    ///
    /// Measured: two runs in a row probed 252 and 258 points, hit the budget,
    /// and reported nothing found, on a desktop where an earlier run had found
    /// a sentence in the first window it looked at.
    const PER_WINDOW: Duration = Duration::from_secs(5);

    let scan_started = Instant::now();
    let mut probed = 0usize;
    let mut found = None;

    // The window in front first. It is the one the reader is looking at, so it
    // is both the likeliest to hold prose and the one they would expect a
    // diagnostic to try. `foreground_window` failing is ordinary and only
    // costs the ordering.
    let mut windows = top_level_windows();
    if let Ok(front) = lens_platform_windows::foreground_window() {
        if let Some(at) = windows.iter().position(|window| window.id == front.id) {
            windows.swap(0, at);
        }
    }

    'outer: for window in windows {
        let window_started = Instant::now();
        let bounds = window.client_bounds;
        let step = 24;
        let mut y = bounds.y + 8;
        while y < bounds.bottom() - 8 {
            let mut x = bounds.x + 8;
            while x < bounds.right() - 8 {
                if scan_started.elapsed() > SCAN_BUDGET {
                    break 'outer;
                }
                if window_started.elapsed() > PER_WINDOW {
                    // Enough of this one. Another window may answer, and the
                    // reader is waiting on the first answer rather than on a
                    // complete survey.
                    continue 'outer;
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
        println!(
            "The window in front was tried first, then the rest, {}s each.",
            PER_WINDOW.as_secs(),
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
        // Honest about its own reach. Acquisition, targeting and the language
        // decision were all exercised; the model, the card and the overlay
        // were not, and a line saying "passed" without that distinction is
        // what sent a reader away satisfied on a machine with no engine.
        println!(
            "
selftest passed as far as it went: the text on screen needed no translation,
so nothing after the language check was exercised. Open something in a
language you do not read and run it again."
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
    // Kept, because step 4 explains what step 2 translated rather than
    // translating it again. That grounding is the deep path's whole shape.
    let mut translated: Option<String> = None;
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
            translated = Some(translation.text.clone());
            Card::translated(&sentence, &translation)
        }
        Err(error) => {
            println!(
                "2. not translated: {} ({})",
                error.user_message(),
                error.reason()
            );
            if let Some(remedy) = error.remedy() {
                println!("              {remedy}");
            }
            Card::not_translated(&sentence, &error)
        }
    };
    // Remembered, because the card is still worth showing and drawing it is
    // still worth checking: the reader should see what the product does when
    // the model refuses. What must not happen is calling that a pass.
    let translation_failed = translated.is_none();

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

    // The deep path, on the sentence that was just translated, exactly as
    // Ctrl+Alt+D would. Last because it is the slow half of ADR-0005 and a
    // selftest that ran it first would look like a product that takes five
    // seconds to answer.
    if let Some(translation) = translated {
        overlay.show(Card::explaining(&sentence), anchor);
        let mut deep = DeepRequest::new(&sentence, &translation, language).for_reader(profile);
        if let Some(word) = &word {
            deep = deep.about_the_word(word);
        }
        let answerable = deep.answerable();
        println!(
            "4. explaining  word={} structure={}",
            answerable.the_word, answerable.the_structure
        );
        match translator.deep(&deep) {
            Ok(analysis) => {
                println!(
                    "   analysed   {} lines in {}ms via {}",
                    analysis.lines().len(),
                    analysis.latency_ms,
                    analysis.model,
                );
                // The headings, not the values. The headings are this
                // repository's own strings; the values are a model's account
                // of text off somebody's screen (`docs/privacy-model.md` §3).
                for (heading, text) in analysis.lines() {
                    println!("   {heading:<12} {} chars", text.chars().count());
                }
                // The same press again, which is what a reader who lost the
                // card and pressed the key a second time produces. Its own
                // cache, its own key (ADR-0005), and four seconds either way.
                let mut explanations = AnalysisCache::new(ANALYSIS_ENTRIES);
                let key = CacheKey::for_analysis(&deep, translator.model(), DEEP_PROMPT_VERSION);
                explanations.insert(key.clone(), analysis.clone());
                let again = Instant::now();
                match explanations.get(&key) {
                    Some(hit) => println!(
                        "   cache      the same sentence again in {}us, {} lines, cached={}",
                        again.elapsed().as_micros(),
                        hit.lines().len(),
                        hit.cached,
                    ),
                    None => println!("   cache      MISS on an analysis just inserted"),
                }

                let card = Card::analysed(&sentence, &analysis);

                // Step 5, before the card is drawn, because drawing sleeps for
                // four seconds and this is the part that can fail.
                //
                // **Into a temporary file, never the reader's own.** A
                // diagnostic that appended to somebody's notes every time they
                // ran it would be writing their file for them, and the whole
                // point of that file is that they decide what goes in it.
                let scratch = std::env::temp_dir().join("mori-lens-selftest-saved.md");
                let _ = std::fs::remove_file(&scratch);
                match card
                    .kept_as(Some("selftest"))
                    .ok_or(keeping::NotKept::NothingToKeep)
                    .and_then(|entry| {
                        keeping::append(&scratch, &entry, std::time::SystemTime::now())
                    }) {
                    Ok(()) => {
                        let written = std::fs::read_to_string(&scratch).unwrap_or_default();
                        // Shape, not content: the file holds a sentence off
                        // this machine's screen (`docs/privacy-model.md` §3).
                        println!(
                            "5. saved      {} lines, {} chars, label {}",
                            written.lines().count(),
                            written.chars().count(),
                            written.contains(lens_core::analyse::DeepAnalysis::LABEL),
                        );
                        if !written.contains(lens_core::analyse::DeepAnalysis::LABEL) {
                            println!("   REFUSED    an explanation was saved without its label");
                            return std::process::ExitCode::from(1);
                        }
                        let _ = std::fs::remove_file(&scratch);
                    }
                    Err(refusal) => {
                        println!(
                            "5. not saved: {} ({})",
                            refusal.user_message(),
                            refusal.reason()
                        );
                        return std::process::ExitCode::from(1);
                    }
                }

                if !overlay.show(card, anchor) {
                    println!("   REFUSED    the analysis card was not showable");
                    return std::process::ExitCode::from(1);
                }
                println!("   shown      on screen for 4 seconds");
                std::thread::sleep(Duration::from_secs(4));
            }
            Err(error) => println!(
                "4. not explained: {} ({})",
                error.user_message(),
                error.reason(),
            ),
        }
    }

    overlay.hide();
    std::thread::sleep(Duration::from_millis(200));
    if translation_failed {
        println!("\nselftest FAILED: a card was drawn and it says the model could not answer.");
        return std::process::ExitCode::from(1);
    }
    println!(
        "
selftest passed: text on screen became a card, pointer untouched."
    );
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tooltip_tests {
    use super::{hotkey_trouble, korean_how_to};
    use lens_core::policy::AppPolicy;

    /// The shell gives a tooltip 127 UTF-16 units and truncates the rest, so
    /// the worst case has to fit or the last line is what disappears.
    ///
    /// Built the way `tooltip` builds it, with the longest of everything: the
    /// longest trouble line any combination can produce, a long model name,
    /// and the policy sentence.
    #[test]
    fn the_worst_tooltip_fits_what_the_shell_will_show() {
        // Every combination, so the longest is measured rather than assumed.
        // The obvious candidate is all three taken, and it is not: that one
        // collapses to the generic sentence and one named key is longer.
        let worst = (0..32)
            .map(|bits| {
                hotkey_trouble(
                    bits & 1 == 0,
                    bits & 2 == 0,
                    bits & 4 == 0,
                    bits & 8 == 0,
                    bits & 16 == 0,
                )
            })
            .max_by_key(|line| line.encode_utf16().count())
            .expect("thirty-two combinations");

        let built = format!(
            "Mori Lens · {}{worst}
{}
{} · {}",
            "일시정지",
            korean_how_to(&AppPolicy::default()),
            "Korean",
            // Longer than anything named in the benchmark directory.
            "qwen2.5-coder:7b-instruct-q8_0",
        );
        let units = built.encode_utf16().count();
        assert!(
            units < 127,
            "the tooltip is {units} units and the shell keeps 127:
{built}",
        );
    }

    /// The ordinary case costs nothing.
    #[test]
    fn five_working_hotkeys_add_no_line() {
        assert!(hotkey_trouble(true, true, true, true, true).is_empty());
    }

    /// One taken key names itself, and the pause line names the way that still
    /// works, because pause is the privacy control and the tray icon does the
    /// same thing on a click.
    #[test]
    fn a_taken_hotkey_names_itself_and_the_way_round_it() {
        let pause_gone = hotkey_trouble(false, true, true, true, true);
        assert!(pause_gone.contains("Ctrl+Alt+L"), "{pause_gone}");
        assert!(pause_gone.contains("클릭"), "{pause_gone}");

        let ocr_gone = hotkey_trouble(true, false, true, true, true);
        assert!(ocr_gone.contains("Ctrl+Alt+O"), "{ocr_gone}");
        assert!(!ocr_gone.contains("클릭"), "{ocr_gone}");

        let explain_gone = hotkey_trouble(true, true, false, true, true);
        assert!(explain_gone.contains("Ctrl+Alt+D"), "{explain_gone}");

        let copy_gone = hotkey_trouble(true, true, true, false, true);
        assert!(copy_gone.contains("Ctrl+Alt+C"), "{copy_gone}");

        let keep_gone = hotkey_trouble(true, true, true, true, false);
        assert!(keep_gone.contains("Ctrl+Alt+S"), "{keep_gone}");
    }

    /// Two or more taken and none is named.
    ///
    /// Naming one of three reads as a complete account of what is wrong, and
    /// naming three does not fit in what the shell keeps.
    #[test]
    fn several_taken_hotkeys_are_reported_without_naming_one() {
        for line in [
            hotkey_trouble(false, false, true, true, true),
            hotkey_trouble(false, true, false, true, true),
            hotkey_trouble(true, false, false, true, true),
            hotkey_trouble(true, true, false, false, true),
            hotkey_trouble(true, true, true, false, false),
            hotkey_trouble(false, false, false, false, false),
        ] {
            assert!(!line.is_empty());
            assert!(!line.contains("Ctrl+Alt+"), "{line}");
            assert!(line.contains("단축키"), "{line}");
        }
    }
}
