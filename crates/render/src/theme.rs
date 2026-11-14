//! The colours of the viewport and the display state of an object (VP-01).
//!
//! The viewport colours come from the design tokens of the UI (`--color-vp-*`), which are
//! `#RRGGBB` strings: [`Rgb::from_hex`] parses and validates them. The renderer works in linear
//! light, as the colours of its objects and its clear colour do, so [`Rgb::linear`] decodes the
//! sRGB transfer function.

/// An opaque colour in sRGB, 8 bits per channel: what a `#RRGGBB` token says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    /// Red, 0 to 255.
    pub r: u8,
    /// Green, 0 to 255.
    pub g: u8,
    /// Blue, 0 to 255.
    pub b: u8,
}

impl Rgb {
    /// Parses `#RRGGBB`, in either case of letters.
    ///
    /// # Errors
    ///
    /// [`ThemeError::MissingHash`], [`ThemeError::WrongLength`] or [`ThemeError::NotHexadecimal`].
    pub fn from_hex(text: &str) -> Result<Self, ThemeError> {
        let digits = text.strip_prefix('#').ok_or(ThemeError::MissingHash)?;
        // Six bytes, so a multi-byte character cannot hide in six `char`s.
        if digits.len() != 6 {
            return Err(ThemeError::WrongLength);
        }
        let channel = |at: usize| {
            digits
                .get(at..at + 2)
                .filter(|pair| pair.bytes().all(|byte| byte.is_ascii_hexdigit()))
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or(ThemeError::NotHexadecimal)
        };
        Ok(Self {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
        })
    }

    /// The colour as `#RRGGBB`, in capitals.
    #[must_use]
    pub fn to_hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// The colour in linear light, each channel between 0 and 1: what the renderer draws with.
    #[must_use]
    pub fn linear(self) -> [f32; 3] {
        [self.r, self.g, self.b].map(decode)
    }
}

/// The sRGB transfer function, inverted: an 8-bit channel to linear light.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the result is between 0 and 1, and the GPU computes in f32"
)]
fn decode(channel: u8) -> f32 {
    let encoded = f64::from(channel) / 255.0;
    let linear = if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        libm::pow((encoded + 0.055) / 1.055, 2.4)
    };
    linear as f32
}

/// Why a colour cannot be parsed. Messages never contain the text (SEC-03).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ThemeError {
    /// The colour does not start with `#`.
    #[error("a colour starts with `#`")]
    MissingHash,
    /// The colour is not six digits long.
    #[error("a colour has six hexadecimal digits")]
    WrongLength,
    /// A digit of the colour is not hexadecimal.
    #[error("a colour has only hexadecimal digits")]
    NotHexadecimal,
}

/// The colours of the viewport: the UI reads them from its tokens and hands them to the renderer
/// ([`crate::Renderer::set_theme`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// The canvas, the clear colour of a frame (`--color-vp-bg`).
    pub background: Rgb,
    /// Selected objects (`--color-vp-select`).
    pub selection: Rgb,
    /// The hovered object (`--color-vp-hover`).
    pub hover: Rgb,
    /// Snap markers (`--color-vp-snap`).
    pub snap: Rgb,
    /// Previews and ghosts (`--color-vp-ghost`).
    pub preview: Rgb,
    /// Cut faces (`--color-vp-cut`).
    pub cut: Rgb,
    /// The halo under every overlay line and marker, so they stay visible over any model colour
    /// (`--color-vp-halo`).
    pub halo: Rgb,
}

impl Theme {
    /// The light theme of the design (board 3.28).
    #[must_use]
    pub const fn light() -> Self {
        Self {
            background: rgb(0xF4, 0xF5, 0xF6),
            selection: rgb(0x0F, 0x6B, 0x5E),
            hover: rgb(0xB2, 0x5E, 0x00),
            snap: rgb(0xB4, 0x13, 0x7B),
            preview: rgb(0x5C, 0x64, 0x70),
            cut: rgb(0xB4, 0x41, 0x0A),
            halo: rgb(0xFF, 0xFF, 0xFF),
        }
    }

    /// The dark theme of the design (board 3.28).
    #[must_use]
    pub const fn dark() -> Self {
        Self {
            background: rgb(0x16, 0x18, 0x1B),
            selection: rgb(0x4D, 0xB6, 0xA3),
            hover: rgb(0xF5, 0xB0, 0x4A),
            snap: rgb(0xFF, 0x7A, 0xD0),
            preview: rgb(0x9A, 0xA3, 0xAE),
            cut: rgb(0xFD, 0xBA, 0x74),
            halo: rgb(0x0B, 0x0C, 0x0E),
        }
    }

    /// The clear colour of a frame: the background, linear, opaque.
    #[must_use]
    pub fn clear_color(&self) -> [f64; 4] {
        let [r, g, b] = self.background.linear();
        [f64::from(r), f64::from(g), f64::from(b), 1.0]
    }

