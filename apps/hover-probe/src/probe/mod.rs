//! The probe loop, Windows.

mod pointer;
mod report;

use std::process::ExitCode;
use std::time::{Duration, Instant};

use lens_core::hover::{HoverConfig, HoverEvent};
use lens_core::policy::{AppPolicy, AppTier};
use lens_core::session::{HoverSession, InputState};
use lens_platform_windows::{declare_dpi_awareness, UiaTextAcquirer};

use report::{Outcome, Report};

/// Options, hand-parsed. A CLI argument crate would be one more dependency in
/// a workspace whose whole point is that the interesting risk is elsewhere.
#[derive(Debug)]
struct Options {
    /// Hover automatically everywhere instead of requiring the Alt modifier.
    automatic: bool,
    dwell: Duration,
    interval: Duration,
    /// Print the resolved word. Off by default; see `show_word_banner`.
    show_word: bool,
    /// The word the operator says is under the pointer, for a correctness
    /// judgement that does not require printing anything.
    expect: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            automatic: false,
            dwell: HoverConfig::default().dwell,
            interval: Duration::from_millis(25),
            show_word: false,
            expect: None,
        }
    }
}

const USAGE: &str = "\
hover-probe · Mori Lens Milestone 1 measurement instrument

USAGE
    hover-probe [OPTIONS]

    Rest the pointer on text. One line is printed per resolved hover.
    Press Ctrl+C for the per-application summary.

OPTIONS
    --auto              Hover in every application, not only while Alt is held.
                        The shipping default is the opposite; this exists so a
                        compatibility run does not need a third hand.
    --dwell <ms>        How long the pointer must rest. Default 320.
    --interval <ms>     Pointer sampling period. Default 25.
    --expect <word>     Say what word is under the pointer. The probe reports
                        match or mismatch without printing anything.
    --show-word         Print the resolved word. Never the sentence or the
                        paragraph. Off by default; prints a banner while on.
    -h, --help          This text.
";

fn parse_options() -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--auto" => options.automatic = true,
            "--show-word" => options.show_word = true,
            "--dwell" => {
                options.dwell = Duration::from_millis(parse_millis(args.next(), "--dwell")?);
            }
            "--interval" => {
                options.interval = Duration::from_millis(parse_millis(args.next(), "--interval")?);
            }
            "--expect" => {
                options.expect = Some(args.next().ok_or("--expect needs a word")?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(options)
}

fn parse_millis(value: Option<String>, flag: &str) -> Result<u64, String> {
    value
        .ok_or_else(|| format!("{flag} needs a number of milliseconds"))?
        .parse()
        .map_err(|_| format!("{flag} needs a number of milliseconds"))
}

pub fn run() -> ExitCode {
    let options = match parse_options() {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    // Before anything reads a coordinate. Without it Windows virtualises this
    // process's coordinates and every rectangle is silently wrong above 100
    // percent scaling.
    let dpi_declared = declare_dpi_awareness();

    let acquirer = match UiaTextAcquirer::new() {
        Ok(acquirer) => acquirer,
        Err(error) => {
            eprintln!("the accessibility layer is unreachable: {error}");
            return ExitCode::from(1);
        }
    };

    let mut policy = AppPolicy::new(if options.automatic {
        AppTier::Allow
    } else {
        AppTier::Modifier
    });
    // Without this the probe reads its own console window and reports itself as
    // a supported application.
    policy.deny_self("hover-probe");

    let mut session = HoverSession::new(
        HoverConfig {
            dwell: options.dwell,
            ..HoverConfig::default()
        },
        policy,
        Box::new(acquirer),
    );

    pointer::install_stop_handler();
    print_banner(&options, dpi_declared);

    let started = Instant::now();
    let mut report = Report::default();

    while !pointer::stopping() {
        let Some(position) = pointer::position() else {
            std::thread::sleep(options.interval);
            continue;
        };
        let input = InputState {
            button_down: pointer::any_button_down(),
            // Scroll and keystroke detection need an input hook, which the probe
            // does not install. The state machine's handling of both is covered
            // by unit tests; what is not covered is how often they fire in real
            // use, and that is an M2 question once there is a card to suppress.
            scrolling: false,
            typing: false,
            modifier_held: pointer::modifier_held(),
            ocr_requested: false,
        };

        let process_name = pointer::process_name_at(position);
        let sample = session.sample(position, started.elapsed(), input, process_name.as_deref());

        if let Some(HoverEvent::Settled { generation, point }) = session.observe(sample) {
            let began = Instant::now();
            let resolution = session.resolve(generation, point, started.elapsed(), input);
            let elapsed = began.elapsed();

            let outcome = Outcome::from_resolution(&resolution, point, elapsed);
            report.record(process_name.as_deref(), &outcome);
            println!("{}", outcome.line(&options.expect, options.show_word));
        }

        std::thread::sleep(options.interval);
    }

    println!();
    report.print_summary(started.elapsed());
    ExitCode::SUCCESS
}

fn print_banner(options: &Options, dpi_declared: bool) {
    println!("hover-probe · Mori Lens M1");
    println!(
        "  mode      {}",
        if options.automatic {
            "automatic in every application"
        } else {
            "hold Alt to hover (pass --auto to change)"
        }
    );
    println!("  dwell     {}ms", options.dwell.as_millis());
    println!("  sampling  {}ms", options.interval.as_millis());
    println!(
        "  dpi       {}",
        if dpi_declared {
            "per-monitor aware v2"
        } else {
            "NOT declared: rectangles above 100% scaling are unreliable"
        }
    );
    if options.show_word {
        // Loud, because the default promise is that this tool prints nothing
        // the user was reading (`docs/privacy-model.md` §3).
        println!("  content   PRINTING THE RESOLVED WORD. Not the sentence, not the paragraph.");
    }
    println!("\nRest the pointer on text. Ctrl+C for the summary.\n");
}
