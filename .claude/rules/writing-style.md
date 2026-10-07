# Writing style (all prose)

Applies to README, docs, CHANGELOG, commit messages, PR text, code comments,
CLI `--help` text, error messages and every string shown in the TUI.

## Hard rules

- **No em dashes (U+2014) and no en dashes (U+2013).** Use a full stop, a
  comma, a colon, parentheses or "to" for ranges (`v0.1 to v0.3`). A hook
  flags them on every edit. Plain hyphens in compound words are fine.
- No emoji in code, comments, commit messages or docs. Status glyphs used by
  the TUI itself (`●`, `○`, `✗`) are UI, not decoration.
- No stock AI words: delve, leverage, seamless, robust, powerful, elevate,
  streamline, crucial, comprehensive, cutting-edge, game-changer, unlock,
  empower, effortless, "in today's fast-paced world".
- No staged phrasing: "It's not just X, it's Y", "Here's the thing:",
  "The result?", one-line dramatic closers, triads added for rhythm.
- No bold lead-ins on every bullet. Bold is for the one thing a skimmer must
  not miss.

## How to write

- Short sentences. Plain words. Say what the thing does, then stop.
- Specific beats general: "SIGTERM, then SIGKILL after 5 s" beats "stops it
  gracefully".
- Second person for docs ("you"), imperative for commit subjects.
- Error messages say what happened and what to do:
  `port 3000 is used by pid 812 (node, not started by Paddock). Press k to kill it.`

## Before publishing prose

For README, docs pages, release notes and the website, run the `humanizer`
skill over the text and fix what it finds. Then check for dashes in bash:
`grep -n $'\u2014\\|\u2013' FILE` must print nothing.
