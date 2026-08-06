//! What the user chose, in a file rather than in flags.
//!
//! Until this existed every launch needed its flags typed again, and the
//! per-application tiers in [`crate::policy`] could be built in code and by
//! nothing else. A privacy control a user cannot reach is not a control, and
//! `--auto --theme forest --text large` typed daily is not a product.
//!
//! # Why the format is hand-written
//!
//! `key = value` lines and one `[applications]` section. No TOML, no JSON, no
//! parser dependency, for the same reason `lens-translate` speaks HTTP by hand:
//! the interesting risk in this project is reading text off a desktop, and
//! every dependency added to reach it is one more thing between a user and a
//! translation. This grammar is small enough to specify in a paragraph and to
//! test exhaustively.
//!
//! It is deliberately **not** called TOML. `theme = system` is not valid TOML,
//! and a file named `.toml` that a TOML parser rejects is worse than an honest
//! name.
//!
//! # Why unknown keys are reported
//!
//! A settings file where a typo does nothing is a settings file the user stops
//! trusting. Every line that is not understood becomes a [`Problem`], and the
//! caller is expected to print them. Parsing never fails: a broken file yields
//! defaults plus a list of what was wrong, because a product that refuses to
//! start over a stray character has chosen the wrong failure.

use crate::policy::{AppPolicy, AppTier};

/// A line the parser did not understand, and where it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// One-based, so it matches what an editor shows.
    pub line: usize,
    pub message: String,
}

/// Everything a user can choose without recompiling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub language: String,
    /// `None` means "whichever suitable model is installed", resolved at
    /// startup rather than pinned here.
    pub model: Option<String>,
    pub dwell_ms: u64,
    pub theme: String,
    pub translucency: String,
    /// Where Ollama is listening.
    ///
    /// Loopback only, and refused otherwise. `docs/privacy-model.md` records
    /// "nothing leaves the machine" as a kept promise, and the reason given is
    /// that no code path to another host exists. This adds one, so it also
    /// adds the check that keeps the promise: a non-loopback address is
    /// reported as a settings error and the default is used.
    ///
    /// The restriction is not squeamishness. The HTTP client here has no TLS,
    /// so sending a reader's screen to another host would send it in clear
    /// text, and the grant mechanism `EngineId::External` describes does not
    /// exist yet.
    pub endpoint: String,
    /// When the OCR fallback may run: `off`, `manual`, or `automatic`.
    ///
    /// `manual` unless asked otherwise, which is what the product has always
    /// done: OCR runs on `Ctrl+Alt+O` and never on its own.
    ///
    /// `automatic` is what an application exposing no text needs to be usable
    /// by hovering rather than by pressing a key. It is a setting rather than
    /// the default because a tool that photographs the screen on its own has to
    /// be turned on by somebody (`docs/privacy-model.md` §4).
    pub ocr: String,
    pub text: String,
    /// The tier for an application with no rule of its own.
    pub default_tier: AppTier,
    /// Process-name substring to tier, in the order they were written.
    pub applications: Vec<(String, AppTier)>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "Korean".to_owned(),
            model: None,
            dwell_ms: 320,
            theme: "system".to_owned(),
            translucency: crate::theme::Translucency::DEFAULT.as_str().to_owned(),
            endpoint: "127.0.0.1:11434".to_owned(),
            ocr: crate::session::OcrMode::Manual.as_str().to_owned(),
            text: "normal".to_owned(),
            // Not `allow`. A tool that reads every window from first launch has
            // to earn that, and the file is where a user grants it
            // (`docs/privacy-model.md` §2).
            default_tier: AppTier::Modifier,
            applications: Vec::new(),
        }
    }
}

impl Settings {
    /// Parse a settings file, keeping defaults for anything absent or broken.
    pub fn parse(text: &str) -> (Self, Vec<Problem>) {
        let mut settings = Self::default();
        let mut problems = Vec::new();
        let mut in_applications = false;

        for (index, raw) in text.lines().enumerate() {
            let line = index + 1;
            let content = strip_comment(raw).trim();
            if content.is_empty() {
                continue;
            }

            if let Some(section) = content.strip_prefix('[') {
                match section.strip_suffix(']').map(str::trim) {
                    Some("applications") => in_applications = true,
                    Some(other) => {
                        in_applications = false;
                        problems.push(Problem {
                            line,
                            message: format!("unknown section [{other}]"),
                        });
                    }
                    None => problems.push(Problem {
                        line,
                        message: "a section needs a closing bracket".to_owned(),
                    }),
                }
                continue;
            }

            let Some((key, value)) = content.split_once('=') else {
                problems.push(Problem {
                    line,
                    message: "expected key = value".to_owned(),
                });
                continue;
            };
            let key = key.trim();
            // Quotes are accepted and stripped, because a user who writes them
            // out of habit should not silently get a language called `"Korean"`.
            let value = unquote(value.trim());

            if value.is_empty() {
                problems.push(Problem {
                    line,
                    message: format!("{key} has no value"),
                });
                continue;
            }

            if in_applications {
                match tier(value) {
                    Some(tier) => settings.applications.push((value_key(key), tier)),
                    None => problems.push(Problem {
                        line,
                        message: format!("{key} must be allow, modifier, or deny, not {value}"),
                    }),
                }
                continue;
            }

            settings.apply(key, value, line, &mut problems);
        }

        (settings, problems)
    }

