//! The retry waits of [`Backoff`].

use std::time::Duration;

use super::{
    jittered, random_fraction, Backoff, BACKOFF_INITIAL_INTERVAL, BACKOFF_MAX_INTERVAL,
    BACKOFF_MULTIPLIER, BACKOFF_RANDOMIZATION_FACTOR,
};

#[test]
fn a_zero_retry_duration_never_retries() {
    let mut backoff = Backoff::new(Duration::ZERO);
    std::thread::sleep(Duration::from_millis(1));

    assert_eq!(backoff.next_backoff(), None);
}

#[test]
fn the_waits_grow_and_stay_within_the_jitter_and_the_maximum() {
    let mut backoff = Backoff::new(Duration::from_secs(24 * 60 * 60));
    let mut interval = BACKOFF_INITIAL_INTERVAL;
    for _ in 0..30 {
        let wait = backoff
            .next_backoff()
            .expect("a long retry duration retries");
        assert!(
            wait >= interval.mul_f64(1.0 - BACKOFF_RANDOMIZATION_FACTOR),
            "{wait:?}"
        );
        assert!(
            wait <= interval.mul_f64(1.0 + BACKOFF_RANDOMIZATION_FACTOR),
            "{wait:?}"
        );
        interval = interval
            .mul_f64(BACKOFF_MULTIPLIER)
            .min(BACKOFF_MAX_INTERVAL);
    }
    assert_eq!(interval, BACKOFF_MAX_INTERVAL);
}

#[test]
fn the_jitter_spans_the_whole_range() {
    let interval = Duration::from_secs(10);

    assert_eq!(
        jittered(interval, 0.0),
        interval.mul_f64(1.0 - BACKOFF_RANDOMIZATION_FACTOR)
    );
    assert_eq!(jittered(interval, 0.5), interval);
    assert_eq!(
        jittered(interval, 1.0),
        interval.mul_f64(1.0 + BACKOFF_RANDOMIZATION_FACTOR)
    );
}

#[test]
fn a_random_fraction_is_from_0_to_1() {
    for _ in 0..1000 {
        let fraction = random_fraction();
        assert!((0.0..1.0).contains(&fraction), "{fraction}");
    }
}
