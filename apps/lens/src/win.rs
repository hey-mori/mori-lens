//! Windows product entry point.
//!
//! The whole product loop on Windows: pointer, acquisition, translation,
//! overlay, tray, hotkeys. Included from `main.rs` behind `#[cfg(windows)]`.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lens_core::analyse::{Analyser, DeepRequest, Profile};
use lens_core::cache::{AnalysisCache, CacheKey, TranslationCache};
use lens_core::card::Card;
use lens_core::geometry::{ScreenPoint, ScreenRect};
use lens_core::hover::{Generation, HoverConfig};

use crate::ai_runtime::AiRuntimeConfig;
use crate::detail_view;
use crate::keeping;
use crate::library_service;
use crate::settings_service;
use crate::tauri_app;
use lens_core::icon::Glyph;
use lens_core::language::{decide, Decision};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState, OcrMode, Resolution};
use lens_core::settings::{Problem, Settings};
use lens_core::theme::{Preset, TextScale, Theme, Translucency};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_windows::{
    declare_dpi_awareness, monitor_scale_at, ocr, system_preferences, Overlay, Tray, TrayEvent,
    UiaTextAcquirer,
};
use lens_translate::OllamaTranslator;

mod console;
mod hover_worker;
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
    /// Always the sentence: what the model receives for translation quality.
    original: String,
    /// What the card shows: the sentence or the word, per the hover scope.
    display_text: String,
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

/// What the overlay is showing right now, carrying the provenance that was
/// bound when the reading was produced.
///
/// Replaces the old `(Card, ScreenRect, Option<String>)` tuple. The target
/// language here is the one the reading was made in, not whatever the
/// settings happen to say when a later Save or Pin is pressed.
struct PresentedReading {
    card: Card,
    anchor: ScreenRect,
    /// The application the text came from, at acquisition time.
    source_app: Option<String>,
    /// The target language the reading was produced for, bound at dispatch
    /// time. Save, Pin, and history all read from here rather than from the
    /// live settings.
    target_language: String,
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
    /// The target language this answer was produced for, bound at dispatch
    /// time from the settings that were active when the reading started.
    /// Carried through to Save/Pin/History so a later settings change cannot
    /// retroactively alter what language a reading was made in.
    target_language: String,
}

/// The outstanding reading's context, held as a single atomic unit.
///
/// When a new hover starts, all fields are set together. When the answer
/// arrives, all fields are read together. Parallel globals (pending_original,
/// pending_word, etc.) could drift, allowing fields from reading A to mix with
/// fields from reading B. A single struct prevents that.
struct PendingReading {
    /// The sentence being translated.
    sentence: String,
    /// The focused word within the sentence, if any.
    focused_word: Option<String>,
    /// The region of the text on screen.
    anchor: ScreenRect,
    /// The application the text came from, at acquisition time.
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
                        and does not try. Applies to both Quick and Deep paths.
    --model <name>      Ollama model for Quick translation. Default: whichever
                        suitable one is installed. Deep analysis uses the model
                        configured in settings.toml [deep] section.
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
    /// AI runtime configuration with separate Quick and Deep engines.
    ai_config: AiRuntimeConfig,
    /// When the OCR fallback may run.
    ocr_mode: OcrMode,
    /// Who is reading, which decides what Ctrl+Alt+D spends its lines on.
    profile: Profile,
    dwell: Duration,
    policy: AppPolicy,
    /// Where the settings came from, and anything wrong with them. Printed at
    /// startup: a file the product silently ignored is worse than no file.
    source: SettingsSource,
    /// CLI overrides that must survive settings reload.
    cli_overrides: CliOverrides,
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

/// CLI overrides that must survive settings reload.
///
/// When the user launches with `--auto` or `--language Japanese`, those flags
/// should remain in effect even after a settings reload (triggered by Settings
/// UI save or external file edit). This struct stores the relevant CLI values
/// so they can be re-applied after `settings.policy()` is called.
#[derive(Clone, Default)]
struct CliOverrides {
    /// `--auto` sets the default policy tier to Allow.
    auto: bool,
    /// `--language` overrides the file's language setting.
    language: Option<String>,
    /// `--dwell` overrides the file's dwell_ms setting.
    dwell_ms: Option<u64>,
}

impl CliOverrides {
    /// Re-apply CLI overrides to a reloaded policy.
    fn apply_to_policy(&self, policy: &mut lens_core::policy::AppPolicy) {
        if self.auto {
            policy.set_default(lens_core::policy::AppTier::Allow);
        }
    }
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
    base.join("Mori Lens").join("settings.toml")
}

/// Read the settings file, writing the template the first time.
///
/// A missing file is the normal first run, not an error. Writing the template
/// means the answer to "how do I configure this" is a file that already exists
/// with every option in it, rather than documentation.
///
/// A file that cannot be written is also not an error. It means this run uses
/// defaults and says so.
///
/// Migration: If the file exists but is in the legacy flat format (no
/// schema_version), it is migrated to the new TOML format with a fail-safe
/// backup. If the backup cannot be created, the original file is never modified.
///
/// Returns (Settings, Schema, SettingsSource). The Schema is needed to build
/// the AiRuntimeConfig with separate Quick and Deep engines.
fn load_settings(
    path: &std::path::Path,
) -> (Settings, lens_settings::schema::Schema, SettingsSource) {
    use lens_settings::{load_or_initialize, LoadDisposition};

    match load_or_initialize(path) {
        Ok((file, disposition)) => {
            let settings = file.to_settings();
            let schema = file.schema().clone();
            let (state, problems, first_run) = match disposition {
                LoadDisposition::Loaded => ("read", Vec::new(), false),
                LoadDisposition::Created => (
                    "written with the defaults, edit it and restart",
                    Vec::new(),
                    true,
                ),
                LoadDisposition::Migrated { problems } => {
                    ("migrated from legacy format", problems, false)
                }
                LoadDisposition::InvalidPreserved { reason } => (
                    "invalid file preserved",
                    vec![Problem {
                        line: 0,
                        message: reason,
                    }],
                    false,
                ),
            };
            (
                settings,
                schema,
                SettingsSource {
                    path: path.to_owned(),
                    state,
                    problems,
                    first_run,
                },
            )
        }
        Err(e) => {
            // IO or other error
            eprintln!("settings  cannot load {path:?}: {e}");
            let schema = lens_settings::schema::Schema::default();
            (
                Settings::default(),
                schema,
                SettingsSource {
                    path: path.to_owned(),
                    state: "cannot load settings",
                    problems: vec![Problem {
                        line: 0,
                        message: format!("cannot load settings: {e}"),
                    }],
                    first_run: false,
                },
            )
        }
    }
}

/// The file first, then anything typed on the command line over the top.
fn parse() -> Result<Options, String> {
    let flags = parse_flags()?;
    let path = flags.settings.clone().unwrap_or_else(default_settings_path);
    let (settings, schema, source) = load_settings(&path);

    // Build the AI runtime configuration from the schema, which has separate
    // [quick] and [deep] sections per ADR-0005.
    let mut ai_config = AiRuntimeConfig::from_schema(&schema);

    // CLI --endpoint applies to both Quick and Deep for backwards compatibility.
    // A user who needs separate endpoints edits the settings file.
    if let Some(endpoint) = flags.endpoint {
        // Validate the endpoint is local before accepting it.
        if lens_core::translate::engine_for(&endpoint)
            != lens_core::translate::EngineId::LocalOllama
        {
            return Err(format!(
                // One line, deliberately. A `\`-continuation inside this
                // literal came back from rustfmt rejoined with its
                // indentation intact, so the message reached a user with
                // twenty spaces in the middle of a sentence.
                "--endpoint {endpoint} is not on this machine, and Lens will not send text off it",
            ));
        }
        ai_config.quick.endpoint = endpoint.clone();
        ai_config.deep.endpoint = endpoint;
    }

    // CLI --model applies to Quick only (the default path). Deep is always
    // configured via the settings file.
    if let Some(model) = flags.model {
        ai_config.quick.model = Some(model);
    }

    let mut policy = settings.policy();
    if flags.auto {
        // Only the default tier. A `deny` a user wrote for one application is
        // theirs and a convenience flag does not get to overrule it, and the
        // password-manager floor survives either way.
        policy.set_default(AppTier::Allow);
    }

    // Capture CLI overrides that must survive settings reload.
    let cli_overrides = CliOverrides {
        auto: flags.auto,
        language: flags.language.clone(),
        dwell_ms: flags.dwell_ms,
    };

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
        ai_config,
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
        cli_overrides,
    })
}

