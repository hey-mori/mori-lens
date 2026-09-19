//! Configurable global shortcuts.
//!
//! This module defines the product's configurable keyboard shortcuts. Each
//! shortcut binds a [`ShortcutAction`] (what the product does) to a [`Shortcut`]
//! (which keys trigger it). The model is platform-agnostic: platform adapters
//! translate semantic shortcuts into OS-specific registration (Win32
//! `RegisterHotKey`, macOS `CGEventTap`).
//!
//! # Design principles
//!
//! - **Semantic keys, not keycodes**: Persist `L`, `O`, `Space`, not `VK_L` or
//!   macOS keycode `0x25`. Platform adapters translate.
//! - **One source of truth**: The settings file holds the user's configuration.
//!   Runtime registration state may differ (OS conflict), but the saved config
//!   is the user's intent.
//! - **Pause safety**: The Pause action cannot be fully unbound. If a configured
//!   shortcut fails registration, the user must be told.
//! - **Validation before persistence**: Invalid shortcuts (unknown keys,
//!   duplicate chords) are rejected before saving.
//!
//! # Reload lifecycle
//!
//! Shortcut changes are **restart-required** on both platforms. This is safer
//! than hot-re-registration:
//!
//! - On Windows, `RegisterHotKey` can fail if another app holds the chord. If
//!   we unregister the old binding and the new registration fails, we've lost
//!   the user's Pause shortcut. Rollback adds complexity and race conditions.
//! - On macOS, the binding table can be swapped atomically, but for consistency
//!   with Windows and to avoid surprising the user, we use restart-required.
//!
//! The Control Center UI (when implemented) should indicate "restart required"
//! after saving shortcut changes. The new shortcuts take effect on next launch.
//!
//! # Platform defaults
//!
//! Windows and macOS have different default modifiers (Ctrl+Alt vs Cmd+Opt).
//! The [`default_windows_shortcuts`] and [`default_macos_shortcuts`] functions
//! return platform-appropriate defaults. macOS does not support the Pin action
//! (no panel window implementation), so its defaults exclude it.

use std::fmt;
use std::str::FromStr;

/// A product action that can be triggered by a global shortcut.
///
/// Only actions that are currently exposed as product-wide global shortcuts
/// are included. Menu-only or local UI actions are not configurable here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutAction {
    /// Pause or resume pointer sampling. Safety-critical: cannot be unbound.
    Pause,
    /// Manual OCR: read text under the pointer via OCR.
    ManualOcr,
    /// Deep/Explain: invoke the deep explanation path (ADR-0005).
    Deep,
    /// Copy the current card content to clipboard.
    Copy,
    /// Save/Keep the current card content to the library.
    Save,
    /// Pin the current card into a persistent window.
    Pin,
}

impl ShortcutAction {
    /// All configurable actions, in canonical order.
    pub const ALL: [Self; 6] = [
        Self::Pause,
        Self::ManualOcr,
        Self::Deep,
        Self::Copy,
        Self::Save,
        Self::Pin,
    ];

    /// Whether this action can be unbound (set to no shortcut).
    ///
    /// Pause is safety-critical and must always have a working control.
    /// Other actions may be unbound if the user prefers.
    pub const fn can_be_unbound(self) -> bool {
        !matches!(self, Self::Pause)
    }

    /// The canonical string identifier for this action, used in settings.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pause => "pause",
            Self::ManualOcr => "ocr",
            Self::Deep => "deep",
            Self::Copy => "copy",
            Self::Save => "save",
            Self::Pin => "pin",
        }
    }

    /// Parse an action from its string identifier.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pause" => Some(Self::Pause),
            "ocr" => Some(Self::ManualOcr),
            "deep" => Some(Self::Deep),
            "copy" => Some(Self::Copy),
            "save" => Some(Self::Save),
            "pin" => Some(Self::Pin),
            _ => None,
        }
    }
}

impl fmt::Display for ShortcutAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Modifier keys that can be part of a shortcut.
///
/// Uses a bitflags-like representation for efficient combination and checking.
/// The platform adapter translates these into OS-specific modifier masks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Modifiers(u8);