    fn apply(&mut self, key: &str, value: &str, line: usize, problems: &mut Vec<Problem>) {
        let mut reject = |expected: &str| {
            problems.push(Problem {
                line,
                message: format!("{key} must be {expected}, not {value}"),
            });
        };

        match key {
            "language" => self.language = value.to_owned(),
            "model" => {
                // A deliberate way back to automatic selection, so a user who
                // pinned a model they later removed is not stuck.
                self.model = (value != "auto").then(|| value.to_owned());
            }
            "dwell_ms" => match value.parse::<u64>() {
                // Bounded rather than trusted. Zero means a card on every
                // twitch, and a very large value means a product that appears
                // to do nothing; both read as broken rather than as configured.
                Ok(parsed) if (80..=3000).contains(&parsed) => self.dwell_ms = parsed,
                Ok(_) => reject("between 80 and 3000"),
                Err(_) => reject("a whole number of milliseconds"),
            },
            "theme" => match value {
                "system" | "forest" | "night-lake" | "paper" => self.theme = value.to_owned(),
                _ => reject("system, forest, night-lake, or paper"),
            },
            "ocr" => match crate::session::OcrMode::named(value) {
                Some(mode) => self.ocr = mode.as_str().to_owned(),
                None => reject("off, manual, or automatic"),
            },
            "endpoint" => {
                if crate::translate::engine_for(value) == crate::translate::EngineId::LocalOllama {
                    self.endpoint = value.to_owned();
                } else {
                    reject("an address on this machine: 127.0.0.1, localhost or ::1, with a port");
                }
            }
            "translucency" => match value {
                "clear" | "balanced" | "solid" => self.translucency = value.to_owned(),
                _ => reject("clear, balanced, or solid"),
            },
            "text" => match value {
                "small" | "normal" | "large" | "larger" => self.text = value.to_owned(),
                _ => reject("small, normal, large, or larger"),
            },
            "default" => match tier(value) {
                Some(tier) => self.default_tier = tier,
                None => reject("allow, modifier, or deny"),
            },
            other => problems.push(Problem {
                line,
                message: format!("unknown setting {other}"),
            }),
        }
    }

    /// The policy these settings describe.
    ///
    /// Built on [`AppPolicy::new`], so the built-in password-manager deny list
    /// is present before any rule here is applied. A user can still override
    /// one deliberately, which is their call to make, but they cannot do it by
    /// accident or by omission.
    pub fn policy(&self) -> AppPolicy {
        let mut policy = AppPolicy::new(self.default_tier);
        for (needle, tier) in &self.applications {
            policy.set(needle, *tier);
        }
        policy
    }
}

/// The file written on first run, so there is something to edit.
///
/// Every value is the default, commented out where it is a choice, because a
/// file full of active settings invites a user to change one and wonder why
/// nothing else works. What is uncommented is what a first-time reader needs to
/// see: the two examples of a per-application rule.
///
/// **A constant, so it can disagree with the defaults, so a test says it does
/// not.** It did: this file told a reader the default translucency was
/// `balanced` for as long as it was, and went on saying so after the default
/// changed. A comment that is wrong about the product is worse than no comment,
/// because it is the only description most people will read.
pub const TEMPLATE: &str = "\
# Mori Lens settings.
#
# Edited by hand, read at startup. Command line flags win over this file, so a
# one-off run can override it without editing anything.
#
# Lines are `key = value`. A `#` starts a comment. Anything not understood is
# reported at startup rather than ignored.

# language     = Korean
# model        = auto          # or a name, e.g. qwen2.5:3b
# dwell_ms     = 320           # how long the pointer must rest, 80 to 3000
# theme        = system        # system, forest, night-lake, paper
# translucency = solid         # clear, balanced, solid
# endpoint     = 127.0.0.1:11434  # where Ollama listens. This machine only.
# ocr          = manual        # off, manual, or automatic. Automatic reads
                               # the pixels around the pointer whenever an
                               # application exposes no text, with no keypress.
