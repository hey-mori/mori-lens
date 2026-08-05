//! The probe loop, Windows.

mod inventory;
mod ocr_probe;
mod pointer;
mod report;
mod sweep;

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
    hover-probe [OPTIONS]           follow the pointer
    hover-probe sweep [OPTIONS]     measure whole windows, pointer untouched
    hover-probe inventory [--only x] ask each window what text it holds
    hover-probe ocr [OPTIONS]       read the foreground window with OCR

FOLLOW MODE
    Rest the pointer on text. One line is printed per resolved hover.
    Press Ctrl+C for the per-application summary.

    --auto              Hover in every application, not only while Alt is held.
                        The shipping default is the opposite; this exists so a
                        compatibility run does not need a third hand.
    --dwell <ms>        How long the pointer must rest. Default 320.
    --interval <ms>     Pointer sampling period. Default 25.
    --expect <word>     Say what word is under the pointer. The probe reports
                        match or mismatch without printing anything.
    --show-word         Print the resolved word. Never the sentence or the
                        paragraph. Off by default; prints a banner while on.

SWEEP MODE
    Samples a grid across every visible window's client area. The cursor is
    never moved: the accessibility APIs take a coordinate as an argument.
    Reports acquisition rate, on-target rate (the resolved word's rectangle
    contains the sample point), latency, and the wake's before-and-after.

    --only <text>       Only windows whose process or title contains this.
    --points <n>        Roughly how many points per window. Default 240.
    --budget <s>        Seconds allowed per window pass. Default 20.
    --no-wake           Skip the second pass, measuring cold behaviour only.

INVENTORY MODE
    Asks each window's provider what text it holds, by handle rather than by
    coordinate, so a covered window answers too. Reports how many text
    elements exist, how localised they are, and whether a point at an
    element's own centre resolves back into that element.

    Far cheaper than a sweep, and it is the only mode with ground truth: the
    element is known before the question is asked.

    --only <text>       Only windows whose process name contains this.
    --titles            Print each window's title. Off by default: two Edge
                        windows are two results and the matrix has to say
                        which is which, but a title is a document name.

OCR MODE
    Captures the foreground window's client area, recognises it, and samples
    the same grid a sweep uses. Reports how many of those points land on a
    recognised word, and, unless told not to, how many the accessibility path
    resolves over the same points. That pair is the row this milestone exists
    to produce.

    The foreground window, and only that: a capture reads pixels, and the
    pixels inside a covered window belong to whatever is covering it. Put the
    application in front and run this. No window is raised or moved.

    Counts only. It never prints recognised text.

    --points <n>        Grid points. Default 240, matching a sweep.
    --expect <text>     Say what is on screen. Reports found or not found
                        without printing anything.
    --no-compare        Skip the accessibility half.
    --budget <s>        Seconds allowed for the accessibility half. Default 20.

    -h, --help          This text.
";

/// Which of the two instruments to run.
#[derive(Debug)]
enum Mode {
    Follow(Options),
    Inventory {
        only: Option<String>,
        titles: bool,
    },
    Sweep {
        options: sweep::SweepOptions,
        only: Option<String>,
    },
    Ocr(ocr_probe::OcrOptions),
}

fn parse_args() -> Result<Mode, String> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|first| first == "inventory") {
        args.remove(0);
        let mut only = None;
        let mut titles = false;
        let mut rest = args.into_iter();
        while let Some(arg) = rest.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    print!("{USAGE}");
                    std::process::exit(0);
                }
                "--titles" => titles = true,
                "--only" => only = Some(rest.next().ok_or("--only needs some text to match")?),
                other => return Err(format!("unknown argument: {other}")),
            }
        }
        return Ok(Mode::Inventory { only, titles });
    }
    if args.first().is_some_and(|first| first == "ocr") {
        args.remove(0);
        return parse_ocr(args).map(Mode::Ocr);
    }
    if args.first().is_some_and(|first| first == "sweep") {
        args.remove(0);
        return parse_sweep(args).map(|(options, only)| Mode::Sweep { options, only });
    }
    parse_options(args).map(Mode::Follow)
}

