//! What a card looks like, as data.
//!
//! No platform types, so the Windows and macOS overlays render the same
//! decisions and the rules about readability are testable without a screen
//! (`docs/decisions/ADR-0011-glass-needs-direct2d.md`).
//!
//! # Why presets rather than settings
//!
//! A blur radius, a shadow density, a border opacity and a tint strength are
//! four knobs whose good combinations are a small fraction of their product. A
//! user handed all four assembles something worse than any of the four
//! finished looks below, and then owns the result. So: four presets, one
//! translucency choice, one text size choice, and nothing else.

/// Straight sRGB, no alpha. Alpha lives on the surface, not the ink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Relative luminance, per WCAG 2.x.
    pub fn luminance(self) -> f64 {
        fn channel(value: u8) -> f64 {
            let v = f64::from(value) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * channel(self.r) + 0.7152 * channel(self.g) + 0.0722 * channel(self.b)
    }

    /// WCAG contrast ratio against another colour, 1.0 to 21.0.
    pub fn contrast_with(self, other: Self) -> f64 {
        let (a, b) = (self.luminance(), other.luminance());
        let (lighter, darker) = if a > b { (a, b) } else { (b, a) };
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Blend towards another colour. Used to work out what text will actually
    /// sit on once a translucent surface lets the desktop through.
    pub fn mix(self, other: Self, amount: f64) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let lerp =
            |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * amount).round() as u8;
        Self::new(
            lerp(self.r, other.r),
            lerp(self.g, other.g),
            lerp(self.b, other.b),
        )
    }
}

/// How much of what is behind the card comes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Translucency {
    /// The most of the desktop, the least contrast.
    Clear,
    Balanced,
    /// Nearly opaque. The default for long text and the automatic choice when
    /// the system asks for reduced transparency.
    Solid,
}

impl Translucency {
    /// Surface alpha, 0 to 255.
    ///
    /// None of these reach fully opaque except `Solid`, and none go far below
    /// three quarters. Text on a surface thinner than that is being read
    /// against whatever happens to be behind the card, which for a translation
    /// is unacceptable however good it looks.
    pub const fn alpha(self) -> u8 {
        match self {
            Self::Clear => 216,
            Self::Balanced => 236,
            Self::Solid => 255,
        }
    }
}

/// Text size, as a multiplier on each theme's base sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextScale {
    Small,
    Normal,
    Large,
    Larger,
}

impl TextScale {
    pub const fn factor(self) -> f64 {
        match self {
            Self::Small => 0.88,
            Self::Normal => 1.0,
            Self::Large => 1.18,
            Self::Larger => 1.4,
        }
    }
}

/// The four finished looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Follows the system's light or dark setting. The default.
    System,
    /// Warm sage, carried over from Mori's family so the two products look
    /// related without sharing code.
    Forest,
    /// For dark screens and reading at night.
    NightLake,
    /// Almost no translucency, maximum legibility. What a long paragraph and a
    /// bright screen both want.
    Paper,
}

impl Preset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Forest => "forest",
            Self::NightLake => "night-lake",
            Self::Paper => "paper",
        }
    }

    pub const ALL: [Self; 4] = [Self::System, Self::Forest, Self::NightLake, Self::Paper];
}

/// What the operating system says, which outranks any preset.
///
/// These are not preferences Lens offers. A user who has asked Windows for
/// less transparency has already answered the question, and asking again in a
/// second place is how the two end up disagreeing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SystemPreferences {
    pub dark: bool,
    pub reduced_transparency: bool,
    pub high_contrast: bool,
    pub reduced_motion: bool,
}

/// Every colour and measurement the overlay needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub surface: Rgb,
    pub border: Rgb,
    /// The translation. The brightest thing on the card.
    pub body: Rgb,
    /// The original, deliberately quieter but never unreadable.
    pub original: Rgb,
    /// Model, engine, latency.
    pub footer: Rgb,
    /// A failure message.
    pub problem: Rgb,
    pub surface_alpha: u8,
    pub body_pt: i32,
    pub small_pt: i32,
    pub padding: i32,
    pub corner_radius: i32,
    /// Milliseconds for the card to appear. Zero when the system asks for
    /// reduced motion.
    pub entrance_ms: u32,
}

/// The lowest contrast ratio the body text may have against its surface.
///
/// WCAG AA for body text is 4.5, and this is a translation someone is reading
/// rather than a label they are glancing at. Every preset is tested against
/// it, including after a translucent surface has let the worst possible
/// background through.
pub const MIN_BODY_CONTRAST: f64 = 4.5;