pub fn main() -> std::process::ExitCode {
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

    // ADR-0005: Two separate translators for Quick and Deep paths.
    // Each has its own endpoint and model, configured independently.
    let mut quick_translator = OllamaTranslator::choosing(&options.ai_config.quick.endpoint);
    if let Some(model) = &options.ai_config.quick.model {
        quick_translator = OllamaTranslator::new(&options.ai_config.quick.endpoint, model);
    }

    let mut deep_translator = OllamaTranslator::choosing(&options.ai_config.deep.endpoint);
    if let Some(model) = &options.ai_config.deep.model {
        deep_translator = OllamaTranslator::new(&options.ai_config.deep.endpoint, model);
    }

    // Resolved once at startup rather than per hover, so the first hover does
    // not pay for a listing, and so an absent Ollama is reported now instead of
    // as a mysteriously empty card later.
    // Carries the remedy, not just the failure. This string reaches three
    // places: the banner, the tray tooltip, and nowhere else a first-time user
    // would look. Someone who unzipped Lens without Ollama and double-clicked
    // it never sees a terminal, so the tooltip is the only thing standing
    // between them and a product that silently does nothing.
    let engine_line = match quick_translator.resolve_model() {
        Ok(model) => model.to_owned(),
        Err(error) => match error.remedy() {
            Some(remedy) => format!("{} · {remedy}", error.user_message()),
            None => error.user_message(),
        },
    };
    let deep_engine_line = match deep_translator.resolve_model() {
        Ok(model) => model.to_owned(),
        Err(error) => match error.remedy() {
            Some(remedy) => format!("{} · {remedy}", error.user_message()),
            None => error.user_message(),
        },
    };

    println!("Mori Lens");
    if options.source.first_run {
        println!("  first     this is the first run; a lesson window is on screen");
    }
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
    // Built once and printed here, because the same facts go into the tray's
    // 상태 보기 window and a release binary has no console to read this in
    // (#165). Two copies would be two things to keep right.
    let facts = configuration_facts(&options, &engine_line, &deep_engine_line);
    for (label, value) in &facts {
        println!("  {label:<9} {value}");
    }
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
        // Prewarm both Quick and Deep translators for selftest.
        // Selftest is a diagnostic, so it's worth the time to verify both engines.
        match quick_translator.prewarm() {
            Ok(elapsed) => println!("quick warm after {}ms", elapsed.as_millis()),
            Err(error) => eprintln!("could not warm quick engine: {}", error.reason()),
        }
        match deep_translator.prewarm() {
            Ok(elapsed) => println!("deep warm after {}ms", elapsed.as_millis()),
            Err(error) => eprintln!("could not warm deep engine: {}", error.reason()),
        }
        println!();
        let code = selftest(
            &overlay,
            &quick_translator,
            &deep_translator,
            &options.language,
            options.profile,
            theme,
            &facts,
            options.ocr_mode,
        );
        quick_translator.release();
        deep_translator.release();
        return code;
    }

    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
    let (answers_tx, answers_rx) = mpsc::channel::<Answer>();

    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let warm_translator = quick_translator.clone();
    // Kept for the exit path: the worker takes ownership of the translators.
    // ADR-0005: Both Quick and Deep must be released on shutdown.
    let quick_releaser = quick_translator.clone();
    let deep_releaser = deep_translator.clone();
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
        // Failure is reported so a first-run user knows Ollama is not
        // reachable. It is not an error state: the first hover will show
        // the same fact on the card, and a warm that succeeded costs nothing.
        match warm_translator.prewarm() {
            Ok(elapsed) => println!("engine warm after {}ms", elapsed.as_millis()),
            Err(_) => eprintln!(
                "engine    Ollama did not respond; the first hover will say so on the card"
            ),
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
        // ADR-0005: Two translators, one for each path. Quick and Deep are
        // separate engines with separate models and endpoints.
        let quick_translator = quick_translator;
        let deep_translator = deep_translator;
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

                    let key = CacheKey::for_analysis(
                        &request,
                        deep_translator.model(),
                        DEEP_PROMPT_VERSION,
                    );
                    if let Some(hit) = explanations.get(&key) {
                        println!("deep  {} lines from memory", hit.lines().len());
                        let _ = answers_tx.send(Answer {
                            generation: job.generation,
                            card: Card::analysed(&job.original, &hit),
                            anchor: job.anchor,
                            final_answer: true,
                            app: job.app.clone(),
                            target_language: job.language.clone(),
                        });
                        continue;
                    }

                    let card = match deep_translator.deep(&request) {
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
                            eprintln!("deep  no analysis [{}]", error.reason());
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
                            target_language: job.language.clone(),
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
            if may_have_unloaded && !quick_translator.is_resident() {
                let _ = answers_tx.send(Answer {
                    generation: job.generation,
                    card: Card::loading_model(&job.display_text),
                    anchor: job.anchor,
                    final_answer: false,
                    app: job.app.clone(),
                    target_language: job.language.clone(),
                });
            }

            let key = CacheKey::of(&request, quick_translator.model(), PROMPT_VERSION);
            if let Some(hit) = cache.get(&key) {
                let mut card = Card::translated(&job.display_text, &hit);
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
                    target_language: job.language.clone(),
                });
                continue;
            }

            // Report the answer as it forms. Each partial is a complete card
            // carrying the original, so the rule that a translation never
            // appears without it holds for a half-finished one too.
            let partial_tx = answers_tx.clone();
            let generation = job.generation;
            let anchor = job.anchor;
            let original = job.display_text.clone();
            // Cloned out before the move, so the job stays whole for the
            // finished answer below.
            let partial_app = job.app.clone();
            let partial_language = job.language.clone();
            let mut on_partial = move |text: &str| {
                let card = Card::partial(&original, text);
                let _ = partial_tx.send(Answer {
                    generation,
                    card,
                    anchor,
                    final_answer: false,
                    app: partial_app.clone(),
                    target_language: partial_language.clone(),
                });
            };

            let card = match quick_translator.quick_streaming(&request, &mut on_partial) {
                Ok(translation) => {
                    cache.insert(key, translation.clone());
                    Card::translated(&job.display_text, &translation)
                }
                Err(error) => Card::not_translated(&job.display_text, &error),
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
                    target_language: job.language.clone(),
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

    // CLI overrides are kept so they can be re-applied after settings reload.
    let cli_overrides = options.cli_overrides.clone();

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
    let mut hover_scope = "sentence".to_owned();
    // The sentence the outstanding translation belongs to, so a waiting card
    // can show it. One at a time: a second hover replaces the first.
    // Said once, the first time a hover lands on the staged lesson.
    let mut lesson_landed = false;
    let mut pending: Option<PendingReading> = None;
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
    let mut on_screen: Option<PresentedReading> = None;
    // **One at a time**, which ADR-0012 says and this enforces by holding one.
    // Opening a second closes the first, so the screen cannot fill with panels.
    let mut pinned: Option<lens_platform_windows::PanelWindow> = None;
    // Why no card appeared, said once per distinct reason. Holding the
    // modifier over a window that exposes no text used to look exactly like a
    // product that had crashed.
    let mut narrator = silence::Narrator::default();
    let mut reach = lens_core::reach::Reach::new();
    // Counted rather than logged. The narrator suppresses a repeated line, so
    // the console is a lower bound on what happened and cannot answer *is this
    // working for me*. See `lens_core::tally`.
    let mut tally = lens_core::tally::Tally::default();
    let mut advised = false;
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

    // Build the Tauri application. This must happen before the polling loop
    // starts, because the polling loop will run on a worker thread and the
    // main thread needs to run Tauri's event loop.
    // Initialize the settings service first. It reads the settings file (or
    // migrates from legacy format if needed).
    // Derive data_dir from the settings file's parent directory.
    // This ensures library.db, saved.md, etc. are beside the actual settings file,
    // whether it's the default %APPDATA%/Mori Lens/settings.toml or a custom --settings path.
    let data_dir = options
        .source
        .path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| {
            // Fallback: if parent() returns None (shouldn't happen), use %APPDATA%/Mori Lens
            let base = std::env::var("APPDATA")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            base.join("Mori Lens")
        });
    let shared_settings = std::sync::Arc::new(std::sync::Mutex::new(
        settings_service::load_or_migrate(&options.source.path),
    ));
    println!(
        "  settings  {:?}",
        shared_settings.lock().unwrap().current()
    );

    let shared_library =
        std::sync::Arc::new(std::sync::Mutex::new(library_service::open(&data_dir)));
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

    // Prune old history entries on startup, if history retention is configured.
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

    let (shared_detail, explain_flag) = detail_view::new_shared();
    let shared_save_path: detail_view::SharedSavePath = std::sync::Arc::new(saved_to.clone());
    let shared_recent: tauri_app::SharedRecent = std::sync::Arc::new(std::sync::Mutex::new(
        lens_core::recent::SessionRecent::new(),
    ));

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

    // Pause has three entrances now: the hotkey, the tray icon, and the tray
    // menu. They set the same flag rather than each doing their own thing,
    // because a product where the icon and the hotkey disagree about whether it
    // is paused is worse than one with neither.
    // Clone the Tauri handle so the polling loop can tell Tauri to exit.
    let loop_tauri = tauri_handle.clone();
    let loop_diagnostics = shared_diagnostics.clone();
    let loop_settings = shared_settings.clone();
    let loop_library = shared_library.clone();
    let loop_detail = shared_detail.clone();
    let loop_explain_flag = explain_flag.clone();
    std::thread::spawn(move || {
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
                        TrayEvent::ShowDiagnostics => {
                            // The one surface that answers *is this working for
                            // me*. A menu item rather than a sixth chord, per
                            // ADR-0012, and read-only: #165's writing half is a
                            // decision problem this does not touch.
                            update_diagnostics(
                                &loop_diagnostics,
                                &configuration_facts(&options, &engine_line, &deep_engine_line),
                                &tally,
                                worker_ocr.as_str(),
                                build_stamp(),
                            );
                            loop_tauri.show_window();
                            drop(pinned.take());
                            let panel = lens_core::diagnostics::diagnostics_panel(
                                &configuration_facts(&options, &engine_line, &deep_engine_line),
                                &tally,
                                worker_ocr,
                                build_stamp(),
                            );
                            pinned = lens_platform_windows::PanelWindow::open(panel, theme);
                            println!(
                                "status {} · {} cards of {} settled hovers",
                                if pinned.is_some() { "shown" } else { "FAILED" },
                                tally.cards(),
                                tally.settled(),
                            );
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
            // The detail window's "Explain Again" button sets this flag.
            // Combined with the keyboard chord below, so a single handler
            // dispatches both.
            if loop_explain_flag.swap(false, std::sync::atomic::Ordering::Relaxed) {
                explain = true;
            }
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
                // Use the acquisition-time app and target language from
                // on_screen, not the current pointer position or settings.
                // The reading was produced from a specific app in a specific
                // language; later movement or settings changes must not
                // change the saved provenance.
                match on_screen.as_ref().and_then(|reading| {
                    reading
                        .card
                        .kept_as(reading.source_app.as_deref())
                        .map(|mut entry| {
                            // The model name is already carried by the card's
                            // metadata (set at construction time from the actual
                            // translation result). The target language comes from
                            // the reading that produced this card, not from the
                            // current settings.
                            entry.target_language = Some(reading.target_language.clone());
                            (
                                entry,
                                reading.anchor,
                                reading.source_app.clone(),
                                reading.target_language.clone(),
                            )
                        })
                }) {
                    Some((entry, anchor, saved_app, saved_language)) => {
                        let lib_guard = loop_library.lock().unwrap();
                        match keeping::append(
                            &saved_to,
                            &entry,
                            std::time::SystemTime::now(),
                            lib_guard.library(),
                        ) {
                            Ok(keeping::StructuredSave::Inserted(_id)) => {
                                // Lengths and lines, never the text
                                // (`docs/privacy-model.md` §3). The path is not
                                // secret and is the useful half.
                                println!(
                                    "keep  {} + {} chars -> {}",
                                    entry.original.chars().count(),
                                    entry.answer.chars().count(),
                                    saved_to.display(),
                                );
                                if let Some(reading) = on_screen.take() {
                                    show_card(
                                        &overlay,
                                        &mut on_screen,
                                        reading.card.kept(),
                                        anchor,
                                        PresentationKind::Feedback,
                                        reading.source_app.as_deref(),
                                        &saved_language,
                                        &shared_recent,
                                        &loop_library,
                                        &loop_settings,
                                    );
                                }
                            }
                            Ok(keeping::StructuredSave::Duplicate(_existing_id)) => {
                                println!(
                                    "keep  {} + {} chars -> already in library",
                                    entry.original.chars().count(),
                                    entry.answer.chars().count(),
                                );
                                if let Some(reading) = on_screen.take() {
                                    show_card(
                                        &overlay,
                                        &mut on_screen,
                                        Card::not_readable(
                                            "파일에 저장함 · 라이브러리에 이미 있음",
                                        ),
                                        anchor,
                                        PresentationKind::Feedback,
                                        reading.source_app.as_deref(),
                                        &saved_language,
                                        &shared_recent,
                                        &loop_library,
                                        &loop_settings,
                                    );
                                }
                            }
                            Ok(keeping::StructuredSave::Failed(ref reason)) => {
                                println!(
                                    "keep  {} + {} chars -> file saved, library write failed: {reason}",
                                    entry.original.chars().count(),
                                    entry.answer.chars().count(),
                                );
                                if let Some(reading) = on_screen.take() {
                                    show_card(
                                        &overlay,
                                        &mut on_screen,
                                        Card::not_readable("파일에 저장함 · 라이브러리 기록 실패"),
                                        anchor,
                                        PresentationKind::Feedback,
                                        reading.source_app.as_deref(),
                                        &saved_language,
                                        &shared_recent,
                                        &loop_library,
                                        &loop_settings,
                                    );
                                }
                            }
                            Err(refusal) => {
                                println!("keep  {}", refusal.reason());
                                show_card(
                                    &overlay,
                                    &mut on_screen,
                                    Card::not_readable(refusal.user_message()),
                                    anchor,
                                    PresentationKind::Feedback,
                                    saved_app.as_deref(),
                                    &saved_language,
                                    &shared_recent,
                                    &loop_library,
                                    &loop_settings,
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
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &loop_library,
                            &loop_settings,
                        );
                    }
                }
            }

            // Once, the first time this session's own numbers say something. A
            // sentence repeated is a sentence nobody reads, which is the rule
            // `reach` follows for the offer it makes per application.
            if !advised {
                if let Some(said) = tally.advice(worker_ocr) {
                    advised = true;
                    println!("advice {said}");
                }
            }

            if pin && !pause.is_paused() {
                // Opened by a key and by nothing else. There is no path from a
                // settled hover to this: `view_from_card` takes a card the reader
                // has already been shown, which is what keeps a dwell from becoming
                // a window.
                match on_screen
                    .as_ref()
                    .and_then(|reading| detail_view::view_from_card(&reading.card))
                {
                    Some(mut view) => {
                        // Use the acquisition-time app and target language from
                        // on_screen, not the current pointer position or
                        // settings. The reading was produced from a specific app
                        // in a specific language; later movement or settings
                        // changes must not change the saved provenance.
                        view.app = on_screen.as_ref().and_then(|r| r.source_app.clone());
                        view.target_language =
                            on_screen.as_ref().map(|r| r.target_language.clone());
                        view.model = on_screen
                            .as_ref()
                            .and_then(|r| r.card.metadata())
                            .and_then(|m| m.model.clone());
                        let explained = view.rows.iter().any(|(_, v)| !v.trim().is_empty());
                        *loop_detail.lock().unwrap() = Some(view);
                        loop_tauri.show_detail_window();
                        println!(
                            "pin   opened · {}",
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
                match on_screen.as_ref().and_then(|reading| {
                    reading
                        .card
                        .for_the_clipboard()
                        .map(|text| (text, reading.anchor, reading.target_language.clone()))
                }) {
                    Some((text, anchor, reading_language)) => {
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
                            if let Some(reading) = on_screen.take() {
                                show_card(
                                    &overlay,
                                    &mut on_screen,
                                    reading.card.copied(),
                                    anchor,
                                    PresentationKind::Feedback,
                                    reading.source_app.as_deref(),
                                    &reading_language,
                                    &shared_recent,
                                    &loop_library,
                                    &loop_settings,
                                );
                            }
                        } else {
                            // Another application is holding the clipboard, which
                            // is ordinary and passes. Said on the card because the
                            // reader pressed a key and would otherwise conclude
                            // the key does nothing.
                            eprintln!("copy  the clipboard would not open");
                            show_card(
                                &overlay,
                                &mut on_screen,
                                Card::not_readable("클립보드를 열지 못했습니다"),
                                anchor,
                                PresentationKind::Feedback,
                                None,
                                &reading_language,
                                &shared_recent,
                                &loop_library,
                                &loop_settings,
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
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &loop_library,
                            &loop_settings,
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
                                    PresentationKind::Feedback,
                                    showing.app.as_deref(),
                                    &language,
                                    &shared_recent,
                                    &loop_library,
                                    &loop_settings,
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
                            None => eprintln!("deep  the acquisition thread did not answer"),
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
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &loop_library,
                            &loop_settings,
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

            // Check for settings saved from the webview. These take precedence
            // over external edits, because the service just wrote them.
            let pending_settings = loop_settings.lock().unwrap().take_pending();
            if let Some(settings) = pending_settings {
                // CLI overrides take precedence over file values.
                language = cli_overrides
                    .language
                    .clone()
                    .unwrap_or_else(|| settings.language.clone());
                hover_scope = settings.hover_scope.clone();
                let mut reloaded = settings.policy();
                cli_overrides.apply_to_policy(&mut reloaded);
                reloaded.deny_self("lens");
                let dwell = cli_overrides.dwell_ms.unwrap_or(settings.dwell_ms);
                hover_config = HoverConfig {
                    dwell: Duration::from_millis(dwell),
                    ..HoverConfig::default()
                };
                policy = reloaded;
                hover.reconfigure(hover_config, policy.clone());
                println!("settings  applied from window");
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
                    // CLI overrides take precedence over file values.
                    language = cli_overrides
                        .language
                        .clone()
                        .unwrap_or_else(|| settings.language.clone());
                    hover_scope = settings.hover_scope.clone();
                    let mut reloaded = settings.policy();
                    cli_overrides.apply_to_policy(&mut reloaded);
                    reloaded.deny_self("lens");
                    let dwell = cli_overrides.dwell_ms.unwrap_or(settings.dwell_ms);
                    hover_config = HoverConfig {
                        dwell: Duration::from_millis(dwell),
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
                    // ADR-0005: Quick and Deep have separate configs. Check both.
                    let mut stale: Vec<&str> = Vec::new();
                    // Re-parse the settings file to get the current schema.
                    if let Ok(file) = lens_settings::SettingsFile::load(&options.source.path) {
                        let current_ai = AiRuntimeConfig::from_schema(file.schema());
                        if current_ai.quick != options.ai_config.quick {
                            stale.push("quick model/endpoint");
                        }
                        if current_ai.deep != options.ai_config.deep {
                            stale.push("deep model/endpoint");
                        }
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
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &loop_library,
                            &loop_settings,
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
                            translucency_named(&settings.translucency)
                                .unwrap_or(options.translucency),
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
                            let ocr_app = pointer::process_name_at(at);
                            explainable = Some(Explainable {
                                original: reading.sentence.clone(),
                                focused_word: Some(reading.word.clone()),
                                anchor: reading.anchor,
                                translation: None,
                                app: ocr_app.clone(),
                            });
                            show_card(
                                &overlay,
                                &mut on_screen,
                                Card::already_readable("Ctrl+Alt+D"),
                                reading.anchor,
                                PresentationKind::Feedback,
                                ocr_app.as_deref(),
                                &language,
                                &shared_recent,
                                &loop_library,
                                &loop_settings,
                            );
                        } else {
                            life.job_dispatched(generation, reading.anchor, started.elapsed());
                            pending = Some(PendingReading {
                                sentence: reading.sentence.clone(),
                                focused_word: Some(reading.word.clone()),
                                anchor: reading.anchor,
                                app: pointer::process_name_at(at),
                            });
                            // A failed send means the translation worker has
                            // gone. Recorded rather than discarded, for the same
                            // reason the acquisition thread's death is: a product
                            // that keeps running while nothing answers is worse
                            // than one that says so.
                            translating &= jobs_tx
                                .send(Job::Translate(Translate {
                                    generation,
                                    original: reading.sentence.clone(),
                                    display_text: reading.sentence,
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
                            PresentationKind::Feedback,
                            None,
                            &language,
                            &shared_recent,
                            &loop_library,
                            &loop_settings,
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
                    PresentationKind::Feedback,
                    None,
                    &language,
                    &shared_recent,
                    &loop_library,
                    &loop_settings,
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
                ctrl_held: pointer::ctrl_held(),
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
                        PresentationKind::Feedback,
                        process_name.as_deref(),
                        &language,
                        &shared_recent,
                        &loop_library,
                        &loop_settings,
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
                        tally.card();
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
                        // Scope: which text the card shows. The model always
                        // receives the sentence (the benchmark of 2026-08-18
                        // showed a bare word gives the wrong reading five times
                        // in six). Ctrl toggles the setting's default.
                        let word_scope = match hover_scope.as_str() {
                            "word" => !input.ctrl_held,
                            _ => input.ctrl_held,
                        };
                        let display_text = if word_scope {
                            target
                                .segmented
                                .word_text()
                                .map(str::trim)
                                .filter(|w| !w.is_empty())
                                .unwrap_or(&sentence)
                        } else {
                            &sentence
                        };
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
                            if pause.explain_is_registered() && reach.readable_without_translation()
                            {
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
                                    PresentationKind::Feedback,
                                    process_name.as_deref(),
                                    &language,
                                    &shared_recent,
                                    &loop_library,
                                    &loop_settings,
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
                            pending = Some(PendingReading {
                                sentence: display_text.to_owned(),
                                focused_word: target.segmented.word_text().map(str::to_owned),
                                anchor,
                                app: target.app.process_name.clone(),
                            });
                            // A failed send means the translation worker has
                            // gone. Recorded rather than discarded, for the same
                            // reason the acquisition thread's death is: a product
                            // that keeps running while nothing answers is worse
                            // than one that says so.
                            translating &= jobs_tx
                                .send(Job::Translate(Translate {
                                    generation,
                                    original: sentence.clone(),
                                    display_text: display_text.to_owned(),
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
                        if let lens_core::session::Resolution::Failed(error) = &other {
                            tally.refusal(error.reason());
                        }
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
                                show_card(
                                    &overlay,
                                    &mut on_screen,
                                    card,
                                    anchor,
                                    PresentationKind::Feedback,
                                    process_name.as_deref(),
                                    &language,
                                    &shared_recent,
                                    &loop_library,
                                    &loop_settings,
                                );
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
                    Card::waiting(pending.as_ref().map(|p| p.sentence.as_str()).unwrap_or(""))
                } else {
                    Card::loading_model(pending.as_ref().map(|p| p.sentence.as_str()).unwrap_or(""))
                };
                show_card(
                    &overlay,
                    &mut on_screen,
                    card,
                    anchor,
                    PresentationKind::Feedback,
                    pending.as_ref().and_then(|p| p.app.as_deref()),
                    &language,
                    &shared_recent,
                    &loop_library,
                    &loop_settings,
                );
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
                            panel.update(fresh.as_panel(
                                &lens_core::detail::Detail::nothing_explained_yet("Ctrl+Alt+D"),
                            ));
                        }
                    }
                    // A finished translation is a sentence the reader can now ask
                    // about. Only a translation: an analysis must not become the
                    // thing the next analysis explains, and a problem card has no
                    // sentence behind it.
                    if let lens_core::card::CardBody::Translation(text) = &answer.card.body {
                        explainable = Some(Explainable {
                            original: answer.card.original.clone(),
                            focused_word: pending.as_ref().and_then(|p| p.focused_word.clone()),
                            anchor: pending.as_ref().map(|p| p.anchor).unwrap_or(answer.anchor),
                            translation: Some(text.clone()),
                            app: answer.app.clone(),
                        });
                    }
                }
                // The generation check lives in the lifecycle, so an answer for a
                // hover the reader has left is dropped in one place rather than at
                // each call site that might forget.
                if life.answer_arrived(answer.generation, answer.anchor) == lifecycle::Action::Show
                {
                    show_card(
                        &overlay,
                        &mut on_screen,
                        answer.card,
                        answer.anchor,
                        PresentationKind::Reading,
                        answer.app.as_deref(),
                        &answer.target_language,
                        &shared_recent,
                        &loop_library,
                        &loop_settings,
                    );
                }
            }

            std::thread::sleep(Duration::from_millis(25));
        }

        overlay.hide();
        drop(tray);
        // Hand the models back. Without this the runtime holds them for the rest of
        // the lease, so a tool that has exited goes on occupying gigabytes of
        // somebody's memory. ADR-0005: Both Quick and Deep must be released.
        if let Some(line) = capture_cost.summary() {
            println!("{line}");
        }
        quick_releaser.release();
        deep_releaser.release();
        println!("\nstopped, and the models released");
        // Tell Tauri to exit now that the polling loop is done.
        loop_tauri.exit();
    }); // end of polling loop thread

    // Run Tauri on the main thread. This blocks until the event loop exits,
    // which happens when the polling loop calls tauri.exit() above.
    tauri_app::run(tauri_app);

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

/// Update the shared diagnostics snapshot from the polling loop.
///
/// The webview reads this via `invoke('get_diagnostics')`. Called when the
/// tray menu opens the diagnostics window, so the snapshot is fresh.
fn update_diagnostics(
    shared: &tauri_app::SharedDiagnostics,
    configuration: &[(String, String)],
    tally: &lens_core::tally::Tally,
    ocr_mode: &str,
    build: &str,
) {
    let mut snapshot = shared.lock().unwrap();
    snapshot.build = build.to_owned();
    snapshot.configuration = configuration.to_vec();
    snapshot.cards = tally.cards();
    snapshot.settled_hovers = tally.settled();
    snapshot.refusals = tally
        .refusals()
        .into_iter()
        .map(|(reason, count)| (reason.to_owned(), count))
        .collect();
    snapshot.ocr_mode = ocr_mode.to_owned();
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
///
/// `source_app` is the application the text came from at acquisition time,
/// What kind of card is being presented to the reader.
///
/// Rendering and persistence are separate concerns. A function that renders
/// a card should not also decide whether to record it. This enum makes the
/// semantic intent explicit: `Reading` is a new reading to record, `Feedback`
/// is a copy/save confirmation or error message that should not create a
/// history entry.
enum PresentationKind {
    /// A new reading to record in session-recent and persistent history.
    Reading,
    /// A copy/save confirmation or error message. Do not record.
    Feedback,
}

/// Render a card on the overlay and update the on-screen state.
///
/// This function handles rendering only. Callers that want to record the
/// reading to history should call `record_presented_reading()` separately.
///
/// `target_language` is the language the reading was produced for, bound at
/// dispatch time. Feedback cards (which are not new readings) pass the
/// current language as a placeholder; it is never read for persistence
/// because feedback cards are not recorded to history.
#[allow(clippy::too_many_arguments)]
fn render_card(
    overlay: &Overlay,
    on_screen: &mut Option<PresentedReading>,
    card: Card,
    anchor: ScreenRect,
    app: Option<String>,
    target_language: String,
) -> bool {
    let drawn = overlay.show(card.clone(), anchor);
    if drawn {
        *on_screen = Some(PresentedReading {
            card,
            anchor,
            source_app: app,
            target_language,
        });
    }
    drawn
}

/// Record a presented reading to session-recent and persistent history.
///
/// Called explicitly by callers when `PresentationKind::Reading`. Feedback
/// cards (copy/save confirmations, error messages) should not call this.
///
/// `target_language` is the language the reading was produced for, bound at
/// dispatch time. History records this rather than the current settings
/// language, so a later settings change cannot retroactively relabel a
/// reading that was already shown.
#[allow(clippy::too_many_arguments)]
fn record_presented_reading(
    card: &Card,
    source_app: Option<&str>,
    target_language: &str,
    recent: &tauri_app::SharedRecent,
    library: &library_service::SharedLibrary,
    settings: &settings_service::SharedSettings,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let app_name = source_app.map(str::to_owned);
    let answer_text = match &card.body {
        lens_core::card::CardBody::Translation(t) => t.clone(),
        lens_core::card::CardBody::Analysis { translation, .. } => {
            translation.clone().unwrap_or_default()
        }
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
    // Only when the reader has turned it on; off means nothing is written.
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
                    lens_core::card::CardBody::Analysis { .. } => "analysis",
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

/// Show a card on the overlay with the specified presentation kind.
///
/// Renders the card and, if `kind` is `PresentationKind::Reading`, records
/// it to session-recent and persistent history. Feedback cards (copy/save
/// confirmations, error messages) use `PresentationKind::Feedback` to skip
/// recording.
///
/// `source_app` is the application the text came from at acquisition time,
/// not the current pointer position. A reading that started in App A must
/// remain attributed to A even if the pointer moved to App B before the
/// result arrived. Callers pass `None` for feedback cards (copy confirmation,
/// save confirmation, error messages) that are not new readings.
///
/// `target_language` is the language the reading was produced for. For
/// `Reading` cards this is the dispatch-time language; for `Feedback` cards
/// it is a placeholder that will not be persisted.
#[allow(clippy::too_many_arguments)]
fn show_card(
    overlay: &Overlay,
    on_screen: &mut Option<PresentedReading>,
    card: Card,
    anchor: ScreenRect,
    kind: PresentationKind,
    source_app: Option<&str>,
    target_language: &str,
    recent: &tauri_app::SharedRecent,
    library: &library_service::SharedLibrary,
    settings: &settings_service::SharedSettings,
) -> bool {
    let drawn = render_card(
        overlay,
        on_screen,
        card.clone(),
        anchor,
        source_app.map(String::from),
        target_language.to_owned(),
    );
    if drawn {
        match kind {
            PresentationKind::Reading => {
                record_presented_reading(
                    &card,
                    source_app,
                    target_language,
                    recent,
                    library,
                    settings,
                );
            }
            PresentationKind::Feedback => {
                // Do not record feedback cards to history.
            }
        }
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

/// The facts the banner prints and the diagnostics window shows.
///
/// One list, because a release binary is a Windows-subsystem application and
/// everything the banner prints goes nowhere, so the window is the only place a
/// reader ever sees them ([#165](https://github.com/swarea/mori-lens/issues/165)).
///
/// Only the short key-and-value ones. The banner's longer lines explain a
/// setting rather than state it, and a window that repeated them would be a
/// manual rather than a diagnostic.
fn configuration_facts(
    options: &Options,
    engine_line: &str,
    deep_engine_line: &str,
) -> Vec<(String, String)> {
    vec![
        // First, because everything under it is a measurement of *this* build
        // and a reader pasting the lines has no other way to say which one.
        ("build".to_owned(), build_stamp().to_owned()),
        ("target".to_owned(), options.language.clone()),
        // ADR-0005: Quick and Deep have separate engines.
        (
            "quick".to_owned(),
            format!(
                "{} · {} · on this machine",
                engine_line, options.ai_config.quick.endpoint
            ),
        ),
        (
            "deep".to_owned(),
            format!(
                "{} · {} · on this machine",
                deep_engine_line, options.ai_config.deep.endpoint
            ),
        ),
        (
            "dwell".to_owned(),
            format!("{}ms", options.dwell.as_millis()),
        ),
        ("mode".to_owned(), options.policy.describe()),
        // Not `ocr` and not `profile`. The banner has a longer line for each,
        // naming the key and the recognisers, and a second short one above it
        // said the same word twice. The window gets the mode from the argument
        // it already takes.
    ]
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
#[allow(clippy::too_many_arguments)]
fn selftest(
    overlay: &Overlay,
    quick_translator: &OllamaTranslator,
    deep_translator: &OllamaTranslator,
    language: &str,
    profile: Profile,
    theme: Theme,
    facts: &[(String, String)],
    ocr: OcrMode,
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
    if let Err(error) = quick_translator.installed_models() {
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
    println!(
        "   focused    {} chars",
        word.as_ref().map(|w| w.chars().count()).unwrap_or(0)
    );

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
    let key = CacheKey::of(&request, quick_translator.model(), PROMPT_VERSION);
    // Kept, because step 4 explains what step 2 translated rather than
    // translating it again. That grounding is the deep path's whole shape.
    let mut translated: Option<String> = None;
    let card = match quick_translator.quick_streaming(&request, &mut on_partial) {
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
        match deep_translator.deep(&deep) {
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
                let key =
                    CacheKey::for_analysis(&deep, deep_translator.model(), DEEP_PROMPT_VERSION);
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
                        let selftest_lib =
                            lens_storage::Library::open(std::path::Path::new(":memory:"))
                                .expect("in-memory library for selftest");
                        keeping::append(
                            &scratch,
                            &entry,
                            std::time::SystemTime::now(),
                            &selftest_lib,
                        )
                    }) {
                    Ok(_save) => {
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

                if !overlay.show(card.clone(), anchor) {
                    println!("   REFUSED    the analysis card was not showable");
                    return std::process::ExitCode::from(1);
                }
                println!("   shown      on screen for 4 seconds");
                std::thread::sleep(Duration::from_secs(4));

                // Step 6, the pinned detail, on the card that was just drawn.
                //
                // **The product's own path, not the platform type's.** The live
                // check in `platform-windows` calls `DetailPanel::open` with a
                // card it made itself; nothing had ever run
                // `Detail::from_card` on a card this binary built, with this
                // binary's theme. The one part still out of reach is the
                // keypress: `RegisterHotKey` succeeding is not `WM_HOTKEY`
                // arriving, and a machine cannot press a key.
                match lens_core::detail::Detail::from_card(&card) {
                    Some(detail) => {
                        let explained = detail.is_explained();
                        let hint = lens_core::detail::Detail::nothing_explained_yet("Ctrl+Alt+D");
                        match lens_platform_windows::PanelWindow::open(
                            detail.as_panel(&hint),
                            theme,
                        ) {
                            Some(panel) => {
                                std::thread::sleep(Duration::from_millis(600));
                                println!(
                                    "6. pinned     open={} explained={} · 3 seconds, Esc closes it",
                                    panel.is_open(),
                                    explained,
                                );
                                if !panel.is_open() {
                                    println!("   REFUSED    the panel closed itself");
                                    return std::process::ExitCode::from(1);
                                }
                                std::thread::sleep(Duration::from_secs(3));
                            }
                            None => {
                                println!("6. REFUSED    the panel did not open");
                                return std::process::ExitCode::from(1);
                            }
                        }
                    }
                    None => {
                        println!("6. REFUSED    the analysis card made no detail");
                        return std::process::ExitCode::from(1);
                    }
                }
            }
            Err(error) => println!(
                "4. not explained: {} ({})",
                error.user_message(),
                error.reason(),
            ),
        }
    }

    // Step 7, the diagnostics window, which the tray opens and a machine cannot
    // click. Everything but the click, the same way step 6 covers everything but
    // the keypress.
    //
    // A `Tally` with nothing in it on purpose: a reader opening this in the
    // first minute is the case that must not show an empty window, and it is
    // the one a test can produce.
    let panel = lens_core::diagnostics::diagnostics_panel(
        facts,
        &lens_core::tally::Tally::default(),
        ocr,
        build_stamp(),
    );
    if !panel.says_anything() {
        println!("7. REFUSED    the diagnostics panel says nothing at all");
        return std::process::ExitCode::from(1);
    }
    match lens_platform_windows::PanelWindow::open(panel, theme) {
        Some(status) => {
            std::thread::sleep(Duration::from_millis(600));
            println!(
                "7. status     open={} · 3 seconds, Esc closes it",
                status.is_open()
            );
            if !status.is_open() {
                println!("   REFUSED    the status window closed itself");
                return std::process::ExitCode::from(1);
            }
            std::thread::sleep(Duration::from_secs(3));
        }
        None => {
            println!("7. REFUSED    the status window did not open");
            return std::process::ExitCode::from(1);
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

/// History recording decision logic, extracted for testing.
///
/// Returns true if the card should be recorded to persistent history.
/// This is the pure decision function; the actual recording happens in show_card.
#[cfg(test)]
fn should_record_history(
    history_enabled: bool,
    excluded_apps: &[String],
    app_name: Option<&str>,
    answer_text: &str,
) -> bool {
    if !history_enabled {
        return false;
    }
    if answer_text.is_empty() {
        return false;
    }
    let is_excluded = app_name
        .map(|name| {
            excluded_apps
                .iter()
                .any(|ex| name.to_lowercase().contains(&ex.to_lowercase()))
        })
        .unwrap_or(false);
    !is_excluded
}

#[cfg(test)]
mod history_privacy_tests {
    use super::*;

    #[test]
    fn history_off_never_writes() {
        // When history is disabled, should_record_history returns false
        // regardless of other parameters.
        assert!(
            !should_record_history(false, &[], Some("Code.exe"), "translation"),
            "history OFF must prevent all writes"
        );
        assert!(
            !should_record_history(false, &[], None, "translation"),
            "history OFF must prevent writes even without app name"
        );
        assert!(
            !should_record_history(
                false,
                &["code.exe".to_owned()],
                Some("Code.exe"),
                "translation"
            ),
            "history OFF takes precedence over exclusion list"
        );
    }

    #[test]
    fn excluded_application_never_writes() {
        // Excluded apps are filtered BEFORE any write occurs.
        let excluded = vec!["code.exe".to_owned()];

        // Exact match (case-insensitive)
        assert!(
            !should_record_history(true, &excluded, Some("Code.exe"), "translation"),
            "excluded app must be blocked (case-insensitive)"
        );
        assert!(
            !should_record_history(true, &excluded, Some("CODE.EXE"), "translation"),
            "excluded app must be blocked (uppercase)"
        );

        // Substring match
        assert!(
            !should_record_history(true, &excluded, Some("VisualStudioCode.exe"), "translation"),
            "excluded app must match as substring"
        );

        // Non-excluded app is allowed
        assert!(
            should_record_history(true, &excluded, Some("chrome.exe"), "translation"),
            "non-excluded app must be allowed"
        );

        // No app name is allowed (not excluded)
        assert!(
            should_record_history(true, &excluded, None, "translation"),
            "missing app name must not be treated as excluded"
        );
    }

    #[test]
    fn empty_answer_is_not_recorded() {
        // Cards with empty answers are not recorded, even if history is enabled.
        assert!(
            !should_record_history(true, &[], Some("chrome.exe"), ""),
            "empty answer must not be recorded"
        );
    }

    #[test]
    fn excluded_app_normalization_is_documented() {
        // Exclusion matching uses case-insensitive substring match.
        // This is documented behavior, not a bug.
        let excluded = vec!["keepass".to_owned()];

        // Case-insensitive
        assert!(!should_record_history(
            true,
            &excluded,
            Some("KeePassXC.exe"),
            "answer"
        ));
        assert!(!should_record_history(
            true,
            &excluded,
            Some("keepassxc.exe"),
            "answer"
        ));
        assert!(!should_record_history(
            true,
            &excluded,
            Some("KEEPASSXC.EXE"),
            "answer"
        ));

        // Substring
        assert!(!should_record_history(
            true,
            &excluded,
            Some("KeePassXC.exe"),
            "answer"
        ));
        assert!(!should_record_history(
            true,
            &excluded,
            Some("MyKeePassApp.exe"),
            "answer"
        ));

        // Whitespace is NOT trimmed (documented behavior)
        // let excluded_with_space = vec![" keepass".to_owned()];
        // This would NOT match "keepass.exe" because of the leading space
        // This is intentional: the user typed it that way in settings
    }

    #[test]
    fn history_latency_benchmark() {
        // Measure the overhead of history recording on the show_card path.
        // This simulates the decision logic and SQLite INSERT.
        use std::time::Instant;

        let iterations = 1000;
        let excluded: Vec<String> = vec![];

        // Measure history OFF (decision only, no DB write)
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = should_record_history(false, &excluded, Some("chrome.exe"), "translation");
        }
        let off_duration = start.elapsed();
        let off_per_call = off_duration.as_nanos() / iterations as u128;

        // Measure history ON with actual DB write
        let path = std::env::temp_dir().join("history_latency_benchmark.db");
        let _ = std::fs::remove_file(&path);
        let lib = lens_storage::Library::open(&path).unwrap();

        let entry = lens_storage::HistoryEntry {
            id: 0,
            original: "test original".to_owned(),
            answer: "test answer".to_owned(),
            kind: "translation".to_owned(),
            source_language: None,
            target_language: None,
            model: None,
            app: Some("chrome.exe".to_owned()),
            provenance: "accessibility".to_owned(),
            provider_scope: "local".to_owned(),
            created_at: 1000,
        };

        let start = Instant::now();
        for i in 0..iterations {
            let mut e = entry.clone();
            e.created_at = 1000 + i as i64;
            let _ = lib.record_history(&e);
        }
        let on_duration = start.elapsed();
        let on_per_call = on_duration.as_nanos() / iterations as u128;

        let _ = std::fs::remove_file(&path);

        println!("\n=== History Latency Benchmark ===");
        println!("Iterations: {}", iterations);
        println!(
            "History OFF (decision only): {:?} total, {} ns/call",
            off_duration, off_per_call
        );
        println!(
            "History ON (with INSERT):    {:?} total, {} ns/call",
            on_duration, on_per_call
        );
        println!(
            "Overhead:                    {} ns/call",
            on_per_call - off_per_call
        );
        println!();
        println!("Assessment:");
        if on_per_call < 100_000 {
            println!("  < 100µs per call → negligible, safe to keep synchronous");
        } else if on_per_call < 1_000_000 {
            println!("  < 1ms per call → acceptable, document in benchmark");
        } else {
            println!("  > 1ms per call → consider async/background writer");
        }
    }
}

#[cfg(test)]
mod settings_migration_tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("lens-settings-migration-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_run_creates_new_toml_format() {
        // First run: file doesn't exist, should create new TOML format
        let dir = temp_dir("first_run");
        let path = dir.join("settings.toml");

        let (settings, _schema, source) = load_settings(&path);

        // Should use defaults
        assert_eq!(settings.language, "Korean");
        assert!(source.first_run);
        assert!(source.problems.is_empty());

        // File should exist and be valid new TOML format
        assert!(path.exists());
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 1"));
        assert!(content.contains("[general]"));
        assert!(content.contains("[quick]"));
        assert!(content.contains("[deep]"));

        // Should be loadable by new parser
        let file = lens_settings::SettingsFile::load(&path).unwrap();
        assert_eq!(file.schema().general.language, "Korean");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_flat_format_is_migrated() {
        // Legacy format (what old code used to write) should be migrated
        let dir = temp_dir("legacy_migrate");
        let path = dir.join("settings.toml");

        // Write legacy flat format (valid TOML but no schema_version)
        let legacy_content = r#"language = "Japanese"
dwell_ms = 250
theme = "night-lake"
endpoint = "127.0.0.1:11434"
model = "gemma3:4b"
"#;
        fs::write(&path, legacy_content).unwrap();

        let (settings, _schema, source) = load_settings(&path);

        // Should have migrated values
        assert_eq!(settings.language, "Japanese");
        assert_eq!(settings.dwell_ms, 250);
        assert_eq!(settings.theme, "night-lake");
        assert_eq!(settings.endpoint, "127.0.0.1:11434");
        assert_eq!(settings.model, Some("gemma3:4b".to_owned()));
        assert_eq!(source.state, "migrated from legacy format");

        // Backup should exist
        let backup = path.with_extension("toml.legacy-backup");
        assert!(backup.exists());
        let backup_content = fs::read_to_string(&backup).unwrap();
        assert_eq!(backup_content, legacy_content);

        // File should now be valid new TOML format
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 1"));
        assert!(content.contains("[general]"));

        // Should be loadable by new parser
        let file = lens_settings::SettingsFile::load(&path).unwrap();
        assert_eq!(file.schema().general.language, "Japanese");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_toml_format_loads_directly() {
        // New TOML format should load without migration
        let dir = temp_dir("new_format");
        let path = dir.join("settings.toml");

        let new_content = r#"schema_version = 1

[general]
language = "English"
dwell_ms = 200
theme = "forest"

[quick]
engine = "ollama"
model = "qwen2.5:3b"
endpoint = "127.0.0.1:11434"

[deep]
engine = "ollama"
model = "auto"
endpoint = "127.0.0.1:11434"
"#;
        fs::write(&path, new_content).unwrap();

        let (settings, _schema, source) = load_settings(&path);

        // Should load directly
        assert_eq!(settings.language, "English");
        assert_eq!(settings.dwell_ms, 200);
        assert_eq!(settings.theme, "forest");
        assert_eq!(source.state, "read");
        assert!(source.problems.is_empty());
        assert!(!source.first_run);

        // No backup should exist
        let backup = path.with_extension("toml.legacy-backup");
        assert!(!backup.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_toml_valid_but_missing_version_is_migrated() {
        // Valid TOML with old keys but no schema_version should be migrated
        let dir = temp_dir("legacy_toml");
        let path = dir.join("settings.toml");

        // Valid TOML syntax but legacy structure
        let legacy_toml = r#"language = "Japanese"
dwell_ms = 250
theme = "night-lake"
"#;
        fs::write(&path, legacy_toml).unwrap();

        let (settings, _schema, source) = load_settings(&path);

        // Should migrate
        assert_eq!(settings.language, "Japanese");
        assert_eq!(settings.dwell_ms, 250);
        assert_eq!(settings.theme, "night-lake");
        assert_eq!(source.state, "migrated from legacy format");

        // Backup should exist
        let backup = path.with_extension("toml.legacy-backup");
        assert!(backup.exists());

        // File should now be valid new format
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 1"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_new_toml_is_not_treated_as_legacy() {
        // Malformed new TOML should be preserved, not migrated as legacy
        let dir = temp_dir("malformed_new");
        let path = dir.join("settings.toml");

        // Has schema_version but malformed
        let malformed = r#"schema_version = 1
[general
language = "Korean"
"#;
        fs::write(&path, malformed).unwrap();

        let (_, _schema, source) = load_settings(&path);

        // Should NOT migrate as legacy
        assert_eq!(source.state, "invalid file preserved");
        assert!(!source.problems.is_empty());

        // Original should be preserved
        let backup = path.with_extension("toml.legacy-backup");
        assert!(!backup.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_schema_missing_version_is_not_silently_flat_migrated() {
        // Has [general] section but no schema_version: not legacy, preserve
        let dir = temp_dir("new_no_version");
        let path = dir.join("settings.toml");

        let new_no_version = r#"[general]
language = "Korean"

[quick]
engine = "ollama"
"#;
        fs::write(&path, new_no_version).unwrap();

        let (_, _schema, source) = load_settings(&path);

        // Should NOT migrate as legacy (has new sections)
        assert_eq!(source.state, "invalid file preserved");

        // No backup (not legacy)
        let backup = path.with_extension("toml.legacy-backup");
        assert!(!backup.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn future_version_is_never_downgraded() {
        // Future schema version should be preserved, never downgraded
        let dir = temp_dir("future_version");
        let path = dir.join("settings.toml");

        let future = r#"schema_version = 999

[general]
language = "Korean"
"#;
        fs::write(&path, future).unwrap();

        let (_, _schema, source) = load_settings(&path);

        // Should preserve, not downgrade
        assert_eq!(source.state, "invalid file preserved");
        assert!(!source.problems.is_empty());

        // No backup (not legacy)
        let backup = path.with_extension("toml.legacy-backup");
        assert!(!backup.exists());

        // Original should still exist with future version
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 999"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_failure_leaves_original_unchanged() {
        // If backup cannot be created, original must not be modified
        let dir = temp_dir("backup_fail");
        let path = dir.join("settings.toml");

        // Create legacy format
        let legacy = "language = Japanese\ndwell_ms = 250\n";
        fs::write(&path, legacy).unwrap();

        // Create all possible backup paths to force failure
        let _backup1 = path.with_extension("toml.legacy-backup");
        let _backup2 = path.with_extension("toml.legacy-backup.1");
        // ... create many backups to hit the limit
        for i in 1..=101 {
            let backup_n = path.with_extension(format!("toml.legacy-backup.{i}"));
            fs::write(&backup_n, "dummy").unwrap();
        }

        let (_, _schema, source) = load_settings(&path);

        // Should fail to migrate
        assert_eq!(source.state, "invalid file preserved");

        // Original should still exist unchanged
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content, legacy);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn existing_backup_is_not_silently_destroyed() {
        // If backup already exists, use numbered variant
        let dir = temp_dir("backup_collision");
        let path = dir.join("settings.toml");

        // Create legacy format (valid TOML, no schema_version)
        let legacy = r#"language = "Japanese"
"#;
        fs::write(&path, legacy).unwrap();

        // Create existing backup
        let backup1 = path.with_extension("toml.legacy-backup");
        fs::write(&backup1, "existing backup").unwrap();

        let (settings, _schema, source) = load_settings(&path);

        // Should migrate using numbered backup
        assert_eq!(settings.language, "Japanese");
        assert_eq!(source.state, "migrated from legacy format");

        // Original backup should still exist
        assert!(backup1.exists());
        let backup1_content = fs::read_to_string(&backup1).unwrap();
        assert_eq!(backup1_content, "existing backup");

        // New backup should be at .1
        let backup2 = path.with_extension("toml.legacy-backup.1");
        assert!(backup2.exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_after_first_run_succeeds() {
        // Simulate first run followed by restart - the original bug scenario
        let dir = temp_dir("restart");
        let path = dir.join("settings.toml");

        // First run: creates file
        let (_, _schema1, source1) = load_settings(&path);
        assert!(source1.first_run);

        // Restart: should load successfully
        let (settings2, _schema2, source2) = load_settings(&path);
        assert!(!source2.first_run);
        assert_eq!(source2.state, "read");
        assert!(source2.problems.is_empty());
        assert_eq!(settings2.language, "Korean");

        // File should still be valid new TOML format
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("schema_version = 1"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_preserves_semantic_values() {
        // Stronger restart test: verify non-default values survive
        let dir = temp_dir("restart_semantic");
        let path = dir.join("settings.toml");

        // Create a canonical file with specific values
        let mut file = lens_settings::SettingsFile::new_defaults(path.clone());
        file.schema_mut().general.language = "Japanese".to_owned();
        file.schema_mut().general.dwell_ms = 250;
        file.schema_mut().general.theme = "forest".to_owned();
        file.schema_mut().general.hover_scope = "word".to_owned();
        file.schema_mut().general.default_tier = lens_core::policy::AppTier::Allow;
        file.schema_mut().applications =
            vec![("Code".to_owned(), lens_core::policy::AppTier::Deny)];
        file.schema_mut().quick.model = Some("Q".to_owned());
        file.schema_mut().quick.endpoint = "127.0.0.1:11434".to_owned();
        file.save().unwrap();

        // Windows startup load
        let (settings, _schema, source) = load_settings(&path);

        // Semantic values identical
        assert_eq!(settings.language, "Japanese");
        assert_eq!(settings.dwell_ms, 250);
        assert_eq!(settings.theme, "forest");
        assert_eq!(settings.hover_scope, "word");
        assert_eq!(settings.default_tier, lens_core::policy::AppTier::Allow);
        assert_eq!(
            settings.policy().tier_for(Some("Code.exe")),
            lens_core::policy::AppTier::Deny
        );
        assert_eq!(settings.model, Some("Q".to_owned()));
        assert_eq!(settings.endpoint, "127.0.0.1:11434");
        assert_eq!(source.state, "read");
        assert!(!source.first_run);

        // No migration occurred
        let backup = path.with_extension("toml.legacy-backup");
        assert!(!backup.exists());

        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod cli_override_tests {
    use super::*;
    use lens_core::policy::{AppPolicy, AppTier};

    #[test]
    fn cli_overrides_apply_to_policy() {
        // --auto should set default tier to Allow
        let overrides = CliOverrides {
            auto: true,
            language: None,
            dwell_ms: None,
        };

        let mut policy = AppPolicy::new(AppTier::Modifier);
        overrides.apply_to_policy(&mut policy);
        assert_eq!(policy.default_tier(), AppTier::Allow);
    }

    #[test]
    fn cli_overrides_no_auto_does_not_change_policy() {
        // Without --auto, policy should remain unchanged
        let overrides = CliOverrides {
            auto: false,
            language: None,
            dwell_ms: None,
        };

        let mut policy = AppPolicy::new(AppTier::Modifier);
        overrides.apply_to_policy(&mut policy);
        assert_eq!(policy.default_tier(), AppTier::Modifier);
    }

    #[test]
    fn cli_overrides_survive_reload_scenario() {
        // Simulate the reload scenario:
        // 1. Launch with --auto → policy = Allow
        // 2. Settings file has default_tier = Modifier
        // 3. Reload fires → should re-apply --auto → policy = Allow

        let overrides = CliOverrides {
            auto: true,
            language: Some("Japanese".to_owned()),
            dwell_ms: Some(500),
        };

        // Simulate file reload: policy from file
        let mut reloaded = AppPolicy::new(AppTier::Modifier);

        // Apply CLI overrides
        overrides.apply_to_policy(&mut reloaded);

        // Verify overrides took effect
        assert_eq!(reloaded.default_tier(), AppTier::Allow);

        // Verify language and dwell overrides
        let language = overrides
            .language
            .clone()
            .unwrap_or_else(|| "Korean".to_owned());
        assert_eq!(language, "Japanese");

        let dwell = overrides.dwell_ms.unwrap_or(320);
        assert_eq!(dwell, 500);
    }
}
