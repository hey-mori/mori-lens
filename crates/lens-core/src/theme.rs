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
    /// What a card uses when nobody has chosen.
    ///
    /// Here rather than at each call site, because it was at each call site: the
    /// overlay picked one and four rendering diagnostics each picked their own,
    /// so changing the product's default left every diagnostic drawing the old
    /// card and reporting it as the current one.
    ///
    /// **Translucent again, 2026-08-18.** It was opaque, and the reason given
    /// was that a translucent panel takes part of its edge colour from whatever
    /// is behind it, so one hairline reads differently along its own length and
    /// over a page of text looks blotchy rather than thin.
    ///
    /// That was true of an edge drawn at partial alpha, and measuring it found
    /// the fix was in the edge rather than in the surface. The same hairline,
    /// seen over a white page and a black one, as a contrast between the two:
    ///
    /// ```text
    ///                 drawn at alpha   drawn as a colour
    ///   solid         1.00             1.00
    ///   balanced      1.06 to 1.16     1.00
    ///   clear         1.14 to 1.37     1.00
    /// ```
    ///
    /// [`Theme::edge_colour`] mixes the softness in ahead of time and the line
    /// is drawn opaque, so it is one colour at every point. The blotchiness is
    /// structurally zero rather than small, which is what makes this reversible
    /// and what `the_hairline_does_not_take_its_colour_from_the_page` asserts.
    ///
    /// `Clear` rather than `Balanced` because a reader asked for the glass
    /// back, and at 236 of 255 there is not much to see. Both other levels stay
    /// available in `settings.conf`.
    pub const DEFAULT: Self = Self::Clear;

    /// The name this level is written under in the settings file.
    ///
    /// Here so the default can be one fact rather than two. It was two: this
    /// enum said the default was opaque and `settings.rs` said the string
    /// `"balanced"`, and since the product builds its theme from the string,
    /// the enum's default reached the fallback theme and every test fixture
    /// and nothing a user ever saw. The banner printed `surface alpha 236`
    /// under a change that claimed to make it 255.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::Balanced => "balanced",
            Self::Solid => "solid",
        }
    }

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
    /// Whether the user has asked for less movement.
    ///
    /// This was carried and unused for a while, with a note saying there was
    /// deliberately no field pretending to act on it. There is one now, below,
    /// because the card animates its entrance and that is exactly the kind of
    /// movement this setting is about.
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
    /// How firmly the card's single hairline is drawn, 0 to 1.
    ///
    /// **The whole boundary rests on this.** There is no shadow: it was built
    /// three times, called a weird box every time by the reader it was for, and
    /// removed rather than tuned a fourth time. The hairline was raised to 0.65
    /// then, to carry alone what two elements had been carrying.
    ///
    /// 0.65 turned out to be the box. Lowered 2026-08-18 to the figure below,
    /// on numbers rather than on a second opinion, and the numbers say
    /// something worth knowing: **on a light card over a light page the edge's
    /// boxiness and its usefulness are the same measurement.** The card's own
    /// surface composites to nearly the page's colour, so the hairline is both
    /// the only thing separating them and the only thing standing out inside
    /// them. Softening one softens the other and there is no setting that does
    /// not.
    ///
    /// What that leaves is a floor rather than a preference:
    /// [`Theme::edge_against`] must stay above [`Theme::FAINTEST_USABLE_EDGE`]
    /// for every preset over both a white and a black page, which 0.30 does not
    /// clear and this does.
    pub border_alpha: f32,
    /// The translation. The brightest thing on the card.
    pub body: Rgb,
    /// The original, deliberately quieter but never unreadable.
    pub original: Rgb,
    /// Model, engine, latency.
    pub footer: Rgb,
    /// A failure message.
    ///
    /// Amber in every preset, and it was blue in two of them and red in a
    /// third. Blue is the one colour a card can use that promises something is
    /// clickable, and this card passes every click through to the application
    /// underneath; red is an alarm, and a translation that did not arrive is
    /// not one. Amber is the colour for "look at this, nothing is broken".
    pub problem: Rgb,
    pub surface_alpha: u8,
    /// Whether a card may animate its arrival.
    ///
    /// False when the system asks for reduced motion, and then the card simply
    /// appears. An entrance is the one piece of movement this product has, so
    /// this is the whole of what that setting means here.
    pub animate: bool,
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
    /// The model, where it ran, and how long it took.
    ///
    /// Its own size, and the smallest on the card, because it is the only line
    /// that is not the reader's text: it is there to answer "where did this
    /// come from", which is a question asked once and then never again. At the
    /// original's size it read as a third paragraph.
    pub footer_pt: i32,
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
    /// How firmly the hairline is drawn. See [`Theme::border_alpha`].
    ///
    /// Measured over a white page, where a light card has no other boundary:
    ///
    /// ```text
    ///                     edge against the surface   edge against the page
    ///   0.65 (was)        1.42                       1.46
    ///   0.42 (is)         1.24                       1.29
    ///   0.30              1.16                       1.21
    ///   0.25              1.13                       1.17, under the floor
    /// ```
    ///
    /// The first column is the box the reader kept seeing and the second is the
    /// card having an edge at all. They move together, which is the finding.
    pub const BORDER_ALPHA: f32 = 0.42;

    /// The least an edge may contrast with the page behind it.
    ///
    /// Not a taste figure. Below this, a light card on a white page has no
    /// boundary a reader can find, which is the state the hairline was raised
    /// to 0.65 to escape.
    pub const FAINTEST_USABLE_EDGE: f64 = 1.2;

    /// The hairline's colour, which does not depend on what is behind it.
    ///
    /// **Softened by colour rather than by alpha, and that is the whole point.**
    /// A hairline drawn at partial alpha takes part of its colour from the page,
    /// so it reads differently along its own length over a page of text. That
    /// blotchiness is the recorded reason the product stopped shipping a
    /// translucent card at all.
    ///
    /// Measured 2026-08-18, the same edge seen over a white page and a black
    /// one, as a contrast between the two:
    ///
    /// ```text
    ///                       drawn at alpha   drawn as a colour
    ///   solid               1.00             1.00
    ///   balanced            1.06 to 1.16     1.00
    ///   clear               1.14 to 1.37     1.00
    /// ```
    ///
    /// And it gets **worse** as the line is softened, which was the surprise: a
    /// fainter line is more surface and less border, and the surface is the part
    /// that varies. Softening the edge and making it translucent are the same
    /// mistake twice.
    ///
    /// Mixed here instead, and drawn opaque, so the line is one colour at every
    /// point regardless of the page and regardless of the translucency setting.
    pub fn edge_colour(&self) -> Rgb {
        self.surface.mix(self.border, f64::from(self.border_alpha))
    }

    /// The hairline contrasted with the page behind the card.
    ///
    /// This is the card having an edge at all.
    pub fn edge_against(&self, behind: Rgb) -> f64 {
        self.edge_colour().contrast_with(behind)
    }

    /// The hairline contrasted with the card it sits on.
    ///
    /// This is the number that reads as a box when it is large. Measured
    /// against the surface's own colour rather than the composited one: the
    /// edge is a fixed colour and this is the design intent, where what a
    /// translucent card does to that relationship is a consequence and is
    /// bounded by [`Theme::edge_against`] instead.
    pub fn edge_within(&self) -> f64 {
        self.edge_colour().contrast_with(self.surface)
    }

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
                Rgb::new(0xe8, 0xb3, 0x6a),
            ),
            Preset::Forest => (
                Rgb::new(0xf2, 0xf4, 0xef),
                Rgb::new(0xc7, 0xd2, 0xc2),
                Rgb::new(0x1e, 0x2b, 0x22),
                Rgb::new(0x53, 0x66, 0x59),
                Rgb::new(0x60, 0x6f, 0x65),
                Rgb::new(0x7d, 0x4f, 0x14),
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
                Rgb::new(0xe0, 0xa9, 0x5c),
            ),
            Preset::System => (
                Rgb::new(0xf7, 0xf7, 0xf8),
                Rgb::new(0xd0, 0xd0, 0xd3),
                Rgb::new(0x18, 0x18, 0x1a),
                Rgb::new(0x55, 0x55, 0x5a),
                Rgb::new(0x6e, 0x6e, 0x74),
                Rgb::new(0x8a, 0x55, 0x0c),
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
            border_alpha: Self::BORDER_ALPHA,
            body,
            original,
            footer,
            problem,
            surface_alpha: translucency.alpha(),
            // The system's answer wins, exactly as it does for transparency
            // and contrast above. A card that slides in for somebody who has
            // asked their whole machine to stop moving is not a nice touch.
            animate: !system.reduced_motion,
            body_pt: (10.0 * factor).round() as i32,
            small_pt: (9.0 * factor).round() as i32,
            footer_pt: (8.0 * factor).round() as i32,
            padding: 9,
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

    /// Every card keeps an edge a reader can find, on any page.
    ///
    /// The hairline is the card's whole boundary: there is no shadow, and on a
    /// light preset over a white page the surface composites to almost the
    /// page's own colour, so this line is the only thing saying where the card
    /// stops. That is why it has a floor and why the floor is not a taste.
    ///
    /// The upper bound is the taste, and it is the thing the reader complained
    /// about twice: an edge that stands out inside its own card reads as a box
    /// drawn around the text rather than as the card's outline.
    #[test]
    fn the_hairline_is_findable_on_any_page_and_a_box_on_none() {
        /// Above this, the edge reads as a box rather than as an outline. Set
        /// from the figure 0.65 produced, 1.42, which is the one a reader
        /// objected to, with room underneath rather than at it.
        const READS_AS_A_BOX: f64 = 1.30;

        for preset in Preset::ALL {
            for dark in [false, true] {
                let system = SystemPreferences {
                    dark,
                    ..SystemPreferences::default()
                };
                let theme = Theme::resolve(preset, Translucency::Clear, TextScale::Normal, system);
                let within = theme.edge_within();
                assert!(
                    within <= READS_AS_A_BOX,
                    "{} draws a box, not an outline: {within:.2}",
                    preset.as_str(),
                );
                for page in [Rgb::new(255, 255, 255), Rgb::new(0, 0, 0)] {
                    let against = theme.edge_against(page);
                    assert!(
                        against >= Theme::FAINTEST_USABLE_EDGE,
                        "{} on {page:?} has no findable edge: {against:.2}",
                        preset.as_str(),
                    );
                }
            }
        }
    }

    /// The hairline is one colour, whatever is behind the card.
    ///
    /// This is what makes a translucent card possible at all. The edge used to
    /// be drawn at partial alpha, so it took part of its colour from the page
    /// and read differently along its own length; over a page of text that is
    /// blotchy rather than thin, and it is the recorded reason the product
    /// stopped shipping translucency.
    ///
    /// Measured at every level, because the failure only appears at some of
    /// them and a test at the default would have seen nothing.
    #[test]
    fn the_hairline_does_not_take_its_colour_from_the_page() {
        for translucency in [
            Translucency::Solid,
            Translucency::Balanced,
            Translucency::Clear,
        ] {
            for preset in Preset::ALL {
                let theme = Theme::resolve(
                    preset,
                    translucency,
                    TextScale::Normal,
                    SystemPreferences::default(),
                );
                // One colour, computed without reference to any page.
                let edge = theme.edge_colour();
                assert_eq!(
                    theme.edge_against(Rgb::new(255, 255, 255)),
                    edge.contrast_with(Rgb::new(255, 255, 255)),
                );
                assert_eq!(
                    theme.edge_against(Rgb::new(0, 0, 0)),
                    edge.contrast_with(Rgb::new(0, 0, 0)),
                );
            }
        }
    }

    /// Translucency changes the card and not its edge.
    ///
    /// The property the previous test states structurally, stated again as the
    /// thing a reader would notice: the same preset at three levels draws the
    /// same hairline.
    #[test]
    fn every_translucency_draws_the_same_hairline() {
        for preset in Preset::ALL {
            let of = |translucency| {
                Theme::resolve(
                    preset,
                    translucency,
                    TextScale::Normal,
                    SystemPreferences::default(),
                )
                .edge_colour()
            };
            assert_eq!(of(Translucency::Solid), of(Translucency::Balanced));
            assert_eq!(of(Translucency::Solid), of(Translucency::Clear));
        }
    }

    /// The floor is where it says it is, and 0.25 does not clear it.
    ///
    /// Without this the constant could drift down one commit at a time and the
    /// test above would keep passing, because it only ever measures the value
    /// that is set.
    ///
    /// The failing figure was 0.30 while the edge was drawn at partial alpha.
    /// A fixed colour holds up better against a page, so 0.30 now clears the
    /// floor and 0.25 is the nearest value that does not. That is the bound
    /// moving because the drawing changed, not because the floor did.
    #[test]
    fn a_fainter_hairline_would_lose_the_edge() {
        let system = SystemPreferences::default();
        let mut theme = Theme::resolve(
            Preset::Paper,
            Translucency::Clear,
            TextScale::Normal,
            system,
        );
        let page = Rgb::new(255, 255, 255);
        assert!(theme.edge_against(page) >= Theme::FAINTEST_USABLE_EDGE);

        theme.border_alpha = 0.25;
        assert!(
            theme.edge_against(page) < Theme::FAINTEST_USABLE_EDGE,
            "0.25 clears the floor, so the floor is not measuring anything",
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
        // The three sizes are a hierarchy, and it has to hold at every scale a
        // reader can pick: the answer is the loudest thing on the card and the
        // provenance line is the quietest. A footer that grew past the original
        // would make the card's least important line look like a paragraph.
        for scale in [
            TextScale::Small,
            TextScale::Normal,
            TextScale::Large,
            TextScale::Larger,
        ] {
            let at = Theme::resolve(
                Preset::System,
                Translucency::DEFAULT,
                scale,
                SystemPreferences::default(),
            );
            assert!(
                at.footer_pt < at.small_pt && at.small_pt < at.body_pt,
                "{}pt footer, {}pt small, {}pt body",
                at.footer_pt,
                at.small_pt,
                at.body_pt,
            );
            assert!(
                at.footer_pt >= 7,
                "footer is {}pt, too small to read",
                at.footer_pt
            );
        }
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

#[cfg(test)]
mod problem_colour {
    use super::*;

    /// Amber in every preset, and it was blue in two and red in a third.
    ///
    /// Blue is the one colour a card can use that promises something is
    /// clickable, and this card passes every click through to whatever is
    /// underneath (ADR-0004). A user reported the card and the blue read as a
    /// link on it.
    #[test]
    fn a_problem_is_never_drawn_in_a_colour_that_looks_like_a_link() {
        for preset in [
            Preset::System,
            Preset::Forest,
            Preset::NightLake,
            Preset::Paper,
        ] {
            for dark in [false, true] {
                let theme = Theme::resolve(
                    preset,
                    Translucency::Balanced,
                    TextScale::Normal,
                    SystemPreferences {
                        dark,
                        ..SystemPreferences::default()
                    },
                );
                let problem = theme.problem;
                assert!(
                    problem.r > problem.b && problem.g > problem.b,
                    "{preset:?} dark={dark} draws a problem in {problem:?}, which is bluer than it is warm",
                );
            }
        }
    }
}

#[cfg(test)]
mod motion {
    use super::*;

    #[test]
    fn a_card_animates_by_default() {
        assert!(
            Theme::resolve(
                Preset::System,
                Translucency::Balanced,
                TextScale::Normal,
                SystemPreferences::default(),
            )
            .animate
        );
    }

    /// The one piece of movement this product has, and the setting that turns
    /// it off. Asserted for every preset, because a preset that kept its own
    /// answer would be overriding an accessibility setting with a preference,
    /// which is the mistake the transparency rule above already made once.
    #[test]
    fn reduced_motion_stops_every_preset_animating() {
        for preset in [
            Preset::System,
            Preset::Forest,
            Preset::NightLake,
            Preset::Paper,
        ] {
            let theme = Theme::resolve(
                preset,
                Translucency::Balanced,
                TextScale::Normal,
                SystemPreferences {
                    reduced_motion: true,
                    ..SystemPreferences::default()
                },
            );
            assert!(!theme.animate, "{} kept animating", preset.as_str());
        }
    }
}
