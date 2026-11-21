//! Detects an audio stream that has stopped delivering callbacks.
//!
//! When a device disappears (headphones unplugged, an interface switched off)
//! or changes its sample rate, cpal pauses the stream for good and reports
//! why. A driver can also stop calling back without an error, so a callback
//! count that holds still ends the stream too.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Written by the stream callbacks, read by the main thread's watchdog.
#[derive(Debug, Default)]
pub struct StreamHealth {
    callbacks: AtomicU64,
    lost: AtomicBool,
    invalidated: AtomicBool,
}

impl StreamHealth {
    /// Count one output callback. Audio thread; never allocates.
    #[inline(always)]
    pub fn beat(&self) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a stream error. Device loss and invalidation (the device's
    /// sample rate changed under it) end the stream; an xrun or backend
    /// hiccup does not.
    pub fn report(&self, err: &cpal::Error) {
        match err.kind() {
            cpal::ErrorKind::DeviceNotAvailable => {
                eprintln!("Audio stream error: {err}");
                self.lost.store(true, Ordering::Relaxed);
            }
            cpal::ErrorKind::StreamInvalidated => {
                eprintln!("Audio stream error: {err}");
                self.invalidated.store(true, Ordering::Relaxed);
            }
            cpal::ErrorKind::Xrun => {}
            _ => eprintln!("Audio stream error: {err}"),
        }
    }

    fn callbacks(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    /// The stream ended: its device left or changed format.
    pub fn has_ended(&self) -> bool {
        self.is_lost() || self.invalidated.load(Ordering::Relaxed)
    }

    /// The stream ended because its device changed format, not because the
    /// device left.
    pub fn was_invalidated(&self) -> bool {
        self.invalidated.load(Ordering::Relaxed) && !self.is_lost()
    }
}

/// How long the callback count may hold still before the stream counts as
/// dead. Several main-process poll intervals, and longer than any device takes
/// to deliver its first callback.
const STALL: Duration = Duration::from_secs(3);

/// A gap between checks this long (the machine slept, or the main thread was
/// busy) restarts the stall window, since the stream's callbacks may not have
/// resumed yet either.
const LONG_GAP: Duration = Duration::from_secs(10);

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
    loss_reported: bool,
}

impl StreamWatchdog {
    pub fn new(now: Instant) -> Self {
        Self {
            last_callbacks: 0,
            last_progress: now,
            last_check: now,
            next_retry: now,
            dead: false,
            loss_reported: false,
        }
    }

    /// Whether the stream is dead as of `now`. A dead stream never resumes,
    /// so once dead it stays dead.
    pub fn is_dead(&mut self, health: &StreamHealth, now: Instant) -> bool {
        self.dead = self.dead || health.has_ended() || self.stalled(health, now);
        self.dead
    }

    /// Whether the last `is_dead` check found the stream dead.
    pub fn has_died(&self) -> bool {
        self.dead
    }

    fn stalled(&mut self, health: &StreamHealth, now: Instant) -> bool {
        let callbacks = health.callbacks();
        let gap = now.saturating_duration_since(self.last_check);
        self.last_check = now;
        if callbacks != self.last_callbacks || gap > LONG_GAP {
            self.last_callbacks = callbacks;
            self.last_progress = now;
            return false;
        }
        now.saturating_duration_since(self.last_progress) >= STALL
    }

    /// True the first time it is called for this stream, so a loss that
    /// cannot be recovered from yet is reported once.
    pub fn take_first_loss(&mut self) -> bool {
        !std::mem::replace(&mut self.loss_reported, true)
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

    const TICK: Duration = Duration::from_secs(1);

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
        assert!(!watchdog.is_dead(&health, start + TICK * 3));
        assert!(watchdog.is_dead(&health, start + TICK * 4));
    }

    #[test]
    fn a_stall_is_noticed_when_the_checks_run_late() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.beat();
        let late = Duration::from_millis(1_010);
        assert!(!watchdog.is_dead(&health, start + late));
        assert!(!watchdog.is_dead(&health, start + late * 2));
        assert!(!watchdog.is_dead(&health, start + late * 3));
        assert!(watchdog.is_dead(&health, start + late * 4));
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
        assert!(watchdog.is_dead(&health, start + STALL));
        assert!(watchdog.is_dead(&health, start + Duration::from_secs(60)));
    }

    #[test]
    fn a_reported_device_loss_is_dead_at_once() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.beat();
        health.report(&cpal::ErrorKind::DeviceNotAvailable.into());
        assert!(watchdog.is_dead(&health, start + TICK));
        assert!(!health.was_invalidated());
    }

    #[test]
    fn a_format_change_ends_the_stream_as_invalidated() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.beat();
        health.report(&cpal::ErrorKind::StreamInvalidated.into());
        assert!(watchdog.is_dead(&health, start + TICK));
        assert!(health.was_invalidated());
    }

    #[test]
    fn a_loss_is_reported_once_per_stream() {
        let mut watchdog = StreamWatchdog::new(Instant::now());
        assert!(watchdog.take_first_loss());
        assert!(!watchdog.take_first_loss());
    }

    #[test]
    fn an_underrun_leaves_the_stream_alive() {
        let start = Instant::now();
        let health = StreamHealth::default();
        let mut watchdog = StreamWatchdog::new(start);
        health.report(&cpal::ErrorKind::Xrun.into());
        health.beat();
        assert!(!watchdog.is_dead(&health, start + TICK));
    }

    #[test]
    fn reopen_attempts_are_spaced() {
        let start = Instant::now();
        let mut watchdog = StreamWatchdog::new(start);
        assert!(watchdog.take_retry(start));
        assert!(!watchdog.take_retry(start + RETRY_INTERVAL / 2));
        assert!(watchdog.take_retry(start + RETRY_INTERVAL));
    }
}
