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
    /// Read, and currently unused: nothing here animates yet. It is carried so
    /// the platform reader and the theme agree on what has been observed, and
    /// there is deliberately no `entrance_ms` field pretending to act on it.
    pub reduced_motion: bool,
    /// The user's own colours, when high contrast is on.
    ///
    /// Windows asks that a high-contrast application take its colours from
    /// `GetSysColor` rather than pick its own, because the whole point of the
    /// mode is that the user has chosen a pair that works for them. Filled in
    /// by the platform; `lens-core` stays free of `GetSysColor`.
    pub high_contrast_colours: Option<SystemColours>,
}

/// The user's chosen colours, from the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemColours {
    pub window: Rgb,
    pub window_text: Rgb,
    pub gray_text: Rgb,
    pub hot_light: Rgb,
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
    /// Body text height in **points**, not pixels.
    ///
    /// 11pt at the normal scale. Windows' own UI font is 9pt, so the
    /// translation sits a little above the interface around it without
    /// becoming a heading. The first version said 14, which is a heading, and
    /// it was picked while the number was still being passed to `CreateFontW`
    /// as a logical height: converting it to real points then grew the text by
    /// a fifth on a 125% display, where it was reported as too large.
    ///
    /// Points because a card is read at arm's length on displays of different
    /// densities, and a pixel height means two sizes on a desk with a 125%
    /// monitor beside a 100% one. The adapter converts using the DPI of the
    /// monitor the card is on.
    ///
    /// This was previously handed straight to `CreateFontW`, whose height is
    /// in logical units, so the number was called points and behaved as
    /// something else.
    pub body_pt: i32,
    pub small_pt: i32,
    pub padding: i32,
    pub corner_radius: i32,
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

