// SPDX-License-Identifier: Apache-2.0
//! One background read, broadcast to every subscriber.
//!
//! Before this existed, `/v1/events` and `/v1/customer/events` each drove
//! their own polling loop: every open connection called into the store on its
//! own ten-second timer, so store reads grew with the number of open tabs
//! rather than staying flat. A [`Ticker`] is the fix -- exactly one task calls
//! the read on an interval and [`Ticker::publish`]es the result here; every
//! stream [`Ticker::subscribe`]s and reads only this channel, never the store.
//!
//! # Why `watch`, not `broadcast`
//!
//! A subscriber only ever wants the *current* value, not a history it missed
//! while disconnected, and a subscriber that joins mid-quiet-minute must see
//! that current value immediately -- without waiting for the next tick and
//! without causing a read of its own. [`tokio::sync::watch`] gives both for
//! free: [`Ticker::subscribe`] returns a receiver already holding whatever was
//! last published. `broadcast`'s bounded history would either drop a slow
//! subscriber's backlog or force a channel capacity nothing here needs, for a
//! problem (delivering every intermediate value) that no caller of this has.

use tokio::sync::watch;

/// The single background-polled value every subscriber reads instead of the
/// store.
///
/// Holds `None` until the first successful read publishes something; see
/// [`Ticker::publish`] for why a failed read publishes nothing at all.
pub struct Ticker<T> {
    tx: watch::Sender<Option<T>>,
}

impl<T> Default for Ticker<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Ticker<T> {
    /// A ticker with nothing published yet.
    #[must_use]
    pub fn new() -> Self {
        let (tx, _rx) = watch::channel(None);
        Self { tx }
    }

    /// A receiver that already holds the current value, if one has been
    /// published -- so a subscriber that joins during a quiet minute sees it
    /// immediately rather than waiting for the next tick, and without causing
    /// a read of its own. Subscribing never touches whatever produces `T`.
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<Option<T>> {
        self.tx.subscribe()
    }
}

impl<T: PartialEq> Ticker<T> {
    /// Publishes a freshly read value, if it differs from what is already
    /// here.
    ///
    /// A no-op on an unchanged value rather than an unconditional send: every
    /// subscriber's stream wakes on a change, and waking one for a value
    /// identical to what it already has would be indistinguishable from a
    /// change that never happened -- the exact bug the old per-connection
    /// `last.as_ref() != Some(&tick)` check existed to prevent, moved here so
    /// it only needs doing once per tick rather than once per connection.
    pub fn publish(&self, value: T) {
        self.tx.send_if_modified(|current| {
            if current.as_ref() == Some(&value) {
                false
            } else {
                *current = Some(value);
                true
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::Ticker;

    /// Counts how often the expensive read actually ran. Same shape as
    /// `cache::tests::counting`, applied to subscriber count instead of
    /// watermark advances.
    fn counting(count: &AtomicUsize, value: u64) -> impl Fn() -> u64 + '_ {
        move || {
            count.fetch_add(1, Ordering::SeqCst);
            value
        }
    }

    #[test]
    fn a_late_subscriber_sees_the_current_value_without_a_read_of_its_own() {
        let ticker = Ticker::new();
        let reads = AtomicUsize::new(0);
        ticker.publish(counting(&reads, 7)());

        // Subscribing is the "late join" this stands in for: nothing here
        // calls `counting` again.
        let mut rx = ticker.subscribe();
        assert_eq!(*rx.borrow_and_update(), Some(7));
        assert_eq!(reads.load(Ordering::SeqCst), 1, "subscribing did not read");
    }

    #[test]
    fn a_subscriber_before_the_first_publish_sees_nothing_yet_rather_than_stalling() {
        let ticker: Ticker<u64> = Ticker::new();
        let mut rx = ticker.subscribe();
        assert_eq!(*rx.borrow_and_update(), None);
    }

    #[test]
    fn publishing_the_same_value_again_does_not_mark_a_subscriber_changed() {
        let ticker = Ticker::new();
        ticker.publish(1u64);
        let mut rx = ticker.subscribe();
        rx.borrow_and_update();

        ticker.publish(1u64);
        assert!(
            !rx.has_changed().expect("sender still alive"),
            "the same value again must not wake a parked subscriber"
        );

        ticker.publish(2u64);
        assert!(
            rx.has_changed().expect("sender still alive"),
            "a genuinely different value must wake it"
        );
    }

    #[test]
    fn reads_per_publish_stay_flat_from_one_subscriber_to_fifty() {
        // The whole point of this type. Opening forty-nine more connections
        // must not cost forty-nine more reads: subscribers read the channel,
        // never whatever `counting` stands in for here.
        let ticker: Ticker<u64> = Ticker::new();
        let reads = AtomicUsize::new(0);

        let _one = ticker.subscribe();
        ticker.publish(counting(&reads, 1)());
        let reads_with_one_subscriber = reads.load(Ordering::SeqCst);
        assert_eq!(reads_with_one_subscriber, 1);

        let _fifty: Vec<_> = (0..50).map(|_| ticker.subscribe()).collect();
        ticker.publish(counting(&reads, 2)());
        let reads_after_fifty_subscribers =
            reads.load(Ordering::SeqCst) - reads_with_one_subscriber;

        assert_eq!(
            reads_after_fifty_subscribers, 1,
            "one publish costs one read no matter how many subscribers are attached"
        );
    }
}