    /// The colours the CPU mixes into object colours.
    pub(crate) fn tints(&self) -> Tints {
        Tints {
            hover: self.hover.linear(),
            selection: self.selection.linear(),
            preview: self.preview.linear(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// How an object is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisplayState {
    /// In its own colour.
    #[default]
    Normal,
    /// Under the pointer: tinted with the hover colour.
    Hovered,
    /// Part of the selection: tinted with the selection colour.
    Selected,
    /// A translucent preview in the preview colour: not drawn with the opaque objects, so it
    /// hides nothing and can be hidden by them.
    Ghost,
}

/// How much of the hover and selection colours replaces the colour of the object.
const HOVER_MIX: f32 = 0.4;
const SELECTION_MIX: f32 = 0.6;

/// The opacity of ghosts and of translucent boxes.
pub(crate) const GHOST_ALPHA: f32 = 0.4;
pub(crate) const BOX_ALPHA: f32 = 0.25;

/// The linear colours of the display states, from a [`Theme`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tints {
    hover: [f32; 3],
    selection: [f32; 3],
    pub(crate) preview: [f32; 3],
}

impl Default for Tints {
    fn default() -> Self {
        Theme::default().tints()
    }
}

impl Tints {
    /// The colour an object of `base` colour is drawn in, in `state`.
    pub(crate) fn apply(&self, state: DisplayState, base: [f32; 4]) -> [f32; 4] {
        let mix = |tint: [f32; 3], amount: f32| {
            [
                base[0] + (tint[0] - base[0]) * amount,
                base[1] + (tint[1] - base[1]) * amount,
                base[2] + (tint[2] - base[2]) * amount,
                base[3],
            ]
        };
        match state {
            DisplayState::Normal => base,
            DisplayState::Hovered => mix(self.hover, HOVER_MIX),
            DisplayState::Selected => mix(self.selection, SELECTION_MIX),
            DisplayState::Ghost => {
                let [r, g, b] = self.preview;
                [r, g, b, GHOST_ALPHA]
            }
        }
    }
}

#[cfg(test)]
#[expect(
    clippy::float_cmp,
    reason = "the colours compared are exact: pure channels and the identity of a copy"
)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_parse_in_either_case() {
        assert_eq!(
            Rgb::from_hex("#0F6B5E"),
            Ok(Rgb {
                r: 0x0F,
                g: 0x6B,
                b: 0x5E
            })
        );
        assert_eq!(Rgb::from_hex("#0f6b5e"), Rgb::from_hex("#0F6B5E"));
        assert_eq!(
            Rgb::from_hex("#000000").map(Rgb::to_hex).unwrap(),
            "#000000"
        );
        assert_eq!(
            Rgb::from_hex("#b4137b").map(Rgb::to_hex).unwrap(),
            "#B4137B"
        );
    }

    #[test]
    fn malformed_colours_are_refused() {
        assert_eq!(Rgb::from_hex("0F6B5E"), Err(ThemeError::MissingHash));
        assert_eq!(Rgb::from_hex(""), Err(ThemeError::MissingHash));
        for text in ["#", "#FFF", "#0F6B5E0", "#0F6B5E00", "# 0F6B5E"] {
            assert_eq!(Rgb::from_hex(text), Err(ThemeError::WrongLength), "{text}");
        }
        // Six bytes, but not six digits; the last one has two bytes.
        for text in ["#0F6B5G", "#0F6B+E", "#-F6B5E", "#0F6B\u{e9}"] {
            assert_eq!(
                Rgb::from_hex(text),
                Err(ThemeError::NotHexadecimal),
                "{text}"
            );
        }
        // A multi-byte character that straddles a pair must not panic.
        assert_eq!(
            Rgb::from_hex("#0\u{e9}B5E"),
            Err(ThemeError::NotHexadecimal)
        );
        assert_eq!(
            Rgb::from_hex("#\u{e9}\u{e9}\u{e9}"),
            Err(ThemeError::NotHexadecimal)
        );
    }

    #[test]
    fn errors_do_not_echo_the_text() {
        let message = Rgb::from_hex("#secret").unwrap_err().to_string();
        assert!(!message.contains("secret"));
    }

    #[test]
    fn linear_light_decodes_the_srgb_curve() {
        assert_eq!(Rgb { r: 0, g: 0, b: 0 }.linear(), [0.0; 3]);
        assert_eq!(
            Rgb {
                r: 255,
                g: 255,
                b: 255
            }
            .linear(),
            [1.0; 3]
        );
        // The linear segment, then the power segment: 188 is 0.737 in sRGB, 0.503 in light.
        let [low, mid, _] = Rgb {
            r: 10,
            g: 188,
            b: 0,
        }
        .linear();
        assert!((low - 0.003_035).abs() < 1e-5, "{low}");
        assert!((mid - 0.502_886).abs() < 1e-4, "{mid}");
    }

    #[test]
    fn the_clear_colour_is_the_linear_background() {
        let theme = Theme {
            background: Rgb { r: 0, g: 255, b: 0 },
            ..Theme::light()
        };
        assert_eq!(theme.clear_color(), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(Theme::default(), Theme::light());
        assert_ne!(Theme::dark(), Theme::light());
    }

    #[test]
    fn display_states_mix_the_theme_colours_into_the_object_colour() {
        let theme = Theme {
            selection: Rgb { r: 0, g: 255, b: 0 },
            hover: Rgb { r: 0, g: 0, b: 255 },
            preview: Rgb {
                r: 255,
                g: 255,
                b: 255,
            },
            ..Theme::light()
        };
        let tints = theme.tints();
        let red = [1.0, 0.0, 0.0, 0.8];
        assert_eq!(tints.apply(DisplayState::Normal, red), red);
        let [r, g, b, a] = tints.apply(DisplayState::Selected, red);
        assert!((r - 0.4).abs() < 1e-6 && (g - 0.6).abs() < 1e-6 && b == 0.0 && a == 0.8);
        let [r, g, b, a] = tints.apply(DisplayState::Hovered, red);
        assert!((r - 0.6).abs() < 1e-6 && g == 0.0 && (b - 0.4).abs() < 1e-6);
        assert_eq!(a, 0.8);
        // A ghost ignores the colour of the object and is translucent.
        assert_eq!(
            tints.apply(DisplayState::Ghost, red),
            [1.0, 1.0, 1.0, GHOST_ALPHA]
        );
        assert_eq!(DisplayState::default(), DisplayState::Normal);
        assert_eq!(Tints::default(), Theme::default().tints());
    }
}
