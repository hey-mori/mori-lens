//! macOS product entry point.
//!
//! M6 milestone: the Accessibility acquisition crate is wired in. The overlay,
//! OCR, tray, and hotkeys arrive in M7. For now the binary starts, verifies
//! the AX permission, and reports status.
//!
//! Included from `main.rs` behind `#[cfg(target_os = "macos")]`.

use lens_core::settings::{self, Problem, Settings};
use lens_platform_macos::{AxPermission, MacTextAcquirer};
use lens_translate::OllamaTranslator;

const USAGE: &str = "lens · Mori Lens (macOS)

USAGE
    lens [OPTIONS]

    Rest the pointer on a sentence. A card appears with the translation.
    Ctrl+Alt+D explains the sentence (M7).
    Ctrl+Alt+O reads with OCR where accessibility gives nothing (M7).

OPTIONS
    --settings <path>   Use this settings file instead of ~/Library/Application Support.
    --language <name>   Target language. Default Korean.
    --endpoint <addr>   Where Ollama listens. Default 127.0.0.1:11434.
    -V, --version       Print the version.
    -h, --help          This text.
";

/// Where the settings file lives on macOS when nobody said otherwise.
///
/// `~/Library/Application Support/Mori Lens/settings.conf`, matching the
/// macOS convention for per-user application data.
fn default_settings_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    std::path::PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Mori Lens")
        .join("settings.conf")
}

/// Read the settings file, writing the template the first time.
fn load_settings(path: &std::path::Path) -> (Settings, &'static str, Vec<Problem>, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            let (settings, problems) = Settings::parse(&text);
            (settings, "read", problems, false)
        }
        Err(_) => {
            let wrote = path
                .parent()
                .map(|p| std::fs::create_dir_all(p).is_ok())
                .unwrap_or(false)
                && std::fs::write(path, settings::TEMPLATE).is_ok();
            (
                Settings::default(),
                if wrote {
                    "written with the defaults, edit it and restart"
                } else {
                    "not readable and not writable, so this run is the defaults"
                },
                Vec::new(),
                wrote,
            )
        }
    }
}

fn build_stamp() -> &'static str {
    option_env!("LENS_BUILD_STAMP").unwrap_or("unknown")
}

pub fn main() -> std::process::ExitCode {
    // Parse command-line flags.
    let mut settings_path: Option<std::path::PathBuf> = None;
    let mut language: Option<String> = None;
    let mut endpoint: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return std::process::ExitCode::SUCCESS;
            }
            "--version" | "-V" => {
                println!(
                    "Mori Lens {} · build {} · macOS",
                    env!("CARGO_PKG_VERSION"),
                    build_stamp()
                );
                return std::process::ExitCode::SUCCESS;
            }
            "--settings" => {
                settings_path = args.next().map(std::path::PathBuf::from);
            }
            "--language" => {
                language = args.next();
            }
            "--endpoint" => {
                endpoint = args.next();
            }
            other => {
                eprintln!("unknown argument: {other}\n\n{USAGE}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    // Settings.
    let path = settings_path.unwrap_or_else(default_settings_path);
    let (settings, state, problems, first_run) = load_settings(&path);
    let language = language.unwrap_or(settings.language);
    let endpoint = endpoint.unwrap_or(settings.endpoint);

    println!("Mori Lens · macOS");
    if first_run {
        println!("  first     this is the first run");
    }
    println!("  settings  {} · {}", path.display(), state);
    for problem in &problems {
        println!("            line {}: {}", problem.line, problem.message);
    }
    println!("  language  {language}");
    println!("  endpoint  {endpoint}");

    // AX permission.
    match AxPermission::check_with_prompt(true) {
        AxPermission::Trusted => {
            println!("  ax        trusted");
        }
        AxPermission::NotTrusted => {
            println!("  ax        NOT TRUSTED");
            println!("            open System Settings > Privacy & Security > Accessibility");
            println!("            and enable Mori Lens, then restart.");
            return std::process::ExitCode::from(1);
        }
    }

    // Translator.
    let mut translator = OllamaTranslator::choosing(&endpoint);
    match translator.resolve_model() {
        Ok(model) => println!("  model     {model}"),
        Err(error) => {
            println!(
                "  model     {} · {}",
                error.user_message(),
                error.remedy().unwrap_or_else(|| "install Ollama and pull a model".to_owned())
            );
        }
    }

    // Acquisition: the MacTextAcquirer captures the display topology on
    // construction. We report it via the Debug representation.
    let _acquirer = MacTextAcquirer::new();
    println!("  ax        acquisition ready (M6)");

    println!();
    println!("  macOS acquisition is wired in (M6).");
    println!("  Overlay, OCR, hotkeys, and tray arrive in M7.");
    println!("  The product loop is not yet running on macOS.");

    std::process::ExitCode::SUCCESS
}
