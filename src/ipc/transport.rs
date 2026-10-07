//! Moves `Request`s and `Event`s between the two sides.
//!
//! v0.1: a pair of tokio mpsc channels. v0.2: a Unix socket with one JSON
//! message per line. Output events are batched so a chatty process cannot
//! flood the TUI (at most ~30 output messages per second per process).
//!
//! Unix sockets only. No TCP, ever: the supply chain scan rejects it.