impl Modifiers {
    /// No modifiers.
    pub const NONE: Self = Self(0);
    /// Control key (Ctrl on Windows, Ctrl on macOS).
    pub const CONTROL: Self = Self(1 << 0);
    /// Alt key (Alt on Windows, Option on macOS).
    pub const ALT: Self = Self(1 << 1);
    /// Shift key.
    pub const SHIFT: Self = Self(1 << 2);
    /// Meta key (Win on Windows, Command on macOS).
    pub const META: Self = Self(1 << 3);

    /// Create a modifier set from a raw bitmask.
    pub const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Get the raw bitmask.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Check if a specific modifier is present.
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine two modifier sets.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Check if no modifiers are set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Check if this modifier set has at least one non-Shift system modifier.
    ///
    /// Global shortcuts should require at least one system modifier (Ctrl, Alt,
    /// Meta) to avoid intercepting normal typing. Shift alone is insufficient.
    pub const fn has_system_modifier(self) -> bool {
        self.contains(Self::CONTROL) || self.contains(Self::ALT) || self.contains(Self::META)
    }

    /// Iterate over the modifier names in canonical order.
    pub fn iter_names(self) -> impl Iterator<Item = &'static str> {
        let names = [
            (Self::CONTROL, "Ctrl"),
            (Self::ALT, "Alt"),
            (Self::SHIFT, "Shift"),
            (Self::META, "Meta"),
        ];
        names
            .into_iter()
            .filter(move |(flag, _)| self.contains(*flag))
            .map(|(_, name)| name)
    }
}

impl fmt::Display for Modifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<_> = self.iter_names().collect();
        if names.is_empty() {
            write!(f, "None")
        } else {
            write!(f, "{}", names.join("+"))
        }
    }
}

/// A semantic key that can be part of a shortcut.
///
/// Keys are represented semantically (letter, digit, function key) rather than
/// as platform-specific keycodes. Platform adapters translate to VK codes
/// (Windows) or keycodes (macOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// Letter keys A-Z.
    Letter(char),
    /// Digit keys 0-9.
    Digit(char),
    /// Function keys F1-F12.
    Function(u8),
    /// Space bar.
    Space,
    /// Enter/Return key.
    Enter,
    /// Escape key.
    Escape,
    /// Tab key.
    Tab,
    /// Arrow keys.
    Arrow(ArrowKey),
}

/// Arrow key direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

impl Key {
    /// Parse a key from a string representation.
    ///
    /// Accepts single letters (case-insensitive), single digits, function keys
    /// (F1-F12), and special names (Space, Enter, Escape, Tab, Up, Down, Left,
    /// Right).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }

        // Single character: letter or digit
        if s.len() == 1 {
            let c = s.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                return Some(Self::Letter(c.to_ascii_uppercase()));
            }
            if c.is_ascii_digit() {
                return Some(Self::Digit(c));
            }
            return None;
        }

        // Multi-character: special names
        match s.to_lowercase().as_str() {
            "space" => Some(Self::Space),
            "enter" | "return" => Some(Self::Enter),
            "escape" | "esc" => Some(Self::Escape),
            "tab" => Some(Self::Tab),
            "up" => Some(Self::Arrow(ArrowKey::Up)),
            "down" => Some(Self::Arrow(ArrowKey::Down)),
            "left" => Some(Self::Arrow(ArrowKey::Left)),
            "right" => Some(Self::Arrow(ArrowKey::Right)),
            _ if s.len() >= 2 && s.starts_with('f') || s.starts_with('F') => {
                // Function key F1-F12
                s[1..].parse::<u8>().ok().and_then(|n| {
                    if (1..=12).contains(&n) {
                        Some(Self::Function(n))
                    } else {
                        None
                    }
                })
            }
            _ => None,
        }
    }

    /// Format the key as a string.
    pub fn as_str(&self) -> String {
        match self {
            Self::Letter(c) => c.to_string(),
            Self::Digit(c) => c.to_string(),
            Self::Function(n) => format!("F{n}"),
            Self::Space => "Space".to_owned(),
            Self::Enter => "Enter".to_owned(),
            Self::Escape => "Escape".to_owned(),
            Self::Tab => "Tab".to_owned(),
            Self::Arrow(arrow) => match arrow {
                ArrowKey::Up => "Up".to_owned(),
                ArrowKey::Down => "Down".to_owned(),
                ArrowKey::Left => "Left".to_owned(),
                ArrowKey::Right => "Right".to_owned(),
            },
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_str())
    }
}