/// The lowest contrast the footer may have.
///
/// The footer is small and incidental, and it carries one thing that is not:
/// whether the answer came from a local engine or left the machine
/// (`docs/privacy-model.md` §1). A privacy signal nobody can read is not a
/// privacy signal, so it gets a floor rather than being treated as decoration.
pub const MIN_FOOTER_CONTRAST: f64 = 3.0;

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
                Rgb::new(0x70, 0x82, 0x97),
                Rgb::new(0x8f, 0xc2, 0xff),
            ),
            Preset::Forest => (
                Rgb::new(0xf2, 0xf4, 0xef),
                Rgb::new(0xc7, 0xd2, 0xc2),
                Rgb::new(0x1e, 0x2b, 0x22),
                Rgb::new(0x53, 0x66, 0x59),
                Rgb::new(0x60, 0x6f, 0x65),
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
                Rgb::new(0x83, 0x83, 0x87),
                Rgb::new(0x82, 0xb1, 0xff),
            ),
            Preset::System => (
                Rgb::new(0xf7, 0xf7, 0xf8),
                Rgb::new(0xd0, 0xd0, 0xd3),
                Rgb::new(0x18, 0x18, 0x1a),
                Rgb::new(0x55, 0x55, 0x5a),
                Rgb::new(0x6e, 0x6e, 0x74),
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

        // In high contrast the user has already chosen a colour pair that
        // works for them, and a preset that keeps its own palette is
        // overriding an accessibility setting with a preference. Earlier this
        // file forced the surface opaque and then carried on painting Forest
        // sage, which was reported as the system winning when only the
        // transparency did.
        let (surface, border, body, original, footer, problem) =
            match (system.high_contrast, system.high_contrast_colours) {
                (true, Some(system_colours)) => (
                    system_colours.window,
                    system_colours.window_text,
                    system_colours.window_text,
                    system_colours.window_text,
                    system_colours.gray_text,
                    system_colours.hot_light,
                ),
                _ => (surface, border, body, original, footer, problem),
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
            body_pt: (11.0 * factor).round() as i32,
            small_pt: (9.0 * factor).round() as i32,
            padding: 14,
            corner_radius: 10,
        }
    }

    /// What a colour on this card actually looks like once the window's
    /// uniform alpha has blended it with whatever is behind.
    ///
    /// **The window alpha applies to the whole window, not just its
    /// background.** `SetLayeredWindowAttributes` with `LWA_ALPHA` sets the
    /// opacity of everything the window draws, so the text fades along with
    /// the surface it sits on.
    ///
    /// The first version of this file blended only the surface and left the
    /// text at full strength, which models a composition nothing performs. It
    /// made every contrast figure optimistic, and it is why the footer went
    /// untested at 2.75:1 while the tests were green.
    ///
    /// It is now a **lower bound rather than a description**, and deliberately
    /// kept that way. The Windows card is drawn with per-pixel alpha, where
    /// the surface is translucent and the ink on it is not, so the real
    /// contrast there is better than this says. The fallback painter, used
    /// where Direct2D cannot start, still fades the ink with the surface
    /// exactly as modelled here.
    ///
    /// Two renderers, one number, and it holds for the worse of them. Raising
    /// it to describe the better one would mean a theme that passes on a
    /// machine with Direct2D and fails on a machine without it, which is a
    /// readability guarantee that depends on the graphics stack.
    pub fn composited(&self, colour: Rgb, behind: Rgb) -> Rgb {
        behind.mix(colour, f64::from(self.surface_alpha) / 255.0)
    }

    /// The surface as it appears over `behind`.
    pub fn effective_surface(&self, behind: Rgb) -> Rgb {
        self.composited(self.surface, behind)
    }

    /// The contrast a reader actually gets for one role over `behind`.
    pub fn effective_contrast(&self, ink: Rgb, behind: Rgb) -> f64 {
        self.composited(ink, behind)
            .contrast_with(self.effective_surface(behind))
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

    /// Every role, every preset, every translucency, over the two
    /// backgrounds a card most often lands on.
    ///
    /// Composited the way the window actually composites: the uniform alpha
    /// fades the ink as well as the surface. The previous version of this test
    /// blended only the surface, which is a composition nothing performs, and
    /// it reported figures that were optimistic for every role and never
    /// looked at the footer at all.
    #[test]
    fn every_role_stays_readable_over_a_white_page_and_a_black_one() {
        let mut worst: Vec<(String, f64, f64)> = Vec::new();

        for (name, theme) in every_theme() {
            for (what, behind) in [("white", WHITE), ("black", BLACK)] {
                for (role, ink, floor) in [
                    ("body", theme.body, MIN_BODY_CONTRAST),
                    ("original", theme.original, MIN_ORIGINAL_CONTRAST),
                    ("footer", theme.footer, MIN_FOOTER_CONTRAST),
                    ("problem", theme.problem, MIN_ORIGINAL_CONTRAST),
                ] {
                    let contrast = theme.effective_contrast(ink, behind);
                    worst.push((format!("{name} {role} over {what}"), contrast, floor));
                    assert!(
                        contrast >= floor,
                        "{name}: {role} over {what} is {contrast:.2}, below {floor}",
                    );
                }
            }
        }

        // Print the tightest few, so a change that eats the margin is visible
        // in the output rather than only when it finally crosses the line.
        worst.sort_by(|a, b| (a.1 - a.2).partial_cmp(&(b.1 - b.2)).unwrap());
        for (what, contrast, floor) in worst.iter().take(3) {
            println!("tightest: {what} {contrast:.2} against a floor of {floor}");
        }
    }

    #[test]
    fn a_solid_surface_composites_to_itself() {
        // At full opacity there is nothing behind to blend with, so the
        // effective colours must be the declared ones.
        let theme = Theme::resolve(
            Preset::Paper,
            Translucency::Solid,
            TextScale::Normal,
            SystemPreferences::default(),
        );
        assert_eq!(theme.composited(theme.body, BLACK), theme.body);
        assert_eq!(theme.composited(theme.body, WHITE), theme.body);
    }

    /// The bug the old model hid, kept as a test.
    #[test]
    fn fading_only_the_surface_would_overstate_the_contrast() {
        // Blending the surface but not the ink is what the first version did.
        // It is not a composition any window performs, and it flatters every
        // theme. If someone reintroduces it, the two numbers will diverge and
        // this fails.
        let theme = Theme::resolve(
            Preset::Forest,
            Translucency::Clear,
            TextScale::Normal,
            SystemPreferences::default(),
        );
        let honest = theme.effective_contrast(theme.footer, WHITE);
        let flattering = theme.footer.contrast_with(theme.effective_surface(WHITE));
        assert!(
            flattering > honest,
            "the old model reported {flattering:.2} where the real one gives {honest:.2}",
        );
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

    /// High contrast must take the user's colours, not just go opaque.
    #[test]
    fn high_contrast_uses_the_system_colours_over_the_preset() {
        let chosen = SystemColours {
            window: Rgb::new(0, 0, 0),
            window_text: Rgb::new(0xff, 0xff, 0x00),
            gray_text: Rgb::new(0x00, 0xff, 0xff),
            hot_light: Rgb::new(0xff, 0x00, 0xff),
        };
        let system = SystemPreferences {
            high_contrast: true,
            high_contrast_colours: Some(chosen),
            ..SystemPreferences::default()
        };

        // Forest is a light sage theme. In high contrast none of it survives.
        let theme = Theme::resolve(
            Preset::Forest,
            Translucency::Clear,
            TextScale::Normal,
            system,
        );
        assert_eq!(theme.surface, chosen.window, "kept its own background");
        assert_eq!(theme.body, chosen.window_text, "kept its own text colour");
        assert_eq!(theme.footer, chosen.gray_text);
        assert_eq!(theme.surface_alpha, Translucency::Solid.alpha());
    }

    /// High contrast with no colours available must still be safe.
    #[test]
    fn high_contrast_without_colours_still_goes_solid() {
        // The platform could not read the pair. The surface must at least stop
        // moving, rather than the preset carrying on translucent.
        let system = SystemPreferences {
            high_contrast: true,
            high_contrast_colours: None,
            ..SystemPreferences::default()
        };
        let theme = Theme::resolve(
            Preset::Forest,
            Translucency::Clear,
            TextScale::Normal,
            system,
        );
        assert_eq!(theme.surface_alpha, Translucency::Solid.alpha());
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

    /// A guard against the number drifting into heading territory again.
    #[test]
    fn the_body_stays_in_the_range_a_card_wants() {
        // Windows' UI font is 9pt. A translation should read a little larger
        // than the interface around it and never like a headline: 14pt was
        // reported as excessive on a 125% display, where it renders at 23px.
        let theme = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Normal,
            SystemPreferences::default(),
        );
        assert!(
            (10..=12).contains(&theme.body_pt),
            "body is {}pt, which is outside what a card wants",
            theme.body_pt,
        );
        assert!(
            (8..=10).contains(&theme.small_pt),
            "small is {}pt",
            theme.small_pt,
        );
        // Even at the largest scale a reader chose, it stays readable rather
        // than becoming a poster.
        let largest = Theme::resolve(
            Preset::System,
            Translucency::Balanced,
            TextScale::Larger,
            SystemPreferences::default(),
        );
        assert!(largest.body_pt <= 18, "largest is {}pt", largest.body_pt);
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