fn parse_options(args: Vec<String>) -> Result<Options, String> {
    let mut options = Options::default();
    let mut args = args.into_iter();

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

fn parse_ocr(args: Vec<String>) -> Result<ocr_probe::OcrOptions, String> {
    let mut options = ocr_probe::OcrOptions::default();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--no-compare" => options.compare = false,
            "--expect" => {
                options.expect = Some(args.next().ok_or("--expect needs some text")?);
            }
            "--points" => {
                options.points = args
                    .next()
                    .ok_or("--points needs a number")?
                    .parse()
                    .map_err(|_| "--points needs a number")?;
            }
            "--budget" => {
                let seconds: u64 = args
                    .next()
                    .ok_or("--budget needs a number of seconds")?
                    .parse()
                    .map_err(|_| "--budget needs a number of seconds")?;
                options.budget = Duration::from_secs(seconds);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok(options)
}

fn parse_sweep(args: Vec<String>) -> Result<(sweep::SweepOptions, Option<String>), String> {
    let mut options = sweep::SweepOptions::default();
    let mut only = None;
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "--no-wake" => options.measure_wake = false,
            "--only" => only = Some(args.next().ok_or("--only needs some text to match")?),
            "--points" => {
                options.points_per_window = args
                    .next()
                    .ok_or("--points needs a number")?
                    .parse()
                    .map_err(|_| "--points needs a number")?;
            }
            "--budget" => {
                let seconds: u64 = args
                    .next()
                    .ok_or("--budget needs a number of seconds")?
                    .parse()
                    .map_err(|_| "--budget needs a number of seconds")?;
                options.window_budget = Duration::from_secs(seconds);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    Ok((options, only))
}

/// The commit this binary was built from, baked in by `build.rs`.
///
/// Printed on every run because a stale binary produces measurements that look
/// entirely ordinary: `cargo test` does not rebuild the binary target, so a
/// check-then-run sequence can measure code that is minutes old. Comparing
/// this against `git rev-parse --short HEAD` is the whole defence, and it only
/// works if the number is in front of the operator.
pub fn build_stamp() -> &'static str {
    env!("LENS_BUILD_SHA")
}

fn parse_millis(value: Option<String>, flag: &str) -> Result<u64, String> {
    value
        .ok_or_else(|| format!("{flag} needs a number of milliseconds"))?
        .parse()
        .map_err(|_| format!("{flag} needs a number of milliseconds"))
}

pub fn run() -> ExitCode {
    let mode = match parse_args() {
        Ok(mode) => mode,
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

    if !acquirer.calls_are_bounded() {
        // Loud, because it changes what a latency number means: an unbounded
        // client can record a 45-second acquire and call it a measurement.
        println!(
            "  guard     UNBOUNDED: no per-call limit, so one wedged application can stall a run"
        );
    }

    match mode {
        Mode::Follow(options) => follow(acquirer, options, dpi_declared),
        Mode::Sweep { options, only } => run_sweep(acquirer, options, only, dpi_declared),
        Mode::Ocr(options) => run_ocr(&acquirer, &options, dpi_declared),
        Mode::Inventory { only, titles } => {
            println!(
                "hover-probe inventory · build {}
",
                build_stamp()
            );
            if titles {
                println!(
                    "  titles    PRINTING WINDOW TITLES. Not their content.
"
                );
            }
            let found = inventory::run(only.as_deref());
            inventory::print_report(&found, titles);
            ExitCode::SUCCESS
        }
    }
}

/// Give the operator time to put the right window in front.
///
/// The probe is started from a terminal, which means the terminal is the
/// foreground window at the moment it starts. Without this pause every run
/// would measure the console it was launched from, and it would look like a
/// perfectly ordinary result.
const FOCUS_DELAY: Duration = Duration::from_secs(4);

fn run_ocr(
    acquirer: &UiaTextAcquirer,
    options: &ocr_probe::OcrOptions,
    dpi_declared: bool,
) -> ExitCode {
    println!(
        "hover-probe ocr · build {}
",
        build_stamp()
    );
    if !dpi_declared {
        println!(
            "  dpi       NOT DECLARED: every rectangle below may be wrong on a scaled display"
        );
    }
    println!(
        "  languages {:?}",
        lens_platform_windows::ocr::available_languages(),
    );
    println!(
        "  waiting   {}s · click the window you want measured",
        FOCUS_DELAY.as_secs(),
    );
    std::thread::sleep(FOCUS_DELAY);

    match ocr_probe::run(acquirer, options) {
        Ok(row) => {
            ocr_probe::print(&row);
            ExitCode::SUCCESS
        }
        Err(reason) => {
            println!(
                "
  failed    {reason}"
            );
            ExitCode::from(1)
        }
    }
}

fn run_sweep(
    acquirer: UiaTextAcquirer,
    options: sweep::SweepOptions,
    only: Option<String>,
    dpi_declared: bool,
) -> ExitCode {
    // A sweep reads every window it is allowed to, so the deny list is the
    // only thing standing between it and a password manager. Allow is the
    // default tier here because a sweep is a deliberate, one-shot act rather
    // than something running in the background.
    let mut policy = AppPolicy::new(AppTier::Allow);
    policy.deny_self("hover-probe");

    println!("hover-probe sweep · Mori Lens M1");
    println!("  build     {}", build_stamp());
    println!("  points    ~{} per window", options.points_per_window);
    println!(
        "  budget    {}s per window pass",
        options.window_budget.as_secs()
    );
    println!(
        "  wake      {}",
        if options.measure_wake {
            "measured, cold pass then woken pass"
        } else {
            "not measured, cold pass only"
        }
    );
    println!(
        "  dpi       {}",
        if dpi_declared {
            "per-monitor aware v2"
        } else {
            "NOT declared: rectangles above 100% scaling are unreliable"
        }
    );
    println!("  cursor    not moved at all\n");

    let rows = sweep::run(&acquirer, &policy, options, only.as_deref());
    sweep::print_report(&rows);
    ExitCode::SUCCESS
}

fn follow(acquirer: UiaTextAcquirer, options: Options, dpi_declared: bool) -> ExitCode {
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
    println!("  build     {}", build_stamp());
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
