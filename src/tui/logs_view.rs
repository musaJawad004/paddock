//! Main pane: the selected process's screen from its `vt100` parser, or its
//! scrollback when scrolled up. Search highlights matches and jumps between
//! them. Error lines get the theme's error style.
//!
//! Attach mode forwards keystrokes to the process (for prompts like Expo's
//! "press i for iOS") until the detach key.
