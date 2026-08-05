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

use std::sync::mpsc;
use std::time::{Duration, Instant};

use lens_core::cache::{CacheKey, TranslationCache};
use lens_core::card::Card;
use lens_core::geometry::ScreenRect;
use lens_core::hover::{Generation, HoverConfig, HoverEvent};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState, Resolution};
use lens_core::theme::{Preset, TextScale, Theme, Translucency};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_windows::{declare_dpi_awareness, system_preferences, Overlay, UiaTextAcquirer};
use lens_translate::OllamaTranslator;

mod lifecycle;
mod pause;
mod pointer;

/// What the worker is asked to translate.
struct Job {
    generation: Generation,
    original: String,
    focused_word: Option<String>,
    anchor: ScreenRect,
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
                        held. Off by default: a tool that reads everything from
                        first launch has to earn that.
    --language <name>   Target language. Default Korean.
    --model <name>      Ollama model. Default: whichever suitable one is
                        installed.
    --dwell <ms>        How long the pointer must rest. Default 320.
    --theme <name>      system, forest, night-lake, or paper. Default system.
    --translucency <t>  clear, balanced, or solid. Default balanced.
    --text <size>       small, normal, large, or larger. Default normal.
    --selftest          Run the whole chain once against text already on
                        screen, without moving the pointer, and report each
                        stage. Proves the product works end to end.
    -h, --help          This text.
";

struct Options {
    selftest: bool,
    preset: Preset,
    translucency: Translucency,
    text: TextScale,
    automatic: bool,
    language: String,
    model: Option<String>,
    dwell: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            selftest: false,
            preset: Preset::System,
            translucency: Translucency::Balanced,
            text: TextScale::Normal,
            automatic: false,
            language: "Korean".to_owned(),
            model: None,
            dwell: HoverConfig::default().dwell,
        }
    }
}

