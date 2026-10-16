// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The retry policy that a download and a wait share: exponential backoff
//! with jitter, and a sleep that a caller can cut short.

use std::time::{Duration, Instant};

use super::AsvoError;

#[cfg(test)]
mod tests;

/// The longest the library sleeps before it asks the caller's `should_stop`
/// again, while it waits to retry.
const STOP_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Sleep for `wait`, and stop early with [`AsvoError::Interrupted`] if
/// `should_stop` (when there is one) says that the caller wants to stop.
pub(crate) fn sleep_unless_stopped(
    wait: Duration,
    should_stop: Option<&dyn Fn() -> bool>,
) -> Result<(), AsvoError> {
    let Some(should_stop) = should_stop else {
        std::thread::sleep(wait);
        return Ok(());
    };
    let deadline = Instant::now() + wait;
    loop {
        if should_stop() {
            return Err(AsvoError::Interrupted);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(());
        }
        std::thread::sleep((deadline - now).min(STOP_CHECK_INTERVAL));
    }
}

/// The first wait before a retry, before the jitter.
const BACKOFF_INITIAL_INTERVAL: Duration = Duration::from_millis(500);
/// How much each wait grows over the one before.
const BACKOFF_MULTIPLIER: f64 = 1.5;
/// The longest wait before a retry, before the jitter.
const BACKOFF_MAX_INTERVAL: Duration = Duration::from_secs(60);
/// The jitter: each wait is a random value within this fraction of the
/// interval either side, so that parallel downloads do not retry together.
const BACKOFF_RANDOMIZATION_FACTOR: f64 = 0.5;

/// The retry policy: exponential backoff with jitter. A download uses it
/// ([`DownloadOptions::retry_duration`](crate::DownloadOptions::retry_duration)),
/// and so does the job list request of a wait
/// ([`AsvoClient::get_jobs_retrying`](crate::AsvoClient::get_jobs_retrying)).
///
/// Transient failures (a dropped connection, a 5xx, a hash mismatch) are
/// retried for `retry_duration`. The
/// waits start at [`BACKOFF_INITIAL_INTERVAL`] and grow by
/// [`BACKOFF_MULTIPLIER`] up to [`BACKOFF_MAX_INTERVAL`]. Zero disables
/// retrying, which is what the test suite uses: a test that deliberately
/// triggers a transient failure would otherwise sit in backoff for fifteen
/// minutes.
pub(crate) struct Backoff {
    /// When the first attempt started.
    start: Instant,
    /// How long retries may go on, from `start`.
    max_elapsed: Duration,
    /// The next wait, before the jitter.
    interval: Duration,
}

impl Backoff {
    /// A new policy, which retries for `retry_duration` from now.
    pub(crate) fn new(retry_duration: Duration) -> Self {
        Self {
            start: Instant::now(),
            max_elapsed: retry_duration,
            interval: BACKOFF_INITIAL_INTERVAL,
        }
    }

    /// The wait before the next retry, or `None` to give up: the wait would
    /// end after `max_elapsed`.
    pub(crate) fn next_backoff(&mut self) -> Option<Duration> {
        let elapsed = self.start.elapsed();
        if elapsed > self.max_elapsed {
            return None;
        }
        let wait = jittered(self.interval, random_fraction());
        self.interval = self
            .interval
            .mul_f64(BACKOFF_MULTIPLIER)
            .min(BACKOFF_MAX_INTERVAL);
        (elapsed + wait <= self.max_elapsed).then_some(wait)
    }
}

/// `interval`, moved by up to [`BACKOFF_RANDOMIZATION_FACTOR`] of itself:
/// `fraction` (from 0 to 1) picks the point in that range.
fn jittered(interval: Duration, fraction: f64) -> Duration {
    let spread = BACKOFF_RANDOMIZATION_FACTOR * (2.0 * fraction - 1.0);
    interval.mul_f64(1.0 + spread)
}

/// A random number from 0 to 1, for the jitter. It needs no random number
/// crate: the standard library seeds each `RandomState` at random.
fn random_fraction() -> f64 {
    use std::hash::{BuildHasher, Hasher};
    let bits = std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish();
    (bits >> 11) as f64 / (1u64 << 53) as f64
}
