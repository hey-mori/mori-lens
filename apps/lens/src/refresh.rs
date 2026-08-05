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
//! `CLAUDE.md` forbids polling **the screen**, and the reason is that reading
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
        let mut changes = Changes::default();

        if now.saturating_sub(self.file_checked) >= FILE_INTERVAL {
            self.file_checked = now;
            let stamp = modified(&self.path);
            if stamp != self.stamp {
                self.stamp = stamp;
                // A missing file is not an error and not a reason to forget
                // what the user chose: it re-reads as the defaults, which is
                // what a fresh install would do.
                let text = std::fs::read_to_string(&self.path).unwrap_or_default();
                changes.settings = Some(Settings::parse(&text));
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

    fn scratch(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("mori-lens-refresh-{name}.conf"));
        let _ = std::fs::remove_file(&path);
        path
    }

    fn light() -> SystemPreferences {
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