/// The lowest contrast the quieter original text may have.
///
/// Lower than the body on purpose, since it is secondary, but not decorative:
/// it is the only way a reader can tell an answer is not a translation of what
/// they pointed at (`docs/privacy-model.md` §1), so it has to be readable.
pub const MIN_ORIGINAL_CONTRAST: f64 = 3.0;

impl Theme {
    /// Resolve a preset and the user's two choices against what the system
    /// asks for.
    pub fn resolve(
        preset: Preset,
        translucency: Translucency,
        scale: TextScale,
        system: SystemPreferences,
    ) -> Self {
        let dark = match preset {
            Preset::System => system.dark,
            Preset::NightLake => true,
            Preset::Forest | Preset::Paper => false,
        };

        let (surface, border, body, original, footer, problem) = match preset {
            Preset::NightLake => (
                Rgb::new(0x12, 0x18, 0x22),
                Rgb::new(0x2b, 0x38, 0x4a),
                Rgb::new(0xe8, 0xf0, 0xfa),
                Rgb::new(0x93, 0xa6, 0xbd),
                Rgb::new(0x6d, 0x7f, 0x95),
                Rgb::new(0x8f, 0xc2, 0xff),
            ),
            Preset::Forest => (
                Rgb::new(0xf2, 0xf4, 0xef),
                Rgb::new(0xc7, 0xd2, 0xc2),
                Rgb::new(0x1e, 0x2b, 0x22),
                Rgb::new(0x53, 0x66, 0x59),
                Rgb::new(0x74, 0x86, 0x7a),
                Rgb::new(0x2f, 0x5d, 0x3f),
            ),
            Preset::Paper => (
                Rgb::new(0xfb, 0xfb, 0xf9),
                Rgb::new(0xc2, 0xc2, 0xbc),
                Rgb::new(0x14, 0x14, 0x14),
                Rgb::new(0x4a, 0x4a, 0x48),
                Rgb::new(0x6b, 0x6b, 0x68),
                Rgb::new(0x8a, 0x2b, 0x1f),
            ),
            Preset::System if dark => (
                Rgb::new(0x1c, 0x1c, 0x1e),
                Rgb::new(0x3a, 0x3a, 0x3d),
                Rgb::new(0xf2, 0xf2, 0xf5),
                Rgb::new(0x98, 0x98, 0x9e),
                Rgb::new(0x74, 0x74, 0x78),
                Rgb::new(0x82, 0xb1, 0xff),
            ),
            Preset::System => (
                Rgb::new(0xf7, 0xf7, 0xf8),
                Rgb::new(0xd0, 0xd0, 0xd3),
                Rgb::new(0x18, 0x18, 0x1a),
                Rgb::new(0x55, 0x55, 0x5a),
                Rgb::new(0x78, 0x78, 0x7e),
                Rgb::new(0x1a, 0x4f, 0xb0),
            ),
        };

        // The system's answers win. Reduced transparency and high contrast are
        // both requests for a surface that does not move under the text.
        let translucency = if system.reduced_transparency || system.high_contrast {
            Translucency::Solid
        } else {
            translucency
        };

        let factor = scale.factor();
        Self {
            surface,
            border,
            body,
            original,
            footer,
            problem,
            surface_alpha: translucency.alpha(),
            body_pt: (19.0 * factor).round() as i32,
            small_pt: (15.0 * factor).round() as i32,
            padding: 14,
            corner_radius: 10,
            entrance_ms: if system.reduced_motion { 0 } else { 120 },
        }
    }

