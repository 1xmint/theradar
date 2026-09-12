// SPDX-License-Identifier: Apache-2.0
//! A small per-key cache with a time-to-live, for market answers.
//!
//! [`crate::cache::Cache`] is one entry keyed on the local store's watermark —
//! exactly right for a scoreboard computed once per store flush. This module
//! answers a different question: fifty visitors reading *different* mints at
//! once, from a source (CryptoHouse) that never advances a watermark this
//! server owns. A single-entry cache would evict itself on every other
//! request — two visitors alternating between two coins would never see a
//! hit — so this holds several keys at once and bounds itself by count rather
//! than by watermark, because there is no watermark to key on.
//!
//! Wall-clock TTL rather than exact, for the same reason: CryptoHouse is
//! itself about three seconds behind the chain (ADR 0002's measurement), so an
//! answer a few seconds stale is not distinguishable from ordinary network
//! latency by anyone looking at the screen.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bounds how many distinct keys are held at once. Fifty concurrent visitors
/// asking about fifty different mints is exactly the load this exists for;
/// without a bound, a caller naming a new key every request would grow this
/// forever.
const MAX_ENTRIES: usize = 256;

/// A bounded, time-limited cache of computed market answers.
pub struct TtlCache<K, V> {
    entries: Mutex<HashMap<K, (Instant, Arc<V>)>>,
    ttl: Duration,
}

impl<K: Eq + Hash + Clone, V> TtlCache<K, V> {
    /// A cache whose entries are recomputed after `ttl`.
    #[must_use]
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// The cached value for `key`, computing and storing it if absent or
    /// past its `ttl`.
    ///
    /// `compute` runs **outside the lock**, matching [`crate::cache::Cache`]:
    /// two callers arriving together on a cold key both compute, which wastes
    /// one CryptoHouse round trip; holding the lock across it would instead
    /// block every other request — for a different key too — for however
    /// long that round trip takes.
    ///
    /// # Errors
    ///
    /// Whatever `compute` returns. A failure is never stored: a transient
    /// CryptoHouse error must not become the answer for the rest of the TTL.
    pub fn get_or_compute<E>(
        &self,
        key: K,
        compute: impl FnOnce() -> Result<V, E>,
    ) -> Result<Arc<V>, E> {
        if let Some(hit) = self.peek(&key) {
            return Ok(hit);
        }
        let value = Arc::new(compute()?);
        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if guard.len() >= MAX_ENTRIES && !guard.contains_key(&key) {
            // No access order is tracked, so the entry dropped is arbitrary.
            // That costs one duplicated computation the next time it is
            // asked for -- never a correctness problem, since a stale entry
            // is never served past its own TTL regardless of which one this
            // evicts.
            if let Some(stale_key) = guard.keys().next().cloned() {
                guard.remove(&stale_key);
            }
        }
        guard.insert(key, (Instant::now(), Arc::clone(&value)));
        Ok(value)
    }

    fn peek(&self, key: &K) -> Option<Arc<V>> {
        let guard = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .get(key)
            .filter(|(at, _)| at.elapsed() < self.ttl)
            .map(|(_, v)| Arc::clone(v))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn a_second_ask_for_the_same_key_within_the_ttl_does_not_recompute() {
        let cache: TtlCache<&str, u64> = TtlCache::new(Duration::from_secs(60));
        let runs = AtomicUsize::new(0);
        let first = cache
            .get_or_compute("a", || {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(7)
            })
            .unwrap();
        let second = cache
            .get_or_compute("a", || {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(9)
            })
            .unwrap();
        assert_eq!(*first, 7);
        assert_eq!(*second, 7, "the cached answer, not the new closure's");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn different_keys_do_not_evict_each_other_within_the_bound() {
        // The property this type exists for: two visitors alternating between
        // two different mints must both see hits, which a single-entry cache
        // cannot give them.
        let cache: TtlCache<&str, u64> = TtlCache::new(Duration::from_secs(60));
        let runs = AtomicUsize::new(0);
        let compute = |v: u64| {
            runs.fetch_add(1, Ordering::SeqCst);
            Ok::<_, ()>(v)
        };
        cache.get_or_compute("mint-a", || compute(1)).unwrap();
        cache.get_or_compute("mint-b", || compute(2)).unwrap();
        assert_eq!(*cache.get_or_compute("mint-a", || compute(99)).unwrap(), 1);
        assert_eq!(*cache.get_or_compute("mint-b", || compute(99)).unwrap(), 2);
        assert_eq!(runs.load(Ordering::SeqCst), 2, "each key computed once");
    }

    #[test]
    fn an_entry_past_its_ttl_is_recomputed() {
        let cache: TtlCache<&str, u64> = TtlCache::new(Duration::from_millis(10));
        let runs = AtomicUsize::new(0);
        cache
            .get_or_compute("a", || {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(1)
            })
            .unwrap();
        std::thread::sleep(Duration::from_millis(40));
        let after = cache
            .get_or_compute("a", || {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, ()>(2)
            })
            .unwrap();
        assert_eq!(*after, 2);
        assert_eq!(runs.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_failure_is_not_cached() {
        let cache: TtlCache<&str, u64> = TtlCache::new(Duration::from_secs(60));
        let failed: Result<Arc<u64>, &str> = cache.get_or_compute("a", || Err("cryptohouse down"));
        assert_eq!(failed.err(), Some("cryptohouse down"));
        let runs = AtomicUsize::new(0);
        let recovered = cache
            .get_or_compute("a", || {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<_, &str>(5)
            })
            .unwrap();
        assert_eq!(*recovered, 5, "the next attempt is allowed to succeed");
    }

    #[test]
    fn the_key_count_is_bounded() {
        let cache: TtlCache<u32, u32> = TtlCache::new(Duration::from_secs(60));
        for k in 0..(u32::try_from(MAX_ENTRIES).expect("MAX_ENTRIES fits a u32") + 10) {
            cache.get_or_compute(k, || Ok::<_, ()>(k)).unwrap();
        }
        let guard = cache.entries.lock().unwrap();
        assert!(guard.len() <= MAX_ENTRIES, "{}", guard.len());
    }
}
