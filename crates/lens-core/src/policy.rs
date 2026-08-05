//! Where the pointer is allowed to look.
//!
//! Three tiers per application, and a default that is deliberately not "read
//! everything". A tool with this reach has to earn automatic operation, and it
//! cannot earn it before the compatibility matrix says how well it reads
//! anything (`docs/privacy-model.md` §2).

/// What Lens may do in a given application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTier {
    /// Hover automatically.
    Allow,
    /// Only while a modifier key is held.
    Modifier,
    /// Never, including with the modifier held.
    Deny,
}

/// Matched case-insensitively against the executable stem, as a substring, so
/// `1Password.exe` and `1password-helper` are both caught by `1password`.
#[derive(Debug, Clone)]
struct Rule {
    needle: String,
    tier: AppTier,
}

/// Per-application tiers with a default for everything unlisted.
#[derive(Debug, Clone)]
pub struct AppPolicy {
    default_tier: AppTier,
    rules: Vec<Rule>,
}

/// Applications denied out of the box. Not exhaustive, and not meant to be: a
/// deny list is a floor, and the `Modifier` default is what actually keeps Lens
/// out of somewhere it was never told about.
const DENIED_BY_DEFAULT: [&str; 9] = [
    "1password",
    "keepass",
    "bitwarden",
    "lastpass",
    "dashlane",
    "keeper",
    "enpass",
    "protonpass",
    "credentialui",
];

impl Default for AppPolicy {
    fn default() -> Self {
        Self::new(AppTier::Modifier)
    }
}

impl AppPolicy {
    /// A policy with a default tier and the built-in deny list.
    pub fn new(default_tier: AppTier) -> Self {
        let mut policy = Self {
            default_tier,
            rules: Vec::new(),
        };
        for needle in DENIED_BY_DEFAULT {
            policy.set(needle, AppTier::Deny);
        }
        policy
    }

    /// Change the tier for everything with no rule of its own.
    ///
    /// The `--auto` flag's entire effect. It moves the floor and leaves every
    /// explicit rule alone, so a `deny` a user wrote survives a convenience
    /// flag, and so does the built-in password-manager list.
    pub fn set_default(&mut self, tier: AppTier) -> &mut Self {
        self.default_tier = tier;
        self
    }

    /// One line saying what this policy does, for the startup banner.
    ///
    /// A user who set `default = deny` and sees "hold Alt" would reasonably
    /// conclude the file was ignored, so the banner reports the policy rather
    /// than the flag that was typed.
    pub fn describe(&self) -> String {
        let base = match self.default_tier {
            AppTier::Allow => "automatic in every application",
            AppTier::Modifier => "hold Alt",
            AppTier::Deny => "nothing is read unless an application says otherwise",
        };
        let listed = self.rules.len();
        format!("{base} · {listed} application rules")
    }

    /// Set the tier for a process name substring. A later call for the same
    /// needle replaces the earlier one rather than stacking, so a rule list
    /// cannot end up with two answers for one application.
    pub fn set(&mut self, needle: &str, tier: AppTier) -> &mut Self {
        let needle = needle.to_lowercase();
        match self.rules.iter_mut().find(|rule| rule.needle == needle) {
            Some(rule) => rule.tier = tier,
            None => self.rules.push(Rule { needle, tier }),
        }
        self
    }

    /// Deny Lens's own process.
    ///
    /// Without this the overlay reads its own translation and translates the
    /// translation. It is a correctness rule, not politeness.
    pub fn deny_self(&mut self, process_name: &str) -> &mut Self {
        self.set(process_name, AppTier::Deny)
    }

    pub fn tier_for(&self, process_name: Option<&str>) -> AppTier {
        let Some(name) = process_name else {
            // An unidentifiable window is not granted automatic operation.
            return match self.default_tier {
                AppTier::Allow => AppTier::Modifier,
                other => other,
            };
        };
        let name = name.to_lowercase();
        // Deny wins over any other match, regardless of rule order, so adding
        // an allow rule can never accidentally re-enable a password manager.
        if self
            .rules
            .iter()
            .any(|rule| rule.tier == AppTier::Deny && name.contains(&rule.needle))
        {
            return AppTier::Deny;
        }
        self.rules
            .iter()
            .find(|rule| name.contains(&rule.needle))
            .map(|rule| rule.tier)
            .unwrap_or(self.default_tier)
    }

    /// Whether acquisition may run right now.
    pub fn allows(&self, process_name: Option<&str>, modifier_held: bool) -> bool {
        match self.tier_for(process_name) {
            AppTier::Allow => true,
            AppTier::Modifier => modifier_held,
            AppTier::Deny => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_managers_are_denied_out_of_the_box() {
        let policy = AppPolicy::default();
        for name in ["1Password", "KeePassXC", "bitwarden-desktop"] {
            assert_eq!(
                policy.tier_for(Some(name)),
                AppTier::Deny,
                "{name} was not denied",
            );
            assert!(
                !policy.allows(Some(name), true),
                "{name} allowed by modifier"
            );
        }
    }

    #[test]
    fn the_default_is_modifier_not_automatic() {
        let policy = AppPolicy::default();
        assert_eq!(policy.tier_for(Some("some-editor")), AppTier::Modifier);
        assert!(!policy.allows(Some("some-editor"), false));
        assert!(policy.allows(Some("some-editor"), true));
    }

    #[test]
    fn an_allowed_app_hovers_without_a_modifier() {
        let mut policy = AppPolicy::default();
        policy.set("chrome", AppTier::Allow);
        assert!(policy.allows(Some("chrome"), false));
    }

    #[test]
    fn an_unidentifiable_window_never_gets_automatic_operation() {
        let policy = AppPolicy::new(AppTier::Allow);
        assert_eq!(policy.tier_for(None), AppTier::Modifier);
        assert!(!policy.allows(None, false));
    }

    #[test]
    fn deny_wins_over_an_allow_rule_added_later() {
        let mut policy = AppPolicy::new(AppTier::Allow);
        // A broad allow that happens to match a denied application.
        policy.set("pass", AppTier::Allow);
        assert_eq!(policy.tier_for(Some("1password")), AppTier::Deny);
    }

    #[test]
    fn setting_the_same_rule_twice_replaces_it() {
        let mut policy = AppPolicy::default();
        policy.set("chrome", AppTier::Allow);
        policy.set("chrome", AppTier::Modifier);
        assert_eq!(policy.tier_for(Some("chrome")), AppTier::Modifier);
    }

    #[test]
    fn lens_denies_itself() {
        let mut policy = AppPolicy::new(AppTier::Allow);
        policy.deny_self("hover-probe");
        assert!(!policy.allows(Some("hover-probe"), true));
    }

    #[test]
    fn matching_is_case_insensitive_on_both_sides() {
        let mut policy = AppPolicy::default();
        policy.set("Code", AppTier::Allow);
        assert!(policy.allows(Some("CODE"), false));
    }
}
