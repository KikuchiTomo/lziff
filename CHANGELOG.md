# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

`dist` reads the entry that matches the tag being released and uses it as
the body of the GitHub Release notes. Stick to one section per version
under `## [vX.Y.Z] - YYYY-MM-DD`, with the standard subsection headings
(Added / Changed / Fixed / Deprecated / Removed / Security).

## [Unreleased]

## [v0.1.2] - 2026-05-09

### Added

- PR picker. Running `lziff --review` with no argument opens a small
  TUI that lists open PRs with the current user as a requested
  reviewer (`review-requested:@me`). Up/Down or j/k to move, Enter
  to open, Esc/q to cancel.
- Setup progress on stderr. `--review` now prints short status lines
  while resolving the backend, looking up the PR, fetching, setting
  up the worktree, and resolving the base SHA, so the tool no longer
  looks hung during the initial seconds.
- Drafts list modal. New `d` keybind shows every buffered draft
  comment in a list — Enter re-opens the comment modal pre-filled
  with the existing body (saving replaces in place), `x` / `Del`
  removes the draft, Esc closes.
- Draft visibility on the diff. Lines with a buffered draft show a
  `✱` glyph in the line-number gutter on the matching side, and the
  header row shows a `💬 N` count whenever the host is in review
  mode.
- Three-way semantic anchor coloring in the gutter ribbon. Pure
  delete segments now draw red dots on both panes (including the
  phantom row on the right pane), pure inserts draw green on both,
  and paired modifications draw yellow. New theme fields:
  `modify_anchor_bright` / `modify_anchor_dim`.
- `[` / `]` snap-step keybinds. They nudge the alignment indicator
  one row up or down on screen and resnap the non-anchor pane —
  distinct from j/k (which scrolls). Useful for fine-tuning the
  snap after a click.
- Review-mode help section. The `?` overlay now lists the
  comment / submit / drafts / verdict keys (and `[` / `]`) when the
  host is in `--review` mode.

### Changed

- Files panel ignores mouse wheel events. Wheel scrolling on long
  file lists felt sluggish and made it too easy to lose your place;
  click and j/k are the only ways to move the selection now.
- Comment modal no longer paints two nested frames with the same
  caption — the textarea's inner block is borderless.

### Fixed

- `--review <PR>` no longer fails with `Unknown JSON field:
  "baseRefOid"` on gh 2.x. The provider now requests only fields the
  CLI actually exposes; the host fetches the base branch from
  `origin` and resolves `base_sha` via `git merge-base`, matching
  GitHub's "Files changed" semantics.
- `--review` no longer hangs silently when `git fetch` or
  `git worktree add` need to prompt (SSH passphrase, credential
  helper, host-key confirmation). The network-touching git
  invocations inherit stdio so prompts and progress reach the
  terminal.
- `[` / `]` now actually move the alignment row instead of scrolling
  the anchor pane in lockstep with the other side.

## [v0.1.1] - 2026-05-09

### Fixed

- `--range <from>..<to>` now lists files correctly. The previous parser
  expected `<status>\t<path>` records, but `git diff --name-status -z`
  separates the status and path with a NUL byte, which produced an empty
  file list (and thus a blank diff view). The parser now reads paired
  NUL-separated fields and tolerates rename/copy three-field records.
- Files that contain hard tabs (Makefiles, Go sources, …) no longer
  corrupt the TUI layout. Tab characters were being passed through to
  the terminal verbatim, where they advanced the cursor to the next
  hardware tab stop and trampled neighboring panes. Tabs are now
  expanded to spaces against a 4-column tab stop relative to the start
  of each rendered row, and other control characters are replaced with
  a single space.

## [v0.1.0-rc.1] - 2026-05-09

First release candidate. Shakedown for the cargo-dist pipeline; the
feature set targeting `v0.1.0` is otherwise complete.

### Added

- Snap-aligned side-by-side diff viewer with click-to-snap selection,
  scroll-pause at change-block boundaries, and a 5-column gutter ribbon
  that connects equal-anchor pairs across the panes.
- Phantom-row injection on pure delete/insert blocks so every change
  segment has a snap target on both panes.
- Conflict-aware load: when a file is in a merge conflict the diff shows
  `ours` (`:2`) on the left and `theirs` (`:3`) on the right; the file
  panel marker is colored via `theme.status_conflict_fg`.
- Git CLI modes: `--staged` (alias `--cached`), `--commit <rev>`,
  `--range <from>..<to>`, plus the existing arbitrary file-pair form.
- File panel: bracketed `[Files]` title with entry count, status-colored
  `●` markers, `Shift-F` show/hide toggle, and a slimmer 20% default
  width.
- Help dialog: structured `[Help]` overlay with sectioned key tables
  (Navigation / Alignment / Display / App), themed colors, and an
  optional Japanese localization.
- Configuration via `~/.config/lziff/config.toml`: full theme, keymap,
  layout, behavior, and i18n overlays. Missing or partial configs fall
  back to defaults.
- Live auto-reload through mtime polling (250 ms tick by default) for
  working-tree and staged sources.
