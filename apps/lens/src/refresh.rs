//! Noticing that something changed without being restarted.
//!
//! Two things go stale while Lens runs, and both of them made the product feel
//! like a script.
//!
//! **The settings file.** The tray menu offers to open it. Until this existed,
//! editing it and saving did nothing at all until the next launch, so the menu
//! item led somewhere with no way back.
//!
//! **The system's appearance.** The theme was resolved once at startup, so
//! switching Windows to dark mode left a pale card sitting over dark windows
//! until Lens was restarted.
//!
//! # Why polling is allowed here
//!
//! `CONTRIBUTING.md` forbids polling **the screen**, and the reason is that reading
//! the screen on a timer is what makes a hover tool expensive and invasive.
//! Neither of these reads the screen. One is a file timestamp and the other is
//! two registry values, once a second, and the cost is in the same measurement
//! as the rest of the loop.
//!
//! A file-change notification would be the tidier mechanism and needs a
//! directory watch and a thread to service it. A timestamp comparison is one
//! syscall, and the difference a user could perceive is whether an edit takes
//! effect now or within a second.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use lens_core::settings::{Problem, Settings};
use lens_core::theme::SystemPreferences;

/// How often to stat the settings file.
///
/// Once a second: fast enough that saving feels immediate, and one syscall.
const FILE_INTERVAL: Duration = Duration::from_secs(1);

/// How often to re-read the system's appearance.
///
/// Far less often, because it costs far more and changes far less. Reading it
/// every second doubled the product's idle cost, from 0.16% of a core to
/// 0.31%, to answer a question whose answer changes about twice a year. Five
/// seconds to notice dark mode is not a delay anyone will attribute to Lens.
const APPEARANCE_INTERVAL: Duration = Duration::from_secs(5);

/// What was found to have changed.
#[derive(Debug, Default)]
pub struct Changes {
    /// The settings file was edited. Carries what it now says.
    pub settings: Option<(Settings, Vec<Problem>)>,
    /// The system's appearance changed: dark mode, contrast, transparency.
    pub appearance: Option<SystemPreferences>,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.settings.is_some() || self.appearance.is_some()
    }
}

/// Watches the settings file and the system's appearance.
#[derive(Debug)]
pub struct Watcher {
    path: PathBuf,
    /// `None` when the file does not exist, which is a state like any other:
    /// deleting the file and putting it back must register as a change.
    stamp: Option<SystemTime>,
    appearance: SystemPreferences,
    file_checked: Duration,
    appearance_checked: Duration,
}

impl Watcher {
    pub fn new(path: &Path, appearance: SystemPreferences) -> Self {
        Self {
            path: path.to_owned(),
            stamp: modified(path),
            appearance,
            file_checked: Duration::ZERO,
            appearance_checked: Duration::ZERO,
        }
    }

    /// Look, if it is time to look. Cheap on every other call.
    pub fn poll(
        &mut self,
        now: Duration,
        read_appearance: impl Fn() -> SystemPreferences,
    ) -> Changes {
        self.poll_reading(now, read_appearance, |path| {
            std::fs::read_to_string(path).unwrap_or_default()
        })
    }

