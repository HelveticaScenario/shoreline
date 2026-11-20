//! Detects an audio stream that has stopped delivering callbacks.
//!
//! When a device disappears (headphones unplugged, an interface switched off),
//! cpal pauses the stream for good. It reports `DeviceNotAvailable` for a
//! device opened by id, but for the device that was the system default when
//! the stream opened it pauses without a word, so a stalled callback count is
//! the only sign every host gives.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Written by the stream callbacks, read by the main thread's watchdog.
#[derive(Debug, Default)]
pub struct StreamHealth {
    callbacks: AtomicU64,
    lost: AtomicBool,
}

impl StreamHealth {
    /// Count one output callback. Audio thread; never allocates.
    #[inline(always)]
    pub fn beat(&self) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a stream error. Device loss and invalidation end the stream;
    /// an underrun or backend hiccup does not.
    pub fn report(&self, err: &cpal::StreamError) {
        eprintln!("Audio stream error: {err}");
        if matches!(
            err,
            cpal::StreamError::DeviceNotAvailable | cpal::StreamError::StreamInvalidated
        ) {
            self.lost.store(true, Ordering::Relaxed);
        }
    }

    fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }
}

/// How long the callback count may hold still before the stream counts as
/// dead. Longer than any device buffer the engine opens.
const STALL: Duration = Duration::from_secs(1);

/// Least time between two attempts to reopen a dead stream.
pub const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Main-thread half: polls a `StreamHealth` and decides when the stream is
/// dead and when to try reopening it.
#[derive(Debug)]
pub struct StreamWatchdog {
    last_callbacks: u64,
    last_progress: Instant,
    last_check: Instant,
    next_retry: Instant,
    dead: bool,
}

impl StreamWatchdog {
    pub fn new(now: Instant) -> Self {
        Self {
            last_callbacks: 0,
            last_progress: now,
            last_check: now,
            next_retry: now,
            dead: false,
        }
    }

    /// Whether the stream is dead as of `now`. A dead stream never resumes,
    /// so once dead it stays dead.
    ///
    /// A gap between checks longer than the stall window (the machine slept,
    /// or the main thread was busy) restarts the window, since the stream's
    /// callbacks may not have resumed yet either.
    pub fn is_dead(&mut self, health: &StreamHealth, now: Instant) -> bool {
        self.dead = self.dead || health.is_lost() || self.stalled(health, now);
        self.dead
    }

    fn stalled(&mut self, health: &StreamHealth, now: Instant) -> bool {
        let callbacks = health.callbacks();
        let gap = now.saturating_duration_since(self.last_check);
        self.last_check = now;
        if callbacks != self.last_callbacks || gap > STALL {
            self.last_callbacks = callbacks;
            self.last_progress = now;
            return false;
        }
        now.saturating_duration_since(self.last_progress) >= STALL
    }

    /// Whether a reopen may be tried at `now`; claims the attempt if so.
    pub fn take_retry(&mut self, now: Instant) -> bool {
        if now < self.next_retry {
            return false;
        }
        self.next_retry = now + RETRY_INTERVAL;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: Duration = Duration::from_millis(500);

    #[test]
    fn a_stream_with_advancing_callbacks_is_alive() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        for i in 1..10 {
            health.beat();
            assert!(!watchdog.is_dead(&health, start + TICK * i));
        }
    }

    #[test]
    fn a_stream_whose_callbacks_stop_is_dead_after_the_stall_window() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.beat();
        assert!(!watchdog.is_dead(&health, start + TICK));
        assert!(!watchdog.is_dead(&health, start + TICK * 2));
        assert!(watchdog.is_dead(&health, start + TICK * 3));
    }

    #[test]
    fn a_long_gap_between_checks_restarts_the_window() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        let woke = start + Duration::from_secs(60);
        assert!(!watchdog.is_dead(&health, woke));
        health.beat();
        assert!(!watchdog.is_dead(&health, woke + TICK));
    }

    #[test]
    fn a_dead_stream_stays_dead_across_a_long_gap() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        assert!(!watchdog.is_dead(&health, start + TICK));
        assert!(watchdog.is_dead(&health, start + TICK * 3));
        assert!(watchdog.is_dead(&health, start + Duration::from_secs(60)));
    }

    #[test]
    fn a_reported_device_loss_is_dead_at_once() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.beat();
        health.report(&cpal::StreamError::DeviceNotAvailable);
        assert!(watchdog.is_dead(&health, start + TICK));
    }

    #[test]
    fn an_underrun_leaves_the_stream_alive() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.report(&cpal::StreamError::BufferUnderrun);
        health.beat();
        assert!(!watchdog.is_dead(&health, start + TICK));
    }

    #[test]
    fn reopen_attempts_are_spaced() {
        let start = Instant::now();
        let mut watchdog = StreamWatchdog::new(start);
        assert!(watchdog.take_retry(start));
        assert!(!watchdog.take_retry(start + TICK));
        assert!(watchdog.take_retry(start + RETRY_INTERVAL));
    }
}
