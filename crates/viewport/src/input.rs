//! Pointer, wheel and keyboard input, independent of the platform: the browser glue maps DOM
//! events to these, and the tests write them by hand.

/// A pointer button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    /// The left button, a tap or a pen contact.
    Primary,
    /// The wheel button.
    Middle,
    /// The right button.
    Secondary,
}

/// The modifier keys held during an event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent keys, as the DOM reports them"
)]
pub struct Modifiers {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt, Option on macOS.
    pub alt: bool,
    /// Meta: Command on macOS.
    pub meta: bool,
}

impl Modifiers {
    /// Ctrl, or Command on macOS: toggles an element in the selection.
    #[must_use]
    pub const fn toggle(self) -> bool {
        self.ctrl || self.meta
    }
}

/// A key, as far as the canvas cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// ←.
    ArrowLeft,
    /// →.
    ArrowRight,
    /// ↑.
    ArrowUp,
    /// ↓.
    ArrowDown,
    /// Enter.
    Enter,
    /// Esc.
    Escape,
    /// Backspace.
    Backspace,
    /// Alt (Option) on its own.
    Alt,
    /// A key that produces this character (`F`, `+`, a digit…).
    Char(char),
    /// Any other key, Tab included: the canvas leaves it to the browser.
    Other,
}

/// An input event, positions in CSS pixels from the top left of the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputEvent {
    /// A button went down.
    PointerDown {
        /// Where.
        pos: [f64; 2],
        /// Which button.
        button: Button,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// The pointer moved.
    PointerMove {
        /// Where.
        pos: [f64; 2],
        /// The modifiers held.
        mods: Modifiers,
    },
    /// A button went up.
    PointerUp {
        /// Where.
        pos: [f64; 2],
        /// Which button.
        button: Button,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// The pointer left the canvas.
    PointerLeave,
    /// A wheel turn or a two-finger scroll: zooms to the pointer, or pans when `pan` (a
    /// trackpad's two-finger scroll, as the glue tells).
    Wheel {
        /// Where.
        pos: [f64; 2],
        /// The scroll, in CSS pixels; positive away from the user (zooms out).
        delta: [f64; 2],
        /// Whether the scroll pans instead of zooming.
        pan: bool,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// A pinch: `scale` above 1 spreads the fingers (zooms in).
    Pinch {
        /// The centre of the pinch.
        pos: [f64; 2],
        /// The ratio of the spread since the last event.
        scale: f64,
    },
    /// A key went down in the focused canvas.
    KeyDown {
        /// Which.
        key: Key,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// A key went up.
    KeyUp {
        /// Which.
        key: Key,
        /// The modifiers held.
        mods: Modifiers,
    },
    /// The canvas lost the focus: keys held are released.
    Blur,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_or_command_toggles() {
        assert!(!Modifiers::default().toggle());
        assert!(
            Modifiers {
                ctrl: true,
                ..Modifiers::default()
            }
            .toggle()
        );
        assert!(
            Modifiers {
                meta: true,
                ..Modifiers::default()
            }
            .toggle()
        );
    }
}