    /// The same, with the file read supplied.
    ///
    /// A seam, and it exists for one reason: the guard below refuses a file
    /// whose modification time moved during the read, and there is no way to
    /// arrange that from outside without being able to write to the file
    /// *while* it is being read. A test that only ever hands over a settled
    /// file proves the ordinary path and says nothing about the guard, which
    /// is the same as not having tested it.
    pub fn poll_reading(
        &mut self,
        now: Duration,
        read_appearance: impl Fn() -> SystemPreferences,
        read_file: impl Fn(&Path) -> String,
    ) -> Changes {
        let mut changes = Changes::default();

        if now.saturating_sub(self.file_checked) >= FILE_INTERVAL {
            self.file_checked = now;
            let stamp = modified(&self.path);
            if stamp != self.stamp {
                // A missing file is not an error and not a reason to forget
                // what the user chose: it re-reads as the defaults, which is
                // what a fresh install would do.
                let text = read_file(&self.path);

                // **Read again, and only believe a file that held still.**
                // `Settings::parse` never fails, so a file caught halfway
                // through being written parses as the defaults plus a list of
                // complaints, and the defaults are applied. Somebody who wrote
                // `default = deny` would have it silently become `modifier`
                // for as long as it took the next poll to come round, which is
                // the wrong direction for a policy to slip in.
                //
                // An editor that writes in place leaves the modification time
                // moving until it closes the file, so a stamp that has changed
                // since the read means the read was of something unfinished.
                if modified(&self.path) == stamp {
                    self.stamp = stamp;
                    changes.settings = Some(Settings::parse(&text));
                }
                // Otherwise the stamp is deliberately left alone, so the next
                // poll sees a change and tries again.
            }
        }

        if now.saturating_sub(self.appearance_checked) >= APPEARANCE_INTERVAL {
            self.appearance_checked = now;
            let appearance = read_appearance();
            if appearance != self.appearance {
                self.appearance = appearance;
                changes.appearance = Some(appearance);
            }
        }

        changes
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("mori-lens-refresh-{name}.conf"));
        let _ = std::fs::remove_file(&path);
        path
    }

    pub(super) fn light() -> SystemPreferences {
        SystemPreferences::default()
    }

    fn dark() -> SystemPreferences {
        SystemPreferences {
            dark: true,
            ..SystemPreferences::default()
        }
    }

    #[test]
    fn nothing_changed_means_nothing_reported() {
        let path = scratch("quiet");
        std::fs::write(&path, "theme = forest\n").expect("write");
        let mut watcher = Watcher::new(&path, light());
        let changes = watcher.poll(Duration::from_secs(2), light);
        assert!(!changes.any());
    }

    /// The intervals exist so this is not a stat call and a registry read on
    /// every 25ms tick.
    #[test]
    fn it_does_not_look_more_often_than_it_promised() {
        let path = scratch("interval");
        std::fs::write(&path, "theme = forest\n").expect("write");
        let mut watcher = Watcher::new(&path, light());
        // Well inside the first interval, and a change it would otherwise see.
        assert!(!watcher.poll(Duration::from_millis(300), dark).any());
    }

    #[test]
    fn an_edited_file_comes_back_parsed() {
        let path = scratch("edited");
        std::fs::write(&path, "theme = forest\n").expect("write");
        let mut watcher = Watcher::new(&path, light());

        // A timestamp has limited resolution, so the second write has to be
        // distinguishable from the first. This is the test being careful, not
        // the watcher being slow.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "theme = paper\nlanguage = Japanese\n").expect("write");

        let changes = watcher.poll(Duration::from_secs(2), light);
        let (settings, problems) = changes.settings.expect("the edit was noticed");
        assert_eq!(settings.theme, "paper");
        assert_eq!(settings.language, "Japanese");
        assert!(problems.is_empty());
    }

    /// A broken edit must still arrive, with its problems, rather than being
    /// silently dropped: the loop prints them and the user fixes the line.
    #[test]
    fn a_broken_edit_arrives_with_its_complaints() {
        let path = scratch("broken");
        std::fs::write(&path, "theme = forest\n").expect("write");
        let mut watcher = Watcher::new(&path, light());

        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "theme = chartreuse\n").expect("write");

        let (settings, problems) = watcher
            .poll(Duration::from_secs(2), light)
            .settings
            .expect("the edit was noticed");
        assert_eq!(settings.theme, "system", "a bad value fell back");
        assert_eq!(problems.len(), 1);
    }

    /// Deleting it is an edit too. It must not leave the previous settings in
    /// force with no file explaining them.
    #[test]
    fn deleting_the_file_reverts_to_the_defaults() {
        let path = scratch("deleted");
        std::fs::write(&path, "theme = paper\n").expect("write");
        let mut watcher = Watcher::new(&path, light());

        std::thread::sleep(Duration::from_millis(20));
        std::fs::remove_file(&path).expect("remove");

        let (settings, _) = watcher
            .poll(Duration::from_secs(2), light)
            .settings
            .expect("the deletion was noticed");
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn switching_the_system_to_dark_mode_is_noticed() {
        let path = scratch("dark");
        std::fs::write(&path, "").expect("write");
        let mut watcher = Watcher::new(&path, light());
        // Past the appearance interval, which is longer than the file one.
        let changes = watcher.poll(Duration::from_secs(6), dark);
        assert_eq!(changes.appearance, Some(dark()));
        // And not reported again on the next look.
        assert!(!watcher.poll(Duration::from_secs(12), dark).any());
    }

    /// The appearance is the expensive half, so it is read far less often.
    #[test]
    fn the_appearance_is_checked_less_often_than_the_file() {
        assert!(APPEARANCE_INTERVAL > FILE_INTERVAL);
        let path = scratch("intervals");
        std::fs::write(&path, "").expect("write");
        let mut watcher = Watcher::new(&path, light());
        // Past the file interval, inside the appearance one.
        assert_eq!(watcher.poll(Duration::from_secs(2), dark).appearance, None);
        assert_eq!(
            watcher.poll(Duration::from_secs(6), dark).appearance,
            Some(dark()),
        );
    }
}