    /// The colour the body text is really sitting on, once a translucent
    /// surface has let `behind` through.
    ///
    /// This is the check that matters. A theme whose contrast is fine against
    /// its own surface can still be unreadable on a white page or a photo,
    /// which is exactly where this card appears.
    pub fn effective_surface(&self, behind: Rgb) -> Rgb {
        let opacity = f64::from(self.surface_alpha) / 255.0;
        behind.mix(self.surface, opacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb = Rgb::new(0xff, 0xff, 0xff);
    const BLACK: Rgb = Rgb::new(0x00, 0x00, 0x00);

    fn every_theme() -> Vec<(String, Theme)> {
        let mut themes = Vec::new();
        for preset in Preset::ALL {
            for translucency in [
                Translucency::Clear,
                Translucency::Balanced,
                Translucency::Solid,
            ] {
                for dark in [false, true] {
                    let system = SystemPreferences {
                        dark,
                        ..SystemPreferences::default()
                    };
                    themes.push((
                        format!("{} {:?} dark={dark}", preset.as_str(), translucency),
                        Theme::resolve(preset, translucency, TextScale::Normal, system),
                    ));
                }
            }
        }
        themes
    }

    #[test]
    fn every_preset_keeps_the_translation_readable_on_its_own_surface() {
        for (name, theme) in every_theme() {
            let contrast = theme.body.contrast_with(theme.surface);
            assert!(
                contrast >= MIN_BODY_CONTRAST,
                "{name}: body contrast {contrast:.2} is below {MIN_BODY_CONTRAST}",
            );
        }
    }

    #[test]
    fn every_preset_keeps_the_original_readable() {
        // The original is the only way a reader can tell an answer is not a
        // translation of what they pointed at, so it is quieter than the body
        // and never decorative.
        for (name, theme) in every_theme() {
            let contrast = theme.original.contrast_with(theme.surface);
            assert!(
                contrast >= MIN_ORIGINAL_CONTRAST,
                "{name}: original contrast {contrast:.2} is below {MIN_ORIGINAL_CONTRAST}",
            );
        }
    }

    /// The check that a design mock-up cannot make.
    #[test]
    fn the_translation_stays_readable_over_a_white_page_and_a_black_one() {
        // A card appears over whatever the user is reading: a white document,
        // a dark editor, a photograph. A translucent surface lets that
        // through, so the contrast that matters is against the blend.
        for (name, theme) in every_theme() {
            for (what, behind) in [("white", WHITE), ("black", BLACK)] {
                let effective = theme.effective_surface(behind);
                let contrast = theme.body.contrast_with(effective);
                assert!(
                    contrast >= MIN_BODY_CONTRAST,
                    "{name} over {what}: body contrast {contrast:.2}",
                );
            }
        }
    }

    #[test]
    fn reduced_transparency_makes_every_surface_solid() {
        let system = SystemPreferences {
            reduced_transparency: true,
            ..SystemPreferences::default()
        };
        for preset in Preset::ALL {
            let theme = Theme::resolve(preset, Translucency::Clear, TextScale::Normal, system);
            assert_eq!(
                theme.surface_alpha,
                Translucency::Solid.alpha(),
                "{} ignored the system's reduced transparency",
                preset.as_str(),
            );
        }
    }

    #[test]
    fn high_contrast_also_makes_the_surface_solid() {
        let system = SystemPreferences {
            high_contrast: true,
            ..SystemPreferences::default()
        };
        let theme = Theme::resolve(
            Preset::System,
            Translucency::Clear,
            TextScale::Normal,
            system,
        );
        assert_eq!(theme.surface_alpha, Translucency::Solid.alpha());
    }

    #[test]
    fn reduced_motion_removes_the_entrance_rather_than_shortening_it() {
        let system = SystemPreferences {
            reduced_motion: true,
            ..SystemPreferences::default()
        };
        let theme = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Normal,
            system,
        );
        assert_eq!(theme.entrance_ms, 0);
    }

    #[test]
    fn the_system_preset_follows_the_system_and_the_others_do_not() {
        let dark = SystemPreferences {
            dark: true,
            ..SystemPreferences::default()
        };
        let light = SystemPreferences::default();

        let system_dark = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Normal,
            dark,
        );
        let system_light = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Normal,
            light,
        );
        assert_ne!(system_dark.surface, system_light.surface);

        // Night Lake is dark whatever the system says; that is what picking it
        // means.
        let night_in_light = Theme::resolve(
            Preset::NightLake,
            Translucency::Balanced,
            TextScale::Normal,
            light,
        );
        let night_in_dark = Theme::resolve(
            Preset::NightLake,
            Translucency::Balanced,
            TextScale::Normal,
            dark,
        );
        assert_eq!(night_in_light.surface, night_in_dark.surface);
    }

    #[test]
    fn a_larger_text_scale_grows_both_sizes() {
        let normal = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Normal,
            SystemPreferences::default(),
        );
        let larger = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Larger,
            SystemPreferences::default(),
        );
        assert!(larger.body_pt > normal.body_pt);
        assert!(larger.small_pt > normal.small_pt);
        // The original must not overtake the translation.
        assert!(larger.small_pt < larger.body_pt);
    }

    #[test]
    fn contrast_is_symmetric_and_bounded() {
        assert!((WHITE.contrast_with(BLACK) - 21.0).abs() < 0.01);
        assert!((BLACK.contrast_with(WHITE) - 21.0).abs() < 0.01);
        assert!((WHITE.contrast_with(WHITE) - 1.0).abs() < 0.01);
    }

    #[test]
    fn a_solid_surface_is_unaffected_by_what_is_behind_it() {
        let theme = Theme::resolve(
            Preset::Paper,
            Translucency::Solid,
            TextScale::Normal,
            SystemPreferences::default(),
        );
        assert_eq!(theme.effective_surface(BLACK), theme.surface);
        assert_eq!(theme.effective_surface(WHITE), theme.surface);
    }
}
