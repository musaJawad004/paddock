# Code style: write it like a careful human did

The goal is code a reviewer cannot tell was machine-written: no padding, no
narration, nothing "just in case".

## Comments

- Comment the why, never the what. If the comment restates the next line,
  delete it.
- No step comments (`// Step 1: parse config`), no section banners, no
  comments addressed to the reader ("Note that...", "Here we...").
- Every module starts with a short `//!` saying what it owns and what it must
  not do. Keep it current when behaviour changes.
- Doc comments (`///`) on public items only, one or two lines unless the
  behaviour is subtle.
- No TODOs without an issue number. No commented-out code.

## Structure

- Do the simple thing. No traits, generics, builders or config options until
  a second real caller needs them.
- No wrapper functions that only forward their arguments.
- No defensive checks for states the types already rule out.
- Names come from the domain: `Project`, `Process`, `Port`, `Supervisor`,
  `LogBuffer`. Avoid `Manager`, `Handler`, `Helper`, `Util`, `Data`, `Info`.
- Functions fit on one screen. Split by meaning, not by line count.
- Match the surrounding code. Consistency wins over personal preference.

## Changes

- Touch only what the task needs. No drive-by reformatting or renames in
  the same commit.
- Delete code that a change makes unused.
- Before finishing, reread the diff once as a reviewer and cut anything that
  does not earn its place.
