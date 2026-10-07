//! `Procfile` (one `name: command` per line, as used by foreman and
//! overmind) and `Makefile` targets named `dev`, `run`, `serve` or `start`.
//!
//! A Procfile is taken as the user's explicit intent, so when one exists it
//! replaces processes detected from package.json for the same project.
