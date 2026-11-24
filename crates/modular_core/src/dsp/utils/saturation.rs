//! A rational `tanh`, `x·(27 + x²)/(27 + 9x²)`, which reaches ±1 at ±3 with
//! zero slope and holds there.

/// `tanh(x)`.
#[inline(always)]
pub fn tanh_pade(x: f32) -> f32 {
    let x2 = x * x;
    if x2 < 9.0 {
        x * (27.0 + x2) / (27.0 + 9.0 * x2)
    } else {
        x.signum()
    }
}

/// `tanh(x) / x`, 1 at the origin.
#[inline]
pub fn tanh_ratio(x: f32) -> f32 {
    let x2 = x * x;
    if x2 < 9.0 {
        (27.0 + x2) / (27.0 + 9.0 * x2)
    } else {
        1.0 / x.abs()
    }
}

/// `tanh(x)` and its slope there.
#[inline]
pub fn tanh_and_slope(x: f32) -> (f32, f32) {
    let x2 = x * x;
    let slope = if x2 < 9.0 {
        let root = (9.0 - x2) / (9.0 + 3.0 * x2);
        root * root
    } else {
        0.0
    };
    (tanh_pade(x), slope)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_tanh_and_its_own_slope() {
        for i in -400..=400 {
            let x = i as f32 * 0.01;
            let (y, slope) = tanh_and_slope(x);
            assert!((y - x.tanh()).abs() < 0.03, "{x}: {y}");
            assert!((y - x * tanh_ratio(x)).abs() < 1e-6);
            let h = 1e-3;
            let numeric = (tanh_and_slope(x + h).0 - tanh_and_slope(x - h).0) / (2.0 * h);
            assert!((slope - numeric).abs() < 2e-3, "{x}: {slope} vs {numeric}");
        }
    }
}
