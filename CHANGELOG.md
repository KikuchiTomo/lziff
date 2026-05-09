# Changelog

All notable changes to this project will be documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

`dist` reads the entry that matches the tag being released and uses it as
the body of the GitHub Release notes. Stick to one section per version
under `## [vX.Y.Z] - YYYY-MM-DD`, with the standard subsection headings
(Added / Changed / Fixed / Deprecated / Removed / Security).

## [Unreleased]

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