/// A keyboard shortcut: modifiers + key.
///
/// The canonical representation of a configurable shortcut. Persisted in
/// settings as a string like "Ctrl+Alt+L" or "Cmd+Opt+O".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// The modifier keys that must be held.
    pub modifiers: Modifiers,
    /// The main key.
    pub key: Key,
}

impl Shortcut {
    /// Create a new shortcut.
    pub const fn new(modifiers: Modifiers, key: Key) -> Self {
        Self { modifiers, key }
    }

    /// Parse a shortcut from a string like "Ctrl+Alt+L".
    ///
    /// Accepts various aliases: Ctrl/Control, Alt/Option/Opt, Shift, Meta/Cmd/Command.
    /// Keys are case-insensitive. Whitespace around `+` is tolerated.
    pub fn parse(s: &str) -> Result<Self, ShortcutParseError> {
        let s = s.trim();
        if s.is_empty() {
            return Err(ShortcutParseError::Empty);
        }

        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        if parts.is_empty() {
            return Err(ShortcutParseError::Empty);
        }

        let mut modifiers = Modifiers::NONE;
        let mut key_part = None;

        for part in &parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers = modifiers.union(Modifiers::CONTROL),
                "alt" | "option" | "opt" => modifiers = modifiers.union(Modifiers::ALT),
                "shift" => modifiers = modifiers.union(Modifiers::SHIFT),
                "meta" | "cmd" | "command" | "win" | "super" => {
                    modifiers = modifiers.union(Modifiers::META)
                }
                _ => {
                    if key_part.is_some() {
                        return Err(ShortcutParseError::MultipleKeys);
                    }
                    key_part = Some(*part);
                }
            }
        }

        let key_str = key_part.ok_or(ShortcutParseError::NoKey)?;
        let key = Key::parse(key_str).ok_or(ShortcutParseError::UnknownKey)?;

        Ok(Self { modifiers, key })
    }

    /// Format the shortcut as a human-readable string.
    ///
    /// Uses platform-neutral terminology: Ctrl, Alt, Shift, Meta.
    pub fn format(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter_names().map(str::to_owned).collect();
        parts.push(self.key.as_str());
        parts.join("+")
    }

    /// Format for Windows display (uses Win key instead of Meta).
    pub fn format_windows(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.contains(Modifiers::CONTROL) {
            parts.push("Ctrl".to_owned());
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Alt".to_owned());
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift".to_owned());
        }
        if self.modifiers.contains(Modifiers::META) {
            parts.push("Win".to_owned());
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }

    /// Format for macOS display (uses Cmd instead of Meta, Option instead of Alt).
    pub fn format_macos(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.contains(Modifiers::CONTROL) {
            parts.push("Ctrl".to_owned());
        }
        if self.modifiers.contains(Modifiers::ALT) {
            parts.push("Option".to_owned());
        }
        if self.modifiers.contains(Modifiers::SHIFT) {
            parts.push("Shift".to_owned());
        }
        if self.modifiers.contains(Modifiers::META) {
            parts.push("Cmd".to_owned());
        }
        parts.push(self.key.as_str());
        parts.join("+")
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.format())
    }
}

impl FromStr for Shortcut {
    type Err = ShortcutParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// Errors that can occur when parsing a shortcut string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutParseError {
    /// The input string is empty.
    Empty,
    /// The string contains multiple keys (only one key is allowed).
    MultipleKeys,
    /// No key was specified (only modifiers).
    NoKey,
    /// The key is not recognized.
    UnknownKey,
}

