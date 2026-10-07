//! Moves `Request`s and `Event`s between the two sides.
//!
//! Today: a pair of bounded tokio channels inside one process. v0.2 adds a
//! Unix socket with one JSON message per line behind the same two ends, so
//! neither side knows which transport is in use.
//!
//! Unix sockets only. No TCP, ever: the supply chain scan rejects it.

use tokio::sync::mpsc;

use super::protocol::{Event, Request};

/// Enough headroom for a burst of output; a full channel slows the sender
/// down instead of growing memory.
const CAPACITY: usize = 256;

/// The TUI's end: send requests, receive events.
pub struct ClientEnd {
    pub requests: mpsc::Sender<Request>,
    pub events: mpsc::Receiver<Event>,
}

/// The process owner's end: receive requests, send events.
pub struct ServerEnd {
    pub requests: mpsc::Receiver<Request>,
    pub events: mpsc::Sender<Event>,
}

pub fn in_process() -> (ClientEnd, ServerEnd) {
    let (request_tx, request_rx) = mpsc::channel(CAPACITY);
    let (event_tx, event_rx) = mpsc::channel(CAPACITY);
    (
        ClientEnd {
            requests: request_tx,
            events: event_rx,
        },
        ServerEnd {
            requests: request_rx,
            events: event_tx,
        },
    )
}