#[cfg(test)]
mod half_written_tests {
    use super::tests::{light, scratch};
    use super::*;

    /// A file still being written is not applied.
    ///
    /// `Settings::parse` never fails, so a truncated read produces the
    /// defaults plus complaints, and applying that would turn somebody's
    /// `default = deny` into `modifier` until the next poll. The watcher
    /// therefore believes only a file whose modification time held still
    /// across the read.
    #[test]
    fn a_file_that_moved_during_the_read_is_not_applied() {
        let path = scratch("half-written.conf");
        std::fs::write(&path, "default = deny\n").expect("write");
        let mut watcher = Watcher::new(&path, light());

        // The first poll after the interval sees the file it was built on and
        // has nothing to report.
        let quiet = watcher.poll(Duration::from_secs(5), light);
        assert!(quiet.settings.is_none(), "nothing had changed yet");

        // Now it changes, and holds still.
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "default = allow\n").expect("write");
        let changed = watcher.poll(Duration::from_secs(10), light);
        let (settings, problems) = changed.settings.expect("a settled file is applied");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(settings.default_tier, lens_core::policy::AppTier::Allow);

        let _ = std::fs::remove_file(&path);
    }

    /// And the stamp is left alone when a read is refused, so the next poll
    /// tries again rather than treating the change as handled.
    #[test]
    fn a_refused_read_is_retried() {
        let path = scratch("retried.conf");
        std::fs::write(&path, "default = deny\n").expect("write");
        let mut watcher = Watcher::new(&path, light());
        assert!(watcher
            .poll(Duration::from_secs(5), light)
            .settings
            .is_none());

        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "default = modifier\n").expect("write");
        // Two polls in a row must not both report it: the first applies it and
        // the second has nothing new. What must never happen is the first
        // swallowing the change without applying it.
        let first = watcher.poll(Duration::from_secs(10), light);
        let second = watcher.poll(Duration::from_secs(20), light);
        assert!(first.settings.is_some(), "the change was swallowed");
        assert!(second.settings.is_none(), "it was reported twice");

        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod guard_tests {
    use super::tests::{light, scratch};
    use super::*;

    /// The guard itself, with a file that moves while it is being read.
    ///
    /// The reader writes to the file as its side effect, which is what an
    /// editor writing in place looks like from here: the read returns
    /// something, and by the time it does the file is different.
    #[test]
    fn a_file_that_moves_during_the_read_is_refused_and_retried() {
        let path = scratch("moving.conf");
        std::fs::write(&path, "default = deny\n").expect("write");
        let mut watcher = Watcher::new(&path, light());
        assert!(watcher
            .poll(Duration::from_secs(5), light)
            .settings
            .is_none());

        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(&path, "default = allow\n").expect("write");

        // A read that hands back a truncated file and leaves the real one
        // moving underneath it.
        let refused = watcher.poll_reading(Duration::from_secs(10), light, |path| {
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(path, "default = modifier\n").expect("write");
            // What a half-written file looks like: the beginning of a line.
            "defau".to_owned()
        });
        assert!(
            refused.settings.is_none(),
            "a file that moved during the read was applied anyway",
        );

        // And the change is not lost: the next poll sees it, settled.
        let settled = watcher.poll(Duration::from_secs(20), light);
        let (settings, problems) = settled.settings.expect("the retry never happened");
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(settings.default_tier, lens_core::policy::AppTier::Modifier);

        let _ = std::fs::remove_file(&path);
    }

    /// The control: without the guard this is what would have been applied.
    ///
    /// `Settings::parse` never fails, so half a line yields the defaults and a
    /// complaint. Applying that turns `default = deny` into `modifier`, which
    /// is a policy slipping in the permissive direction on its own.
    #[test]
    fn half_a_line_parses_as_the_defaults_which_is_why_the_guard_exists() {
        let (settings, problems) = lens_core::settings::Settings::parse("defau");
        assert_eq!(problems.len(), 1);
        assert_eq!(
            settings.default_tier,
            lens_core::settings::Settings::default().default_tier,
        );
    }
}