impl fmt::Display for ShortcutParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "shortcut is empty"),
            Self::MultipleKeys => write!(f, "shortcut has multiple keys"),
            Self::NoKey => write!(f, "shortcut has no key"),
            Self::UnknownKey => write!(f, "unknown key in shortcut"),
        }
    }
}

impl std::error::Error for ShortcutParseError {}

/// Validation errors for shortcut configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutValidationError {
    /// The shortcut does not have enough modifiers to be safe for global use.
    InsufficientModifiers,
    /// Two or more actions are bound to the same shortcut.
    DuplicateBinding {
        action1: ShortcutAction,
        action2: ShortcutAction,
    },
    /// A required action (Pause) is unbound.
    RequiredActionUnbound { action: ShortcutAction },
    /// The action cannot be unbound.
    CannotBeUnbound { action: ShortcutAction },
}

impl fmt::Display for ShortcutValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsufficientModifiers => {
                write!(
                    f,
                    "global shortcuts must have at least one system modifier (Ctrl, Alt, or Meta)"
                )
            }
            Self::DuplicateBinding { action1, action2 } => {
                write!(
                    f,
                    "actions {} and {} are bound to the same shortcut",
                    action1, action2
                )
            }
            Self::RequiredActionUnbound { action } => {
                write!(f, "action {} is required and cannot be unbound", action)
            }
            Self::CannotBeUnbound { action } => {
                write!(f, "action {} cannot be unbound", action)
            }
        }
    }
}

impl std::error::Error for ShortcutValidationError {}

/// A complete shortcut configuration: one optional shortcut per action.
///
/// `None` means the action is unbound (no shortcut). Only actions where
/// [`ShortcutAction::can_be_unbound`] returns true may be `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutConfig {
    /// The shortcut for each action, or `None` if unbound.
    pub bindings: Vec<(ShortcutAction, Option<Shortcut>)>,
}

impl ShortcutConfig {
    /// Create a new configuration from a list of bindings.
    pub fn new(bindings: Vec<(ShortcutAction, Option<Shortcut>)>) -> Self {
        Self { bindings }
    }

    /// Get the shortcut for an action.
    pub fn get(&self, action: ShortcutAction) -> Option<&Option<Shortcut>> {
        self.bindings
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, s)| s)
    }

    /// Set the shortcut for an action.
    pub fn set(&mut self, action: ShortcutAction, shortcut: Option<Shortcut>) {
        if let Some(entry) = self.bindings.iter_mut().find(|(a, _)| *a == action) {
            entry.1 = shortcut;
        }
    }

    /// Validate the configuration.
    ///
    /// Checks:
    /// - No duplicate bindings (two actions with the same shortcut)
    /// - Required actions (Pause) are not unbound
    /// - All bound shortcuts have sufficient modifiers
    pub fn validate(&self) -> Result<(), Vec<ShortcutValidationError>> {
        let mut errors = Vec::new();

        // Check for duplicate bindings
        let mut seen: Vec<(Shortcut, ShortcutAction)> = Vec::new();
        for (action, shortcut) in &self.bindings {
            if let Some(sc) = shortcut {
                if let Some((_, prev_action)) = seen.iter().find(|(s, _)| s == sc) {
                    errors.push(ShortcutValidationError::DuplicateBinding {
                        action1: *prev_action,
                        action2: *action,
                    });
                } else {
                    seen.push((*sc, *action));
                }
            }
        }

        // Check required actions and modifier sufficiency
        for (action, shortcut) in &self.bindings {
            if shortcut.is_none() && !action.can_be_unbound() {
                errors.push(ShortcutValidationError::RequiredActionUnbound { action: *action });
            }
            if let Some(sc) = shortcut {
                if !sc.modifiers.has_system_modifier() {
                    errors.push(ShortcutValidationError::InsufficientModifiers);
                    break; // Only report once
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// Get the default shortcuts for Windows.
pub fn default_windows_shortcuts() -> ShortcutConfig {
    ShortcutConfig::new(vec![
        (
            ShortcutAction::Pause,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('L'),
            )),
        ),
        (
            ShortcutAction::ManualOcr,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('O'),
            )),
        ),
        (
            ShortcutAction::Deep,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('D'),
            )),
        ),
        (
            ShortcutAction::Copy,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('C'),
            )),
        ),
        (
            ShortcutAction::Save,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('S'),
            )),
        ),
        (
            ShortcutAction::Pin,
            Some(Shortcut::new(
                Modifiers::CONTROL.union(Modifiers::ALT),
                Key::Letter('P'),
            )),
        ),
    ])
}

