//! Every colour and style the TUI uses. View code asks the theme, never
//! builds a `Color` itself.
//!
//! Status colours are fixed: running green, starting yellow, crashed red,
//! stopped dim. One accent colour for focus. With `NO_COLOR` set, only bold,
//! dim and reverse are used.
