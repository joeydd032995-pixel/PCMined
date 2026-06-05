//! Reliability primitives: exponential backoff with jitter, a circuit breaker,
//! and fallback-pool rotation. These are pure and unit-tested; the poll task and
//! supervisor wire them into live reconnect/recovery behavior.

use rand::Rng;

/// Exponential backoff with **full jitter**. The ceiling doubles each attempt up
/// to `max_ms`; the actual delay is a random value in `[0, ceiling]` to avoid
/// reconnect storms (the thundering-herd problem).
#[derive(Debug, Clone)]
pub struct Backoff {
    base_ms: u64,
    max_ms: u64,
    attempt: u32,
}

impl Backoff {
    pub fn new(base_ms: u64, max_ms: u64) -> Self {
        Backoff { base_ms: base_ms.max(1), max_ms: max_ms.max(1), attempt: 0 }
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }

    /// Current ceiling (deterministic) before jitter: min(max, base·2^attempt).
    pub fn ceiling_ms(&self) -> u64 {
        let shifted = self.base_ms.checked_shl(self.attempt).unwrap_or(u64::MAX);
        shifted.min(self.max_ms)
    }

    /// Advance one attempt and return a jittered delay in `[0, ceiling]`.
    pub fn next_delay_ms(&mut self) -> u64 {
        let ceiling = self.ceiling_ms();
        self.attempt = self.attempt.saturating_add(1);
        rand::rng().random_range(0..=ceiling)
    }
}

/// Circuit-breaker state for a flapping miner/pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Opens after `failure_threshold` consecutive failures; a single success (or a
/// half-open trial success) closes it again.
#[derive(Debug, Clone)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    consecutive_failures: u32,
    state: BreakerState,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32) -> Self {
        CircuitBreaker {
            failure_threshold: failure_threshold.max(1),
            consecutive_failures: 0,
            state: BreakerState::Closed,
        }
    }

    pub fn state(&self) -> BreakerState {
        self.state
    }

    pub fn is_open(&self) -> bool {
        self.state == BreakerState::Open
    }

    pub fn on_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = BreakerState::Closed;
    }

    pub fn on_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        if self.consecutive_failures >= self.failure_threshold {
            self.state = BreakerState::Open;
        }
    }

    /// Allow a single trial after the breaker has been open.
    pub fn half_open(&mut self) {
        if self.state == BreakerState::Open {
            self.state = BreakerState::HalfOpen;
        }
    }
}

/// Next pool index to try, wrapping through the configured fallbacks.
pub fn next_pool_index(current: usize, pool_count: usize) -> usize {
    if pool_count == 0 {
        0
    } else {
        (current + 1) % pool_count
    }
}

/// Whether `consecutive_failures` warrants rotating to the next pool.
pub fn should_rotate(consecutive_failures: u32, rotate_after: u32) -> bool {
    rotate_after > 0 && consecutive_failures >= rotate_after
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_ceiling_doubles_and_caps() {
        let mut b = Backoff::new(100, 2_000);
        assert_eq!(b.ceiling_ms(), 100);
        b.next_delay_ms(); // attempt -> 1
        assert_eq!(b.ceiling_ms(), 200);
        b.next_delay_ms(); // -> 2
        assert_eq!(b.ceiling_ms(), 400);
        for _ in 0..10 {
            b.next_delay_ms();
        }
        assert_eq!(b.ceiling_ms(), 2_000, "ceiling caps at max");
    }

    #[test]
    fn backoff_jitter_within_ceiling_and_resets() {
        let mut b = Backoff::new(50, 1_000);
        for _ in 0..50 {
            let ceiling = b.ceiling_ms();
            let d = b.next_delay_ms();
            assert!(d <= ceiling, "jittered delay {d} exceeded ceiling {ceiling}");
        }
        b.reset();
        assert_eq!(b.ceiling_ms(), 50);
    }

    #[test]
    fn breaker_opens_after_threshold_and_recovers() {
        let mut cb = CircuitBreaker::new(3);
        assert_eq!(cb.state(), BreakerState::Closed);
        cb.on_failure();
        cb.on_failure();
        assert!(!cb.is_open());
        cb.on_failure(); // 3rd → open
        assert!(cb.is_open());
        cb.half_open();
        assert_eq!(cb.state(), BreakerState::HalfOpen);
        cb.on_success();
        assert_eq!(cb.state(), BreakerState::Closed);
    }

    #[test]
    fn pool_rotation_wraps() {
        assert_eq!(next_pool_index(0, 3), 1);
        assert_eq!(next_pool_index(2, 3), 0);
        assert_eq!(next_pool_index(0, 0), 0);
        assert!(should_rotate(3, 3));
        assert!(!should_rotate(2, 3));
        assert!(!should_rotate(5, 0));
    }
}
