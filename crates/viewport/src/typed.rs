//! The value being typed during a move or a placement: digits and one decimal point, in metres
//! (no unit suffix: letters are global shortcuts, `design/ux-notes.md` flow 4).

/// The longest text kept.
const MAX_LENGTH: usize = 16;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Typed {
    text: String,
}

impl Typed {
    /// Adds a typed character; `false` when it cannot extend the number (the text is unchanged).
    pub(crate) fn push(&mut self, c: char) -> bool {
        let accepted = (c.is_ascii_digit() || (c == '.' && !self.text.contains('.')))
            && self.text.len() < MAX_LENGTH;
        if accepted {
            self.text.push(c);
        }
        accepted
    }

    /// Removes the last character; `false` when there was none.
    pub(crate) fn pop(&mut self) -> bool {
        self.text.pop().is_some()
    }

    pub(crate) fn clear(&mut self) {
        self.text.clear();
    }

    /// The text, or `None` when nothing is typed.
    pub(crate) fn text(&self) -> Option<&str> {
        (!self.text.is_empty()).then_some(self.text.as_str())
    }

    /// The value typed, when it is a number.
    pub(crate) fn value(&self) -> Option<f64> {
        self.text.parse::<f64>().ok().filter(|v| v.is_finite())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_and_one_point_make_a_value() {
        let mut typed = Typed::default();
        assert_eq!((typed.text(), typed.value()), (None, None));
        for c in "2.5".chars() {
            assert!(typed.push(c));
        }
        assert!(!typed.push('.'), "one decimal point only");
        assert!(!typed.push('m'));
        assert_eq!((typed.text(), typed.value()), (Some("2.5"), Some(2.5)));
        assert!(typed.pop());
        assert_eq!(typed.value(), Some(2.0));
        typed.clear();
        assert!(!typed.pop());
        assert!(typed.push('.'));
        assert_eq!(typed.value(), None);
        for _ in 0..40 {
            typed.push('9');
        }
        assert_eq!(typed.text().map(str::len), Some(MAX_LENGTH));
    }
}
