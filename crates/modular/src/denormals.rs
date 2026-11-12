//! Flushes denormal floats to zero on the audio thread.
//!
//! A decaying envelope or filter state settles on denormals (below about
//! 1.2e-38), and x86 CPUs compute with them many times slower than with
//! normal floats, so an idle voice could cost far more than a sounding one.
//! Flushed, those values read and land as zero, which no module can hear.

/// Holds the CPU's flush-to-zero mode on for its lifetime and restores the
/// thread's previous mode on drop, so the audio host's own code on the
/// callback thread runs in the mode it set.
pub struct FlushDenormals {
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    previous: usize,
}

impl FlushDenormals {
    #[inline]
    pub fn enter() -> Self {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        {
            let previous = arch::read();
            arch::write(previous | arch::FLUSH);
            Self { previous }
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        Self {}
    }
}

impl Drop for FlushDenormals {
    #[inline]
    fn drop(&mut self) {
        #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
        arch::write(self.previous);
    }
}

#[cfg(target_arch = "x86_64")]
mod arch {
    use std::arch::asm;

    /// MXCSR's flush-to-zero (bit 15) and denormals-are-zero (bit 6).
    pub const FLUSH: usize = 1 << 15 | 1 << 6;

    #[inline(always)]
    pub fn read() -> usize {
        let mut mxcsr: u32 = 0;
        // SAFETY: stores MXCSR to a local.
        unsafe { asm!("stmxcsr [{}]", in(reg) &mut mxcsr, options(nostack, preserves_flags)) };
        mxcsr as usize
    }

    #[inline(always)]
    pub fn write(value: usize) {
        let mxcsr = value as u32;
        // SAFETY: loads MXCSR from a value read from it with only the flush
        // bits changed, which leaves rounding and exception masks as they were.
        unsafe {
            asm!("ldmxcsr [{}]", in(reg) &mxcsr, options(nostack, readonly, preserves_flags))
        };
    }
}

#[cfg(target_arch = "aarch64")]
mod arch {
    use std::arch::asm;

    /// FPCR's flush-to-zero (bit 24), which flushes denormal inputs and
    /// results alike.
    pub const FLUSH: usize = 1 << 24;

    #[inline(always)]
    pub fn read() -> usize {
        let fpcr: usize;
        // SAFETY: reads FPCR.
        unsafe { asm!("mrs {}, fpcr", out(reg) fpcr, options(nomem, nostack, preserves_flags)) };
        fpcr
    }

    #[inline(always)]
    pub fn write(value: usize) {
        // SAFETY: writes FPCR from a value read from it with only the flush
        // bit changed, which leaves rounding and traps as they were.
        unsafe { asm!("msr fpcr, {}", in(reg) value, options(nomem, nostack, preserves_flags)) };
    }
}

#[cfg(test)]
mod tests {
    use super::FlushDenormals;
    use std::hint::black_box;

    /// A product computed where it is called, so it runs in the mode held
    /// there.
    #[inline(never)]
    fn product(a: f32, b: f32) -> f32 {
        black_box(a) * black_box(b)
    }

    #[test]
    fn flushes_denormals_while_held_and_restores_after() {
        let half_min = || product(f32::MIN_POSITIVE, 0.5);
        // Unflushed, this denormal input times 4 is a normal result.
        let denormal_input = || product(f32::from_bits(0x0040_0000), 4.0);
        assert_ne!(half_min().to_bits(), 0);
        assert_eq!(denormal_input(), 2.0 * f32::MIN_POSITIVE);
        {
            let _flush = FlushDenormals::enter();
            assert_eq!(half_min().to_bits(), 0, "results flush");
            assert_eq!(denormal_input().to_bits(), 0, "inputs flush");
            assert_eq!(product(f32::MIN_POSITIVE, 2.0), 2.0 * f32::MIN_POSITIVE);
        }
        assert_ne!(half_min().to_bits(), 0, "the previous mode returns");
    }
}
