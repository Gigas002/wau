//! Progress reporting pub/sub — ports instawow's `progress_reporting.py`.
//!
//! Python threads this through `contextvars` (implicit global state scoped
//! per async task tree, since arbitrary code anywhere can call the
//! module-level `update_progress`/`make_progress_receiver` functions); this
//! port uses an explicit [`ProgressBus`] handle instead, matching the
//! project's "explicit struct threaded through calls, not global state"
//! architecture. Same observable behaviour (every subscriber sees every
//! update), no hidden global state.
//!
//! Not yet wired into `pkg_management`'s resolve/download loops — that
//! instrumentation lands with the CLI's `indicatif`-based renderer, so the
//! integration points can be chosen alongside real rendering rather than
//! speculatively.

use std::{
    collections::HashMap,
    future::Future,
    sync::atomic::{AtomicU64, Ordering},
};

use tokio::sync::watch;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Generic,
    Download,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    pub kind: ProgressKind,
    pub label: String,
    pub current: u64,
    pub total: Option<u64>,
}

/// Every in-flight progress entry, keyed by an opaque id.
pub type ProgressGroup = HashMap<u64, Progress>;

/// A shared progress bus: producers [`ProgressBus::update`], observers
/// [`ProgressBus::subscribe`] or take a one-off [`ProgressBus::snapshot`].
pub struct ProgressBus {
    ids: AtomicU64,
    tx: watch::Sender<ProgressGroup>,
}

impl Default for ProgressBus {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressBus {
    pub fn new() -> Self {
        let (tx, _) = watch::channel(HashMap::new());
        Self {
            ids: AtomicU64::new(0),
            tx,
        }
    }

    /// A fresh, process-unique progress entry id.
    pub fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }

    /// Sets (`Some`) or clears (`None`) the entry at `id` and wakes subscribers.
    pub fn update(&self, id: u64, progress: Option<Progress>) {
        self.tx.send_modify(|group| match progress {
            Some(p) => {
                group.insert(id, p);
            }
            None => {
                group.remove(&id);
            }
        });
    }

    /// Subscribes to future updates (the receiver also sees the current state immediately).
    pub fn subscribe(&self) -> watch::Receiver<ProgressGroup> {
        self.tx.subscribe()
    }

    /// The current state without subscribing.
    pub fn snapshot(&self) -> ProgressGroup {
        self.tx.borrow().clone()
    }
}

/// Tracks a fixed-size batch of futures as one "N of M done" entry,
/// clearing it once every future completes — instawow's
/// `make_incrementing_progress_tracker`.
pub struct IncrementingTracker<'a> {
    bus: &'a ProgressBus,
    id: u64,
    label: String,
    total: u64,
    done: AtomicU64,
}

impl<'a> IncrementingTracker<'a> {
    /// Returns `None` for an empty batch (nothing to track), matching
    /// instawow's `total < 1` short-circuit.
    pub fn new(bus: &'a ProgressBus, total: u64, label: impl Into<String>) -> Option<Self> {
        if total < 1 {
            return None;
        }
        let id = bus.next_id();
        let label = label.into();
        bus.update(
            id,
            Some(Progress {
                kind: ProgressKind::Generic,
                label: label.clone(),
                current: 0,
                total: Some(total),
            }),
        );
        Some(Self {
            bus,
            id,
            label,
            total,
            done: AtomicU64::new(0),
        })
    }

    /// Awaits `fut`, then advances the shared counter (clearing the entry
    /// once the batch is complete).
    pub async fn track<F: Future>(&self, fut: F) -> F::Output {
        let result = fut.await;
        let done = self.done.fetch_add(1, Ordering::SeqCst) + 1;
        if done >= self.total {
            self.bus.update(self.id, None);
        } else {
            self.bus.update(
                self.id,
                Some(Progress {
                    kind: ProgressKind::Generic,
                    label: self.label.clone(),
                    current: done,
                    total: Some(self.total),
                }),
            );
        }
        result
    }
}