# text         = normal        # small, normal, large, larger

# What Lens may do in an application it has no rule for.
#   allow      read whenever the pointer rests
#   modifier   only while Alt is held
#   deny       never, not even with Alt
# default = modifier

# Per-application rules. The name is matched against the executable, case
# insensitively, as a substring: `chrome` catches `chrome.exe`.
#
# Password managers are denied before this file is read and do not need listing.
[applications]
# chrome = allow
# msedge = allow
";

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(at) => &line[..at],
        None => line,
    }
}

fn unquote(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[0] == bytes[bytes.len() - 1]
    {
        return &value[1..value.len() - 1];
    }
    value
}

fn value_key(key: &str) -> String {
    unquote(key).to_lowercase()
}

fn tier(value: &str) -> Option<AppTier> {
    match value {
        "allow" => Some(AppTier::Allow),
        "modifier" => Some(AppTier::Modifier),
        "deny" => Some(AppTier::Deny),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults_and_no_complaints() {
        let (settings, problems) = Settings::parse("");
        assert_eq!(settings, Settings::default());
        assert!(problems.is_empty());
    }

    /// The template ships as the first run's file, so it had better parse
    /// cleanly. A commented example that is subtly wrong teaches the wrong
    /// syntax to every user who uncomments it.
    #[test]
    fn the_template_parses_with_nothing_to_report() {
        let (settings, problems) = Settings::parse(TEMPLATE);
        assert_eq!(problems, Vec::new());
        assert_eq!(settings, Settings::default());
    }

    /// And it stays true when the examples are switched on, which is what a
    /// user does first.
    #[test]
    fn the_templates_examples_work_when_uncommented() {
        let uncommented = TEMPLATE
            .replace("# chrome = allow", "chrome = allow")
            .replace("# msedge = allow", "msedge = allow")
            .replace("# default = modifier", "default = allow")
            .replace("# theme        = system", "theme        = forest");
        let (settings, problems) = Settings::parse(&uncommented);
        assert_eq!(problems, Vec::new());
        assert_eq!(settings.theme, "forest");
        assert_eq!(settings.default_tier, AppTier::Allow);
        assert_eq!(
            settings.applications,
            vec![
                ("chrome".to_owned(), AppTier::Allow),
                ("msedge".to_owned(), AppTier::Allow),
            ],
        );
    }

    #[test]
    fn a_typo_is_reported_rather_than_ignored() {
        let (settings, problems) = Settings::parse("thmee = forest\n");
        assert_eq!(settings.theme, "system");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, 1);
        assert!(problems[0].message.contains("thmee"));
    }

    #[test]
    fn a_bad_value_names_what_was_expected_and_what_was_given() {
        let (_, problems) = Settings::parse("theme = purple\n");
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("purple"), "{problems:?}");
        assert!(problems[0].message.contains("forest"), "{problems:?}");
    }

    /// One broken line must not take the rest of the file with it. A user who
    /// mistypes their theme should still get their language.
    #[test]
    fn a_broken_line_does_not_discard_the_good_ones() {
        let (settings, problems) = Settings::parse("theme = purple\nlanguage = Japanese\n");
        assert_eq!(settings.language, "Japanese");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, 1);
    }

    #[test]
    fn the_reported_line_number_is_the_one_an_editor_shows() {
        let (_, problems) = Settings::parse("\n\n# a comment\ntheme = purple\n");
        assert_eq!(problems[0].line, 4);
    }

    #[test]
    fn quotes_are_stripped_rather_than_kept_as_part_of_the_value() {
        let (settings, problems) = Settings::parse("language = \"Japanese\"\n");
        assert_eq!(settings.language, "Japanese");
        assert!(problems.is_empty());
    }

    #[test]
    fn a_dwell_outside_the_usable_range_is_refused_not_clamped_silently() {
        for value in ["0", "5", "60000"] {
            let (settings, problems) = Settings::parse(&format!("dwell_ms = {value}\n"));
            assert_eq!(settings.dwell_ms, 320, "{value} was accepted");
            assert_eq!(problems.len(), 1, "{value} was not reported");
        }
        let (settings, problems) = Settings::parse("dwell_ms = 500\n");
        assert_eq!(settings.dwell_ms, 500);
        assert!(problems.is_empty());
    }

    #[test]
    fn model_auto_means_whichever_is_installed() {
        let (settings, _) = Settings::parse("model = auto\n");
        assert_eq!(settings.model, None);
        let (settings, _) = Settings::parse("model = qwen2.5:3b\n");
        assert_eq!(settings.model.as_deref(), Some("qwen2.5:3b"));
    }

    #[test]
    fn an_application_rule_needs_a_tier_and_says_so() {
        let (settings, problems) = Settings::parse("[applications]\nchrome = yes\n");
        assert!(settings.applications.is_empty());
        assert_eq!(problems.len(), 1);
        assert!(problems[0].message.contains("allow"), "{problems:?}");
    }

    /// The most important property in this file. A user cannot switch off the
    /// password-manager floor by writing a settings file, only by naming one
    /// deliberately.
    #[test]
    fn a_password_manager_is_denied_even_when_the_default_is_allow() {
        let (settings, _) = Settings::parse("default = allow\n");
        let policy = settings.policy();
        assert_eq!(policy.tier_for(Some("chrome")), AppTier::Allow);
        assert_eq!(policy.tier_for(Some("1Password.exe")), AppTier::Deny);
        assert_eq!(policy.tier_for(Some("KeePassXC")), AppTier::Deny);
    }

    #[test]
    fn a_rule_can_still_override_a_built_in_deny_on_purpose() {
        // Their machine, their call. What must not happen is doing it by
        // omission, which the test above covers.
        let (settings, problems) = Settings::parse("[applications]\nkeepass = modifier\n");
        assert!(problems.is_empty());
        assert_eq!(
            settings.policy().tier_for(Some("keepass")),
            AppTier::Modifier,
        );
    }

    #[test]
    fn an_unknown_section_is_reported_and_its_lines_are_not_treated_as_rules() {
        let (settings, problems) = Settings::parse("[colours]\nchrome = allow\n");
        assert!(settings.applications.is_empty());
        // Two: the section, and then `chrome` read as an unknown top-level key.
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].message.contains("colours"));
    }

    #[test]
    fn a_comment_at_the_end_of_a_line_is_not_part_of_the_value() {
        let (settings, problems) = Settings::parse("theme = forest  # the green one\n");
        assert_eq!(settings.theme, "forest");
        assert!(problems.is_empty());
    }

    #[test]
    fn a_key_with_no_value_is_reported_rather_than_blanking_the_setting() {
        let (settings, problems) = Settings::parse("language =\n");
        assert_eq!(settings.language, "Korean");
        assert_eq!(problems.len(), 1);
    }

    #[test]
    fn application_names_are_matched_case_insensitively() {
        let (settings, _) = Settings::parse("[applications]\nCHROME = allow\n");
        assert_eq!(
            settings.policy().tier_for(Some("chrome.exe")),
            AppTier::Allow
        );
    }

    /// Settings that came from a file must be able to say the same things the
    /// flags could, or the file is a downgrade.
    #[test]
    fn a_full_file_round_trips_every_setting() {
        let (settings, problems) = Settings::parse(
            "language = Japanese\n\
             model = gemma3:4b\n\
             dwell_ms = 250\n\
             theme = night-lake\n\
             translucency = clear\n\
             text = large\n\
             default = allow\n\
             [applications]\n\
             code = deny\n",
        );
        assert_eq!(problems, Vec::new());
        assert_eq!(settings.language, "Japanese");
        assert_eq!(settings.model.as_deref(), Some("gemma3:4b"));
        assert_eq!(settings.dwell_ms, 250);
        assert_eq!(settings.theme, "night-lake");
        assert_eq!(settings.translucency, "clear");
        assert_eq!(settings.text, "large");
        assert_eq!(settings.default_tier, AppTier::Allow);
        assert_eq!(settings.policy().tier_for(Some("code")), AppTier::Deny);
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;

    /// Every commented default in the template is the actual default.
    ///
    /// Written as data rather than as five assertions so adding a setting to
    /// the template without adding it here is visible: the list is the
    /// checklist.
    #[test]
    fn the_template_describes_the_defaults_it_claims_to() {
        let defaults = Settings::default();
        let claimed: [(&str, String); 7] = [
            ("language", defaults.language.clone()),
            ("dwell_ms", defaults.dwell_ms.to_string()),
            ("theme", defaults.theme.clone()),
            ("translucency", defaults.translucency.clone()),
            ("endpoint", defaults.endpoint.clone()),
            ("ocr", defaults.ocr.clone()),
            ("text", defaults.text.clone()),
        ];
        for (key, value) in claimed {
            let line = TEMPLATE
                .lines()
                .find(|line| line.starts_with("# ") && line[2..].trim_start().starts_with(key))
                .unwrap_or_else(|| panic!("the template says nothing about {key}"));
            let stated = line
                .split('=')
                .nth(1)
                .unwrap_or_default()
                .split('#')
                .next()
                .unwrap_or_default()
                .trim();
            assert_eq!(
                stated, value,
                "the template says {key} defaults to {stated:?} and it is {value:?}",
            );
        }
    }
}
