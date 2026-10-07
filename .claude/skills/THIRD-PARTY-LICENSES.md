# Third-party skills

These skills were copied from other open-source repositories. Their license
texts are in `LICENSES/`. Each was read and checked before it was added: no
network calls, no hidden instructions, and the scripts only run locally.

| Skill folder | Source | Commit | License |
|---|---|---|---|
| `humanizer` | [blader/humanizer](https://github.com/blader/humanizer) | `225a6f3` | MIT (`LICENSES/humanizer-MIT.txt`) |
| `brainstorming`, `writing-plans`, `executing-plans`, `subagent-driven-development`, `test-driven-development`, `systematic-debugging`, `verification-before-completion`, `requesting-code-review`, `receiving-code-review`, `finishing-a-development-branch`, `using-git-worktrees` | [obra/superpowers](https://github.com/obra/superpowers) `skills/` | `8ca22db` | MIT (`LICENSES/superpowers-MIT.txt`) |
| `rust-skills` | [leonardomso/rust-skills](https://github.com/leonardomso/rust-skills) (`SKILL.md` and `rules/` only) | `fd2a861` | MIT (`LICENSES/rust-skills-MIT.txt`) |
| `rust-engineer`, `cli-developer`, `code-reviewer`, `architecture-designer`, `secure-code-guardian` | [Jeffallan/claude-skills](https://github.com/Jeffallan/claude-skills) `skills/` | `1be15d8` | MIT (`LICENSES/claude-skills-MIT.txt`) |
| `testing-ratatui-tuis` | [fedexist/grafatui](https://github.com/fedexist/grafatui) `.agents/skills/` | `4754519` | Apache-2.0 (`LICENSES/grafatui-Apache-2.0.txt`) |

Changes made when copying:

- `testing-ratatui-tuis`: removed `agents/openai.yaml` (Codex-only metadata).
  No other file was modified.
- `humanizer`: copied `SKILL.md` only (plugin manifests, CI and the package
  validator were left out).
- All other folders are unmodified copies.

Superpowers skills refer to each other as `superpowers:<name>`. In this repo
that means the folder `.claude/skills/<name>`.

`ratatui-tui` is not third-party; it was written for Paddock.

To update a skill, copy the new upstream version over the folder, read the
diff, and change the commit in this table.
