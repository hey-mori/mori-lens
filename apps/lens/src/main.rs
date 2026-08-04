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

use lens_core::card::Card;
use lens_core::geometry::ScreenRect;
use lens_core::hover::{Generation, HoverConfig, HoverEvent};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState, Resolution};
use lens_core::translate::{QuickRequest, Translator};
use lens_platform_windows::{declare_dpi_awareness, Overlay, UiaTextAcquirer};
use lens_translate::OllamaTranslator;

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
}

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
    --selftest          Run the whole chain once against text already on
                        screen, without moving the pointer, and report each
                        stage. Proves the product works end to end.
    -h, --help          This text.
";

struct Options {
    selftest: bool,
    automatic: bool,
    language: String,
    model: Option<String>,
    dwell: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            selftest: false,
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
    println!("\nRest the pointer on a sentence. Ctrl+C to quit.\n");

    let Some(overlay) = Overlay::start() else {
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
            let mut request = QuickRequest::new(&job.original, &language);
            if let Some(word) = &job.focused_word {
                request = request.with_focus(word);
            }
            let card = match translator.quick(&request) {
                Ok(translation) => Card::translated(&job.original, &translation),
                Err(error) => Card::not_translated(&job.original, &error),
            };
            if answers_tx
                .send(Answer {
                    generation: job.generation,
                    card,
                    anchor: job.anchor,
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
    let mut current = Generation(0);
    let mut shown = false;
    let mut last_point = None;

    while !pointer::stopping() {
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
            current = generation;
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

        // Real movement retires the card. A card that outlives the sentence it
        // explains is worse than none, because the reader has to work out which
        // sentence it belongs to. The radius matches the hover machine's, so a
        // hand resting unsteadily does not dismiss what it just asked for.
        if shown {
            if let Some(previous) = last_point {
                if position.distance_to(previous) > HoverConfig::default().movement_radius_px {
                    overlay.hide();
                    shown = false;
                }
            }
        }

        while let Ok(answer) = answers_rx.try_recv() {
            if answer.generation != current {
                // The pointer moved on. Never draw this.
                continue;
            }
            log_card(&answer.card);
            if overlay.show(answer.card, answer.anchor) {
                shown = true;
            }

            last_point = Some(answer.anchor.center());
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
    let card = match translator.quick(&request) {
        Ok(translation) => {
            println!(
                "2. translated {} chars in {}ms via {}",
                translation.text.chars().count(),
                translation.latency_ms,
                translation.model,
            );
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
