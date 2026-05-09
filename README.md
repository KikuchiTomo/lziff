# lziff

[![Release CI](https://github.com/KikuchiTomo/lziff/actions/workflows/release.yml/badge.svg)](https://github.com/KikuchiTomo/lziff/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/KikuchiTomo/lziff?logo=github&color=blue)](https://github.com/KikuchiTomo/lziff/releases/latest)
[![Homebrew tap](https://img.shields.io/badge/homebrew-KikuchiTomo%2Ftap-orange?logo=homebrew)](https://github.com/KikuchiTomo/homebrew-tap)
[![License: MIT](https://img.shields.io/github/license/KikuchiTomo/lziff?color=informational)](./LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey)](https://github.com/KikuchiTomo/lziff/releases/latest)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-dea584?logo=rust)](https://www.rust-lang.org/)

A cross-platform terminal diff & review tool with JetBrains-style
snap-aligned side-by-side diffs.

<!-- screenshot here -->

## What it does

- **Side-by-side diff** of two files or any pair of git states.
- **Snap alignment**: click any line and the other pane scrolls until
  the corresponding line lands on the same row. Equal anchors stay
  visually connected by `●─●` ribbons in the gutter.
- **No filler rows**: each pane shows its file as-is. When one side
  has more change than the other, scrolling automatically pauses the
  shorter side at change-block boundaries so the panes re-sync at the
  next equal anchor.
- **Word-level highlights** inside modified lines, with a similarity
  threshold so unrelated pair-ups don't turn into red/green noise.
- **Auto-reload** while you edit — saves outside the tool show up
  within a couple hundred milliseconds.
- **Mouse + keyboard**: click to snap, wheel to scroll the pane under
  the pointer, vim-style keys for everything else.

## Install

```sh
# From source (Rust 1.74+):
cargo install --git https://github.com/KikuchiTomo/lziff
```

## Usage

### Git workflows

```sh
lziff                          # working tree vs HEAD/index (default)
lziff --staged                 # staged changes (index vs HEAD; alias: --cached)
lziff --commit HEAD            # what HEAD changed (HEAD~ vs HEAD)
lziff --commit abc123          # what an arbitrary commit changed
lziff --range main..feature    # what `feature` added on top of `main`
```

Run lziff anywhere inside a git repository and it picks up the repo
automatically. The Files panel on the left lists every changed file;
pick one with `j`/`k` (or click), then explore the diff on the right.

### Arbitrary file pair

```sh
lziff a.txt b.txt              # diff any two files, no git required
```

The Files panel hides itself in this mode so the diff fills the screen.

### Live editing loop

Run `lziff` in one terminal and edit in another — saves are picked up
automatically and the diff redraws in place. Useful when you want to
review your in-progress changes without alt-tabbing through `git diff`.

## Keys

| Key            | Action                                         |
| -------------- | ---------------------------------------------- |
| `j` / `k`      | scroll both panes 1:1                          |
| `J` / `K`      | jump to next / previous change hunk            |
| `Ctrl-d/u`     | half-page down / up                            |
| `g` / `G`      | top / bottom                                   |
| `n` / `p`      | next / previous file                           |
| `click`        | select that line; the other pane snaps to it   |
| `wheel`        | scroll the pane under the pointer              |
| `=`            | re-snap the non-anchor pane to the cursor row  |
| `Tab`          | toggle focus (Files / Diff)                    |
| `F`            | show / hide the Files panel                    |
| `r`            | manual reload                                  |
| `?`            | open this help                                 |
| `q`            | quit                                           |

## Configuration

lziff reads `$XDG_CONFIG_HOME/lziff/config.toml` (or the platform
equivalent — e.g. `~/Library/Application Support/lziff/config.toml`
on macOS). The file is optional; missing keys fall back to sensible
defaults.

```toml
[i18n]
lang = "ja"                    # "en" or "ja" — defaults to $LANG

[layout]
files_panel_width_pct = 20     # % of horizontal space for the Files list
ribbon_width = 5               # 5-char gutter between the two diff panes
target_y_divisor = 3           # alignment row sits at viewport_h / N

[behavior]
tick_ms = 250                  # auto-reload poll interval
scroll_step = 3                # mouse-wheel step
half_page_step = 15            # ctrl-d / ctrl-u step
similarity_threshold = 0.30    # below this, paired lines drop word-diff

# Rebind any of the 19 actions. Override accepts a single string or an
# array. Modifiers: ctrl-, shift-, alt-. Named keys: Up, Down, Left,
# Right, Enter, Esc, Tab, F1..F12, PageUp, PageDown.
[keymap]
quit         = ["q", "Esc"]
scroll_down  = ["j", "Down"]
scroll_up    = ["k", "Up"]
next_hunk    = "J"
prev_hunk    = "K"
half_page_down = "ctrl-d"
half_page_up   = "ctrl-u"
toggle_help    = "?"
toggle_files_panel = "F"
resnap         = "="

# Every visual color is themable via #rrggbb. Below is just a sample;
# all theme keys are documented in the config module.
[theme]
bg_delete = "#301618"
bg_insert = "#142c1a"
hl_delete = "#963c46"
hl_insert = "#328246"
anchor_bright = "#f5f5fa"
help_section_fg = "#e1c882"
```

## Status

Early days — the diff engine and snap UX are stable, but the plugin
boundary (for hosting non-git sources) is still in design. Issues and
suggestions welcome on the issue tracker.