/// Get the default shortcuts for macOS.
///
/// Note: macOS does not currently support the Pin action (no panel window
/// implementation). Only the five actions that have macOS product support
/// are included.
pub fn default_macos_shortcuts() -> ShortcutConfig {
    ShortcutConfig::new(vec![
        (
            ShortcutAction::Pause,
            Some(Shortcut::new(
                Modifiers::META.union(Modifiers::ALT),
                Key::Letter('L'),
            )),
        ),
        (
            ShortcutAction::ManualOcr,
            Some(Shortcut::new(
                Modifiers::META.union(Modifiers::ALT),
                Key::Letter('O'),
            )),
        ),
        (
            ShortcutAction::Deep,
            Some(Shortcut::new(
                Modifiers::META.union(Modifiers::ALT),
                Key::Letter('D'),
            )),
        ),
        (
            ShortcutAction::Copy,
            Some(Shortcut::new(
                Modifiers::META.union(Modifiers::ALT),
                Key::Letter('C'),
            )),
        ),
        (
            ShortcutAction::Save,
            Some(Shortcut::new(
                Modifiers::META.union(Modifiers::ALT),
                Key::Letter('S'),
            )),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_shortcut() {
        let sc = Shortcut::parse("Ctrl+Alt+L").unwrap();
        assert!(sc.modifiers.contains(Modifiers::CONTROL));
        assert!(sc.modifiers.contains(Modifiers::ALT));
        assert_eq!(sc.key, Key::Letter('L'));
    }

    #[test]
    fn parse_case_insensitive() {
        let sc = Shortcut::parse("ctrl+alt+l").unwrap();
        assert_eq!(sc.key, Key::Letter('L'));
    }

    #[test]
    fn parse_with_whitespace() {
        let sc = Shortcut::parse("Ctrl + Alt + L").unwrap();
        assert_eq!(sc.key, Key::Letter('L'));
    }

    #[test]
    fn parse_aliases() {
        let sc1 = Shortcut::parse("Control+Option+O").unwrap();
        assert!(sc1.modifiers.contains(Modifiers::CONTROL));
        assert!(sc1.modifiers.contains(Modifiers::ALT));

        let sc2 = Shortcut::parse("Cmd+Opt+D").unwrap();
        assert!(sc2.modifiers.contains(Modifiers::META));
        assert!(sc2.modifiers.contains(Modifiers::ALT));
    }

    #[test]
    fn parse_function_key() {
        let sc = Shortcut::parse("Ctrl+F1").unwrap();
        assert_eq!(sc.key, Key::Function(1));
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(Shortcut::parse("Ctrl+Space").unwrap().key, Key::Space);
        assert_eq!(Shortcut::parse("Ctrl+Enter").unwrap().key, Key::Enter);
        assert_eq!(Shortcut::parse("Ctrl+Esc").unwrap().key, Key::Escape);
    }

    #[test]
    fn parse_errors() {
        assert_eq!(Shortcut::parse(""), Err(ShortcutParseError::Empty));
        assert_eq!(Shortcut::parse("Ctrl+Alt"), Err(ShortcutParseError::NoKey));
        assert_eq!(
            Shortcut::parse("Ctrl+Alt+L+O"),
            Err(ShortcutParseError::MultipleKeys)
        );
        assert_eq!(
            Shortcut::parse("Ctrl+Alt+Unknown"),
            Err(ShortcutParseError::UnknownKey)
        );
    }

    #[test]
    fn format_roundtrip() {
        let sc = Shortcut::new(Modifiers::CONTROL.union(Modifiers::ALT), Key::Letter('L'));
        let formatted = sc.format();
        let parsed = Shortcut::parse(&formatted).unwrap();
        assert_eq!(sc, parsed);
    }

    #[test]
    fn validate_no_duplicates() {
        let config = ShortcutConfig::new(vec![
            (
                ShortcutAction::Pause,
                Some(Shortcut::parse("Ctrl+Alt+L").unwrap()),
            ),
            (
                ShortcutAction::ManualOcr,
                Some(Shortcut::parse("Ctrl+Alt+L").unwrap()),
            ),
        ]);
        let errors = config.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ShortcutValidationError::DuplicateBinding { .. })));
    }

    #[test]
    fn validate_pause_required() {
        let config = ShortcutConfig::new(vec![(ShortcutAction::Pause, None)]);
        let errors = config.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ShortcutValidationError::RequiredActionUnbound { .. })));
    }

    #[test]
    fn validate_insufficient_modifiers() {
        let config = ShortcutConfig::new(vec![(
            ShortcutAction::Pause,
            Some(Shortcut::new(Modifiers::NONE, Key::Letter('L'))),
        )]);
        let errors = config.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, ShortcutValidationError::InsufficientModifiers)));
    }

    #[test]
    fn validate_valid_config() {
        let config = default_windows_shortcuts();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn default_windows_shortcuts_are_valid() {
        let config = default_windows_shortcuts();
        assert!(config.validate().is_ok());
        // All defaults use Ctrl+Alt
        for (_, shortcut) in &config.bindings {
            if let Some(sc) = shortcut {
                assert!(sc.modifiers.contains(Modifiers::CONTROL));
                assert!(sc.modifiers.contains(Modifiers::ALT));
            }
        }
    }

    #[test]
    fn default_macos_shortcuts_are_valid() {
        let config = default_macos_shortcuts();
        assert!(config.validate().is_ok());
        // All defaults use Cmd+Opt (META+ALT)
        for (_, shortcut) in &config.bindings {
            if let Some(sc) = shortcut {
                assert!(sc.modifiers.contains(Modifiers::META));
                assert!(sc.modifiers.contains(Modifiers::ALT));
            }
        }
    }

    #[test]
    fn can_be_unbound_policy() {
        assert!(!ShortcutAction::Pause.can_be_unbound());
        assert!(ShortcutAction::ManualOcr.can_be_unbound());
        assert!(ShortcutAction::Deep.can_be_unbound());
        assert!(ShortcutAction::Copy.can_be_unbound());
        assert!(ShortcutAction::Save.can_be_unbound());
        assert!(ShortcutAction::Pin.can_be_unbound());
    }

    #[test]
    fn default_windows_shortcuts_have_no_internal_duplicates() {
        let config = default_windows_shortcuts();
        let mut seen = std::collections::HashSet::new();
        for (_, shortcut) in &config.bindings {
            if let Some(sc) = shortcut {
                assert!(
                    seen.insert(*sc),
                    "duplicate shortcut in Windows defaults: {sc}"
                );
            }
        }
        assert_eq!(
            seen.len(),
            6,
            "Windows should have 6 unique default shortcuts"
        );
    }

    #[test]
    fn default_macos_shortcuts_have_no_internal_duplicates() {
        let config = default_macos_shortcuts();
        let mut seen = std::collections::HashSet::new();
        for (_, shortcut) in &config.bindings {
            if let Some(sc) = shortcut {
                assert!(
                    seen.insert(*sc),
                    "duplicate shortcut in macOS defaults: {sc}"
                );
            }
        }
        // macOS has 5 actions (no Pin support)
        assert_eq!(
            seen.len(),
            5,
            "macOS should have 5 unique default shortcuts"
        );
    }
}
