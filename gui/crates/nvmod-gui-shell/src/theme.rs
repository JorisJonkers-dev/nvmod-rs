// SPDX-License-Identifier: GPL-3.0-only
//! The Mojave Dusk and Bleach colour tokens of `docs/design/gui/03-design-tokens-and-components.md` in nv-linux,
//! as plain data any toolkit can paint, and the contrast check that keeps the table honest.

// The tokens are written as the design table writes them (`0xffa23d`), and the casts take bytes out of those.
#![allow(clippy::unreadable_literal, clippy::cast_possible_truncation)]

/// An sRGB colour with an alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha, 255 is opaque.
    pub a: u8,
}

impl Color {
    /// An opaque colour from `0xRRGGBB`.
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
            a: 255,
        }
    }

    /// A translucent colour from `0xRRGGBB` and an alpha in percent (0 to 100).
    pub const fn hex_alpha(rgb: u32, percent: u8) -> Self {
        let mut c = Self::hex(rgb);
        c.a = ((percent as u16 * 255 + 50) / 100) as u8;
        c
    }

    /// `0xRRGGBBAA`, the form GPUI's `rgba` takes.
    pub const fn rgba_u32(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | self.a as u32
    }

    /// This colour laid over an opaque `ground`.
    #[must_use]
    pub fn over(self, ground: Self) -> Self {
        let a = f64::from(self.a) / 255.0;
        let mix = |top: u8, under: u8| {
            // The result is between 0 and 255 by construction.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (f64::from(top) * a + f64::from(under) * (1.0 - a)).round() as u8;
            v
        };
        Self {
            r: mix(self.r, ground.r),
            g: mix(self.g, ground.g),
            b: mix(self.b, ground.b),
            a: 255,
        }
    }

    fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }
}

/// The WCAG contrast ratio of two opaque colours.
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// One theme: the semantic tokens of the design table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// The window ground.
    pub bg: Color,
    /// Inset fields.
    pub bg2: Color,
    /// Panels (translucent).
    pub surface: Color,
    /// Decorative outlines and dividers.
    pub edge: Color,
    /// Outlines of controls.
    pub control: Color,
    /// Body text.
    pub text: Color,
    /// Secondary text.
    pub muted: Color,
    /// Tertiary text.
    pub faint: Color,
    /// Primary action and the current step.
    pub accent: Color,
    /// The brighter accent (hover).
    pub accent2: Color,
    /// Text on an accent fill.
    pub accent_ink: Color,
    /// Information and the equivalent-command strip.
    pub sky: Color,
    /// Success.
    pub ok: Color,
    /// Warning.
    pub warn: Color,
    /// Error.
    pub err: Color,
    /// The focus ring.
    pub focus: Color,
}

impl Palette {
    /// Dusk: the dark signature theme.
    pub const DUSK: Self = Self {
        bg: Color::hex(0x0e0c12),
        bg2: Color::hex(0x15111a),
        surface: Color::hex_alpha(0x221c28, 78),
        edge: Color::hex(0x4a3f52),
        control: Color::hex(0x8d7f9d),
        text: Color::hex(0xf3e9db),
        muted: Color::hex(0xb9aa9a),
        faint: Color::hex(0x9a8d80),
        accent: Color::hex(0xffa23d),
        accent2: Color::hex(0xffc36b),
        accent_ink: Color::hex(0x1d0f03),
        sky: Color::hex(0x63cfe0),
        ok: Color::hex(0x8fdc9c),
        warn: Color::hex(0xf6d466),
        err: Color::hex(0xff7a6b),
        focus: Color::hex(0x7fe9ff),
    };

    /// Bleach: the light theme.
    pub const BLEACH: Self = Self {
        bg: Color::hex(0xefe5d3),
        bg2: Color::hex(0xe7dac4),
        surface: Color::hex_alpha(0xfffaf1, 92),
        edge: Color::hex(0xb49a7c),
        control: Color::hex(0x756146),
        text: Color::hex(0x2a1e14),
        muted: Color::hex(0x5e4d3d),
        faint: Color::hex(0x6e5e4e),
        accent: Color::hex(0xa54307),
        accent2: Color::hex(0xbb5512),
        accent_ink: Color::hex(0xffffff),
        sky: Color::hex(0x0b6a85),
        ok: Color::hex(0x25663a),
        warn: Color::hex(0x7a5100),
        err: Color::hex(0xb3281f),
        focus: Color::hex(0x0a56b8),
    };

    /// The panel colour laid over the ground, opaque.
    pub fn surface_on_bg(&self) -> Color {
        self.surface.over(self.bg)
    }

    /// Every pair the design table promises a threshold for: name, foreground, ground, minimum ratio.
    pub fn promised_pairs(&self) -> Vec<(&'static str, Color, Color, f64)> {
        let surface = self.surface_on_bg();
        vec![
            ("text on bg", self.text, self.bg, 4.5),
            ("muted on bg", self.muted, self.bg, 4.5),
            ("faint on bg", self.faint, self.bg, 4.5),
            ("text on surface", self.text, surface, 4.5),
            ("muted on surface", self.muted, surface, 4.5),
            ("faint on surface", self.faint, surface, 4.5),
            ("control on surface", self.control, surface, 3.0),
            ("focus on bg", self.focus, self.bg, 3.0),
            ("accent-ink on accent", self.accent_ink, self.accent, 4.5),
            ("accent-ink on accent2", self.accent_ink, self.accent2, 4.5),
            ("ok on surface", self.ok, surface, 4.5),
            ("warn on surface", self.warn, surface, 4.5),
            ("err on surface", self.err, surface, 4.5),
            ("sky on surface", self.sky, surface, 4.5),
        ]
    }
}

/// Space tokens, in logical pixels (gaps are multiples of 4).
pub mod space {
    /// 4.
    pub const XS: f32 = 4.0;
    /// 8.
    pub const S: f32 = 8.0;
    /// 12.
    pub const M: f32 = 12.0;
    /// 16.
    pub const L: f32 = 16.0;
    /// 24.
    pub const XL: f32 = 24.0;
    /// The minimum height of a regular button.
    pub const BUTTON_HEIGHT: f32 = 40.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_rgba_round_trip() {
        let c = Color::hex(0xffa23d);
        assert_eq!(c.rgba_u32(), 0xffa2_3dff);
        assert_eq!(Color::hex_alpha(0x000000, 50).a, 128);
    }

    #[test]
    fn contrast_of_black_on_white_is_21() {
        let r = contrast(Color::hex(0x000000), Color::hex(0xffffff));
        assert!((r - 21.0).abs() < 1e-9, "{r}");
    }

    #[test]
    fn dusk_meets_every_promised_threshold() {
        check(&Palette::DUSK);
    }

    #[test]
    fn bleach_meets_every_promised_threshold() {
        check(&Palette::BLEACH);
    }

    fn check(p: &Palette) {
        for (name, fg, bg, min) in p.promised_pairs() {
            let r = contrast(fg, bg);
            assert!(r >= min, "{name}: {r:.2} is below {min}");
        }
    }
}
