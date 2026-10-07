//! Works out how to run a project by reading its files. Read-only: never
//! writes, never executes anything, never touches the network.
//!
//! `detect(dir) -> Vec<ProcessSpec>` asks each detector in turn and merges
//! the results. A detector that fails or finds nothing contributes nothing;
//! malformed project files are logged and skipped, never fatal.
//!
//! Detectors only see the project folder they are given.

pub mod compose;
pub mod node;
pub mod procfile;
pub mod rust;
