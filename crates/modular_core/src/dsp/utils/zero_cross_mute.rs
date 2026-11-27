//! Start mute that holds a fresh voice silent until its output crosses zero.

/// Which crossing of zero releases a [`ZeroCrossMute`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    /// From at or below zero to at or above it.
    Rising,
    /// From at or above zero to at or below it.
    Falling,
    /// Any change of sign, or landing on zero.
    Either,
}

impl Crossing {
    /// Whether the output crossed from `prev` to `out`.
    #[inline(always)]
    fn crossed(self, prev: f32, out: f32) -> bool {
        match self {
            Crossing::Rising => prev <= 0.0 && out >= 0.0,
            Crossing::Falling => prev >= 0.0 && out <= 0.0,
            Crossing::Either => (out > 0.0) != (prev > 0.0) || out == 0.0,
        }
    }
}

/// A voice's start mute, so a voice that starts away from zero does not click.
/// It lives in lane state: a voice carried over by a patch update keeps it, and
/// only a voice that has never started arms it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ZeroCrossMute {
    /// Not started yet; it passes the output until [`start`](Self::start).
    #[default]
    Fresh,
    /// Silent until the output crosses zero from this value. With none, the
    /// first sample releases it only at exactly zero and becomes the value.
    Waiting(Option<f32>),
    Open,
}

impl ZeroCrossMute {
    /// Arms a fresh voice to wait for a crossing from `from` when `wait`, or
    /// opens it. A voice already started keeps its state.
    #[inline(always)]
    pub fn start(&mut self, wait: bool, from: Option<f32>) {
        if *self == ZeroCrossMute::Fresh {
            *self = if wait {
                ZeroCrossMute::Waiting(from)
            } else {
                ZeroCrossMute::Open
            };
        }
    }

    /// `out`, or silence while the voice waits for `crossing`. The crossing
    /// sample itself sounds.
    #[inline(always)]
    pub fn pass(&mut self, out: f32, crossing: Crossing) -> f32 {
        let ZeroCrossMute::Waiting(prev) = *self else {
            return out;
        };
        let crossed = match prev {
            Some(prev) => crossing.crossed(prev, out),
            None => out == 0.0,
        };
        if crossed {
            *self = ZeroCrossMute::Open;
            out
        } else {
            *self = ZeroCrossMute::Waiting(Some(out));
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mut mute: ZeroCrossMute, crossing: Crossing, outs: &[f32]) -> Vec<f32> {
        outs.iter().map(|&out| mute.pass(out, crossing)).collect()
    }

    fn waiting(from: Option<f32>) -> ZeroCrossMute {
        let mut mute = ZeroCrossMute::default();
        mute.start(true, from);
        mute
    }

    #[test]
    fn rising_releases_only_going_up_through_zero() {
        let mute = waiting(Some(0.5));
        assert_eq!(
            run(mute, Crossing::Rising, &[0.2, -0.1, -0.3, 0.0, 0.4, -0.2]),
            [0.0, 0.0, 0.0, 0.0, 0.4, -0.2]
        );
        let mute = waiting(Some(-0.2));
        assert_eq!(run(mute, Crossing::Rising, &[0.0, 0.3]), [0.0, 0.3]);
    }

    #[test]
    fn falling_releases_only_going_down_through_zero() {
        let mute = waiting(Some(-0.5));
        assert_eq!(
            run(mute, Crossing::Falling, &[-0.2, 0.1, 0.3, -0.1, 0.5]),
            [0.0, 0.0, 0.0, -0.1, 0.5]
        );
        let mute = waiting(Some(0.2));
        assert_eq!(run(mute, Crossing::Falling, &[0.0, 0.3]), [0.0, 0.3]);
    }

    #[test]
    fn either_releases_on_a_change_of_sign_or_on_zero() {
        let mute = waiting(Some(0.5));
        assert_eq!(
            run(mute, Crossing::Either, &[0.2, -0.1, 0.3]),
            [0.0, -0.1, 0.3]
        );
        let mute = waiting(Some(-0.5));
        assert_eq!(run(mute, Crossing::Either, &[-0.2, 0.1]), [0.0, 0.1]);
        let mute = waiting(Some(-0.5));
        assert_eq!(run(mute, Crossing::Either, &[-0.2, 0.0]), [0.0, 0.0]);
    }

    #[test]
    fn with_no_value_to_cross_from_the_first_sample_is_held() {
        assert_eq!(
            run(waiting(None), Crossing::Either, &[0.4, 0.3, -0.1]),
            [0.0, 0.0, -0.1]
        );
        let mut mute = waiting(None);
        assert_eq!(mute.pass(0.0, Crossing::Rising), 0.0);
        assert_eq!(mute, ZeroCrossMute::Open);
    }

    #[test]
    fn a_voice_that_does_not_wait_plays_at_once() {
        let mut mute = ZeroCrossMute::default();
        mute.start(false, Some(0.5));
        assert_eq!(run(mute, Crossing::Rising, &[0.4, 0.3]), [0.4, 0.3]);
    }

    #[test]
    fn a_started_voice_keeps_its_state() {
        let mut mute = waiting(Some(0.5));
        mute.pass(0.4, Crossing::Rising);
        mute.start(false, None);
        assert_eq!(mute, ZeroCrossMute::Waiting(Some(0.4)));
        let mut open = ZeroCrossMute::Open;
        open.start(true, Some(0.5));
        assert_eq!(run(open, Crossing::Rising, &[0.4]), [0.4]);
    }
}
