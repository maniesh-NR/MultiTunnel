//! Shared shutdown primitive for background tasks.
//!
//! CONTRACT (harness/CONTRACTS-S6.md "Expert B"): the server runs until the
//! `watch::Receiver<bool>` flips true; the library never creates its own
//! runtime and never owns the shutdown sender.
//!
//! IMPLEMENTATION NOTES (S6-B): a dropped sender is treated as shutdown so
//! no background task (prober, ticker) can outlive the server owner; the
//! initial borrow is re-checked first so a pre-flipped channel shuts down
//! immediately.

use tokio::sync::watch;

/// Resolve when `*rx` becomes true or the sender is dropped.
/// Cancel-safe to poll inside `select!`.
pub(crate) async fn wait_for_shutdown(rx: &mut watch::Receiver<bool>) {
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            // Sender dropped without flipping: treat as shutdown.
            return;
        }
    }
}