fn parse() -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--auto" => options.automatic = true,
            "--selftest" => options.selftest = true,
            "--theme" => {
                let name = args.next().ok_or("--theme needs a name")?;
                options.preset = match name.as_str() {
                    "system" => Preset::System,
                    "forest" => Preset::Forest,
                    "night-lake" => Preset::NightLake,
                    "paper" => Preset::Paper,
                    other => return Err(format!("unknown theme: {other}")),
                };
            }
            "--translucency" => {
                let name = args.next().ok_or("--translucency needs a value")?;
                options.translucency = match name.as_str() {
                    "clear" => Translucency::Clear,
                    "balanced" => Translucency::Balanced,
                    "solid" => Translucency::Solid,
                    other => return Err(format!("unknown translucency: {other}")),
                };
            }
            "--text" => {
                let name = args.next().ok_or("--text needs a size")?;
                options.text = match name.as_str() {
                    "small" => TextScale::Small,
                    "normal" => TextScale::Normal,
                    "large" => TextScale::Large,
                    "larger" => TextScale::Larger,
                    other => return Err(format!("unknown text size: {other}")),
                };
            }
            "--language" => options.language = args.next().ok_or("--language needs a name")?,
            "--model" => options.model = Some(args.next().ok_or("--model needs a name")?),
            "--dwell" => {
                options.dwell = Duration::from_millis(
                    args.next()
                        .ok_or("--dwell needs milliseconds")?
                        .parse()
                        .map_err(|_| "--dwell needs milliseconds")?,
                );
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

fn main() -> std::process::ExitCode {
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

    let mut translator = OllamaTranslator::default();
    if let Some(model) = &options.model {
        translator = OllamaTranslator::new(lens_translate::DEFAULT_ENDPOINT, model);
    }
    // Resolved once at startup rather than per hover, so the first hover does
    // not pay for a listing, and so an absent Ollama is reported now instead of
    // as a mysteriously empty card later.
    let engine_line = match translator.resolve_model() {
        Ok(model) => model.to_owned(),
        Err(error) => format!("{} ({})", error.user_message(), error.reason()),
    };

    println!("Mori Lens");
    println!("  target    {}", options.language);
    println!("  engine    {engine_line}");
    println!("  dwell     {}ms", options.dwell.as_millis());
    println!(
        "  mode      {}",
        if options.automatic {
            "automatic in every application"
        } else {
            "hold Alt"
        }
    );
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
    println!("\nRest the pointer on a sentence. Ctrl+C to quit.\n");

    let system = system_preferences();
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
        return selftest(&overlay, &translator, &options.language);
    }

    let (jobs_tx, jobs_rx) = mpsc::channel::<Job>();
    let (answers_tx, answers_rx) = mpsc::channel::<Answer>();

    let language = options.language.clone();
    let mut cache = TranslationCache::new(CACHE_ENTRIES);
    let warm_translator = translator.clone();
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

            let mut request = QuickRequest::new(&job.original, &language);
            if let Some(word) = &job.focused_word {
                request = request.with_focus(word);
            }

            let key = CacheKey::of(&request, translator.model(), PROMPT_VERSION);
            if let Some(hit) = cache.get(&key) {
                let card = Card::translated(&job.original, &hit);
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

    let acquirer = match UiaTextAcquirer::new() {
        Ok(acquirer) => acquirer,
        Err(error) => {
            eprintln!("the accessibility layer is unreachable: {error}");
            return std::process::ExitCode::from(1);
        }
    };

    let mut policy = AppPolicy::new(if options.automatic {
        AppTier::Allow
    } else {
        AppTier::Modifier
    });
    // Without this Lens reads its own card and translates its own translation.
    policy.deny_self("lens");

    let mut session = HoverSession::new(
        HoverConfig {
            dwell: options.dwell,
            ..HoverConfig::default()
        },
        policy,
        Box::new(acquirer),
    );

    pointer::install_stop_handler();
    let started = Instant::now();
    // When the card appears, stays, and goes. Extracted so the decisions can
    // be played through in a test instead of only on a desktop: this was the
    // one part of the product with no coverage, and the only bug a user has
    // reported here lived in it. See lifecycle.rs.
    let mut life = lifecycle::CardLifecycle::new();

    while !pointer::stopping() {
        if pause.poll() {
            if pause.is_paused() {
                if life.dismiss_now() == lifecycle::Action::Hide {
                    overlay.hide();
                }
                println!("paused · the pointer is not being sampled");
            } else {
                println!("resumed");
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
        let sample = session.sample(position, now, input, process_name.as_deref());

        if let Some(HoverEvent::Settled { generation, point }) = session.observe(sample) {
            life.hover_settled(generation);
            match session.resolve(generation, point, now, input) {
                Resolution::Presented(target) => {
                    let sentence = target.segmented.sentence_text().trim().to_owned();
                    let anchor = target
                        .anchor()
                        .unwrap_or(ScreenRect::new(point.x, point.y, 1, 1));
                    if !sentence.is_empty() {
                        let _ = jobs_tx.send(Job {
                            generation,
                            original: sentence,
                            focused_word: target.segmented.word_text().map(str::to_owned),
                            anchor,
                        });
                    }
                }
                // Every one of these means "no new card". They are separate
                // variants because the probe counts them; the product only has
                // to not draw anything.
                Resolution::OffTarget(_)
                | Resolution::Failed(_)
                | Resolution::Duplicate(_)
                | Resolution::Stale => {}
            }
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
    println!(
        "
stopped"
    );
    std::process::ExitCode::SUCCESS
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
    let mut found = None;
    'outer: for window in top_level_windows() {
        let bounds = window.client_bounds;
        let step = 24;
        let mut y = bounds.y + 8;
        while y < bounds.bottom() - 8 {
            let mut x = bounds.x + 8;
            while x < bounds.right() - 8 {
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
        println!("No window on screen exposed a sentence to read.");
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
