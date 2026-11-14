//! Scale-degree value type of scale patterns.
//!
//! A pattern under `.scale(scale)` parses its integer atoms as
//! [`IntervalValue`]s; `seq::lower` gives each its scale, adds `.trans`
//! degrees with `sp_combine`, and resolves it to a V/Oct voltage.

/// Value type for scale-degree patterns: either a degree or a rest.
#[derive(Clone, Debug)]
pub enum IntervalValue {
    /// Scale degree (can be negative for downward movement)
    Degree(i32),
    /// Rest - no output, gate low
    Rest,
}

impl IntervalValue {
    pub fn degree(&self) -> Option<i32> {
        match self {
            IntervalValue::Degree(d) => Some(*d),
            IntervalValue::Rest => None,
        }
    }
}

impl crate::pattern_system::mini::convert::FromMiniAtom for IntervalValue {
    fn from_atom(
        atom: &crate::pattern_system::mini::ast::AtomValue,
    ) -> Result<Self, crate::pattern_system::mini::convert::ConvertError> {
        use crate::pattern_system::mini::ast::AtomValue;
        use crate::pattern_system::mini::convert::ConvertError;
        match atom {
            AtomValue::Number(n) => {
                if !n.is_finite() || n.fract() != 0.0 {
                    return Err(ConvertError::InvalidAtom(format!(
                        "IntervalValue requires integer scale degrees, got {n}"
                    )));
                }
                Ok(IntervalValue::Degree(*n as i32))
            }
            AtomValue::Hz(_) => Err(ConvertError::InvalidAtom(
                "a scale pattern reads its atoms as scale-degree integers, so Hz atoms are not allowed under .scale(...)".into(),
            )),
            AtomValue::Note { .. } => Err(ConvertError::InvalidAtom(
                "a scale pattern reads its atoms as scale-degree integers, so note names are not allowed under .scale(...)".into(),
            )),
            AtomValue::Truthy => Err(ConvertError::InvalidAtom(
                "'x' is a structure marker with no pitch; it is only valid inside .struct() boolean patterns".into(),
            )),
        }
    }

    fn from_list(
        atoms: &[crate::pattern_system::mini::ast::AtomValue],
    ) -> Result<Self, crate::pattern_system::mini::convert::ConvertError> {
        if atoms.len() == 1 {
            Self::from_atom(&atoms[0])
        } else {
            Err(crate::pattern_system::mini::convert::ConvertError::ListNotSupported)
        }
    }

    fn combine_with_head(
        _head_atoms: &[crate::pattern_system::mini::ast::AtomValue],
        _tail: &Self,
    ) -> Result<Self, crate::pattern_system::mini::convert::ConvertError> {
        Err(crate::pattern_system::mini::convert::ConvertError::ListNotSupported)
    }

    fn rest_value() -> Option<Self> {
        Some(IntervalValue::Rest)
    }

    fn supports_rest() -> bool {
        true
    }
}

impl crate::pattern_system::mini::convert::HasRest for IntervalValue {
    fn rest_value() -> Self {
        IntervalValue::Rest
    }
}

/// Error returned when a pattern source string is empty or whitespace-only.
/// A rest (`~`) is a real atom and stays valid.
pub(crate) const EMPTY_PATTERN_SOURCE_ERR: &str =
    "empty pattern source: a pattern string must contain at least one atom";
