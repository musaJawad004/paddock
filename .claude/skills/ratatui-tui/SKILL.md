---
name: ratatui-tui
description: How Paddock builds its terminal UI with ratatui 0.30 and crossterm. Use before writing or changing anything under src/tui/, adding a screen, pane, popup or keybinding, wiring the event loop, or fixing a drawing, resize or input bug.
---

# Building Paddock's TUI with ratatui

Ratatui is immediate mode: every frame, the whole screen is drawn again from
state. There are no retained widgets to update. Keep that in mind and most
design questions answer themselves.

Versions: ratatui 0.30.x, crossterm through `ratatui::crossterm` (do not add a
separate crossterm dependency with a different version). Check `Cargo.toml`
before relying on an API from memory, and read docs.rs for the exact version
when unsure.

## Shape of the app

Three parts, kept apart:

1. **State** (`tui/app.rs`): plain structs. What is selected, which pane has
   focus, the latest snapshot from the monitor, open popup. No terminal
   types in here.
2. **Update**: `fn update(&mut self, msg: Msg) -> Option<Effect>`. Turns key
   presses, ticks and monitor events into state changes and effects (send a
   request, copy, open a URL, save the config). Pure enough to unit test
   without a terminal.
3. **View**: one module per area (`sidebar.rs`, `details.rs`,
   `status_bar.rs`, `help.rs`). Each implements `Widget for &T` or
   `StatefulWidget`. Views read state and never change it.

```rust
pub enum Msg {
    Key(KeyEvent),
    Resize,
    Tick,
    Monitor(Event), // a new snapshot, or a notice
}
```

## Terminal setup

Use `ratatui::init()` and `ratatui::restore()`, or `ratatui::run(|terminal| ...)`.
`init` enables raw mode, enters the alternate screen and installs a panic
hook that restores the terminal, so a crash never leaves the user's shell
broken. Do not hand-roll `enable_raw_mode` + `EnterAlternateScreen`.

```rust
fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new()?.run(terminal);
    ratatui::restore();
    result
}
```

Mouse capture is opt-in (`EnableMouseCapture`) and must be disabled on exit.

## Event loop

Paddock has three input sources: the keyboard, a redraw tick, and events
from the monitor. Use tokio and `select!` over them so a slow scan never
blocks a key press. Crossterm's async stream needs the
`event-stream` feature.

```rust
loop {
    if self.dirty {
        terminal.draw(|frame| frame.render_widget(&self, frame.area()))?;
        self.dirty = false;
    }
    tokio::select! {
        Some(Ok(ev)) = events.next() => self.on_terminal_event(ev),
        Some(ev) = monitor_rx.recv() => self.on_monitor_event(ev),
        _ = tick.tick() => self.on_tick(),
    }
    if self.should_quit { break; }
}
```

Rules for the loop:

- Only redraw when something changed (`dirty` flag), at most ~30 times a
  second.
- Handle keys only when `key.kind == KeyEventKind::Press`. Windows and some
  terminals also send Release and Repeat.
- Nothing slow inside `draw`. No file reads, no process-table reads, no
  locks held long.
- `Event::Resize` just marks dirty; the next draw uses the new `frame.area()`.

## Layout

```rust
let [sidebar, main] = Layout::horizontal([Constraint::Length(28), Constraint::Fill(1)])
    .areas(area);
let [logs, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
    .areas(main);
```

- Prefer `.areas()` with array destructuring over indexing `.split()` results.
- `Constraint::Fill` for flexible space, `Length` for fixed bars, `Min` only
  when something must not shrink below a size.
- Handle tiny terminals: if the area is too small, draw a one-line
  "terminal too small" message instead of panicking or overlapping.
- Popups: compute a centered `Rect`, render `Clear` first, then the popup.

## Widgets

- `Block::bordered().title(...)` for panes. The focused pane gets the accent
  border colour; others stay dim. Do not use more than one accent colour.
- `List` with `ListState` for the sidebar and the ports pane. Selection is
  kept by `ServerId`, not by row, so it survives a new snapshot.
- Text: `Line`, `Span`, `Text`. Use `Stylize` shorthands (`"x".bold().cyan()`).
- Truncate with `unicode-width` (`tui::fit`), never by byte or char count;
  names and paths contain wide characters.
- Write a custom widget when a pane needs drawing logic of its own. Put it in
  its own file with a short `//!` comment saying what it shows.

## Theme

- All colours come from `tui/theme.rs`. No `Color::Rgb(...)` literals in view
  code.
- Respect `NO_COLOR`: if it is set, use only default fg/bg plus bold and
  reverse for emphasis.
- Status colours are fixed and meaningful: running green, stopping yellow,
  foreign ports yellow, errors red. Never use red for anything that is not a
  failure.

## Keys

- Keys live in one keymap (`tui/keys.rs`) that the handler, the status bar,
  the help popup and the settings popup all read, so help cannot drift from
  behaviour. Users rebind keys in settings or config.toml.
- Defaults: arrows and `j/k` move, `tab` switches panes, `o` open, `y`/`Y`
  copy URL/command, `x` stop, `K` kill, `,` settings, `?` help, `q` quit.
  Anything that stops a process asks first.
- `Ctrl+C` always quits; `Esc` and `Enter` cannot be rebound.

## Testing

- Unit test update logic directly: build `App`, send `Msg`s, assert state.
- Render tests use `TestBackend` with a fixed size and compare the buffer
  (`insta` snapshots). Use fixed fixture data, never real processes or the
  real clock (fixture servers start "in the future" so uptime shows 0s).
- For full-terminal checks (startup, quit, resize, restoring the shell), use
  the `testing-ratatui-tuis` skill and its `pty-smoke` script.

## Before calling a TUI change done

1. `cargo clippy --all-targets -- -D warnings` and `cargo test` pass.
2. Run `cargo run` in a real terminal and try the change, including a narrow
   window (80x24) and resizing while it runs.
3. Quit with `q` and with a panic path if you touched setup: the shell must
   come back clean (cursor visible, echo on, no alternate screen left over).
