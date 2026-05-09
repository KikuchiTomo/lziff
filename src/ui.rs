use crate::app::{AnchorSide, App, Focus, LayoutCache};
use crate::config::Theme;
use crate::diff::{LineKind, PaneLine, Segment, SegmentKind};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(f, chunks[0], app);

    let mut layout = LayoutCache::default();
    if app.show_files_panel {
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(app.config.layout.files_panel_width_pct),
                Constraint::Min(20),
            ])
            .split(chunks[1]);
        render_files(f, body[0], app, &mut layout);
        render_diff(f, body[1], app, &mut layout);
    } else {
        render_diff(f, chunks[1], app, &mut layout);
    }
    app.layout = layout;

    render_status(f, chunks[2], app);

    if app.show_help {
        render_help(f, area, app);
    }
}

fn render_title(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.config.theme;
    let (l, r) = app.header_label();
    let (a, d, m) = app.change_summary();
    let title = Line::from(vec![
        Span::styled(
            " lziff ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(theme.title_bg)
                .fg(theme.title_fg),
        ),
        Span::raw(" "),
        Span::styled(l, Style::default().fg(theme.title_old)),
        Span::raw("  ↔  "),
        Span::styled(r, Style::default().fg(theme.title_new)),
        Span::raw("   "),
        Span::styled(format!("+{a}"), Style::default().fg(Color::Green)),
        Span::raw(" "),
        Span::styled(format!("-{d}"), Style::default().fg(Color::Red)),
        Span::raw(" "),
        Span::styled(format!("~{m}"), Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

fn render_files(f: &mut Frame, area: Rect, app: &App, layout: &mut LayoutCache) {
    let theme = &app.config.theme;
    let focused = matches!(app.focus, Focus::Files);
    // Title shows entry count when there's something to show; otherwise just
    // the bracketed label so the panel still reads cleanly when empty.
    let title = if app.entries.is_empty() {
        app.strings.title_files_panel.clone()
    } else {
        format!(
            "{} {} ",
            app.strings.title_files_panel.trim_end(),
            app.entries.len()
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style(theme, focused));
    layout.files_inner = block.inner(area);

    // Each row: " ● filename" — `●` is colored by status (M/A/D/?/etc.),
    // filename in default fg, both bold so the panel reads compactly.
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|e| {
            let color = status_color(theme, &e.status);
            let mark_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
            let name_style = Style::default().add_modifier(Modifier::BOLD);
            ListItem::new(Line::from(vec![
                Span::raw(" "),
                Span::styled("●", mark_style),
                Span::raw(" "),
                Span::styled(e.display.clone(), name_style),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(if focused {
                    theme.list_focus_bg
                } else {
                    theme.list_dim_bg
                })
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▎");

    let mut state = ListState::default();
    if !app.entries.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn render_diff(f: &mut Frame, area: Rect, app: &App, layout: &mut LayoutCache) {
    let theme = &app.config.theme;
    let focused = matches!(app.focus, Focus::Diff);

    let (l_label, r_label) = app.header_label();
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(app.config.layout.ribbon_width),
            Constraint::Min(10),
        ])
        .split(area);

    let l_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{}{}",
            app.strings.title_old_prefix,
            short_label(&l_label)
        ))
        .border_style(side_border_style(theme, true, focused));
    let r_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            "{}{}",
            app.strings.title_new_prefix,
            short_label(&r_label)
        ))
        .border_style(side_border_style(theme, false, focused));

    let l_inner = l_block.inner(cols[0]);
    let r_inner = r_block.inner(cols[2]);
    let ribbon_area = Rect {
        x: cols[1].x,
        y: l_inner.y,
        width: cols[1].width,
        height: l_inner.height,
    };
    layout.diff_left_inner = l_inner;
    layout.diff_right_inner = r_inner;

    f.render_widget(l_block, cols[0]);
    f.render_widget(r_block, cols[2]);

    let viewport_h = l_inner.height.min(r_inner.height) as usize;
    layout.viewport_h = viewport_h as u16;
    if viewport_h == 0 {
        return;
    }

    let cursor_y = app.cursor_y.min(viewport_h.saturating_sub(1));
    let lineno_w_l = lineno_width(&app.diff.left);
    let lineno_w_r = lineno_width(&app.diff.right);

    let l_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(lineno_w_l as u16), Constraint::Min(1)])
        .split(l_inner);
    let r_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(lineno_w_r as u16), Constraint::Min(1)])
        .split(r_inner);

    let mut l_no_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut l_text_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut r_no_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut r_text_lines: Vec<Line> = Vec::with_capacity(viewport_h);

    let l_text_w = l_cols[1].width as usize;
    let r_text_w = r_cols[1].width as usize;

    for y in 0..viewport_h {
        let li_render = app.left_top + y;
        let ri_render = app.right_top + y;
        let l_line = app
            .diff
            .left_render
            .get(li_render)
            .and_then(|opt| opt.as_ref().and_then(|&i| app.diff.left.get(i)));
        let r_line = app
            .diff
            .right_render
            .get(ri_render)
            .and_then(|opt| opt.as_ref().and_then(|&i| app.diff.right.get(i)));
        let is_alignment = y == cursor_y;

        let (lno, ltxt) = render_pane_row(theme, l_line, true, is_alignment, l_text_w, lineno_w_l);
        let (rno, rtxt) = render_pane_row(theme, r_line, false, is_alignment, r_text_w, lineno_w_r);
        l_no_lines.push(lno);
        l_text_lines.push(ltxt);
        r_no_lines.push(rno);
        r_text_lines.push(rtxt);
    }

    let ribbon_lines = build_ribbon_grid(app, viewport_h, cursor_y);

    f.render_widget(Paragraph::new(l_no_lines), l_cols[0]);
    f.render_widget(Paragraph::new(l_text_lines), l_cols[1]);
    f.render_widget(Paragraph::new(ribbon_lines), ribbon_area);
    f.render_widget(Paragraph::new(r_no_lines), r_cols[0]);
    f.render_widget(Paragraph::new(r_text_lines), r_cols[1]);
}

/// Compose the 5-column ribbon column. We fill a (viewport_h × 5) char grid
/// with cell-by-cell logic:
///
/// - Each pane gets a continuous vertical "track" (`│`) on rows where its
///   side is in a Modified or Standalone change. Tracks are tinted red on
///   the left, green on the right.
/// - Each *segment boundary* — start of an Equal block, start of a Change
///   block — places anchors on both sides. If the two anchors land on the
///   same row, draw a horizontal connector `○────○`. If they land on
///   different rows (which happens at the trailing edge of a change block
///   when one side has more content than the other), trace an L-shaped
///   path through the middle column: `○─┐ … └─○`.
/// - Anchors that sit on the cursor row are drawn filled (`●`) and bright
///   white. Others are hollow (`○`) and a softer white.
fn build_ribbon_grid(app: &App, viewport_h: usize, cursor_y: usize) -> Vec<Line<'static>> {
    let theme = &app.config.theme;
    // Internal grid is fixed at 5 columns: anchor / horizontal / vertical /
    // horizontal / anchor. `LayoutConfig::ribbon_width` controls the *cell*
    // width on screen — extra width pads, smaller width crops.
    const COLS_N: usize = 5;
    let mut grid: Vec<Vec<(char, Color, Color)>> =
        vec![vec![(' ', Color::Reset, Color::Reset); COLS_N]; viewport_h];

    // Tracks for change rows: we *just* tint the background of cols 0 and 4
    // so the side bars read as continuous colored stripes. Anchors land on
    // top of the tint, never against a `│` glyph.
    for (y, row) in grid.iter_mut().enumerate().take(viewport_h) {
        let l_change = matches!(
            app.diff.left_render.get(app.left_top + y),
            Some(Some(idx)) if app.diff.left.get(*idx)
                .is_some_and(|l| matches!(l.kind, LineKind::Modified | LineKind::Standalone))
        );
        let r_change = matches!(
            app.diff.right_render.get(app.right_top + y),
            Some(Some(idx)) if app.diff.right.get(*idx)
                .is_some_and(|l| matches!(l.kind, LineKind::Modified | LineKind::Standalone))
        );
        // Phantom rows count as the change kind too — the side that lacks
        // content still shows the colored band so the reader sees "this
        // segment took space, just on the other side".
        let l_phantom = matches!(app.diff.left_render.get(app.left_top + y), Some(None));
        let r_phantom = matches!(app.diff.right_render.get(app.right_top + y), Some(None));
        if l_change || l_phantom {
            row[0].2 = theme.track_bg_left;
        }
        if r_change || r_phantom {
            row[4].2 = theme.track_bg_right;
        }
    }

    // Plan which connector lines actually get drawn. Many segments can have
    // anchors visible at once, and their L-shaped paths share the middle
    // column — drawing all of them at once produces a tangled mess. So we
    // greedily hide the most-conflicting path, repeatedly, until what's left
    // doesn't crash. The user-selected segment (the one containing the
    // alignment-row cursor) is *always* kept visible so clicking moves the
    // visualization to the segment of interest.
    let in_view = |y: isize| y >= 0 && y < viewport_h as isize;
    let mut paths: Vec<PathPlan> = Vec::new();
    for (idx, seg) in app.diff.segments.iter().enumerate() {
        let yl = seg.l_render_start as isize - app.left_top as isize;
        let yr = seg.r_render_start as isize - app.right_top as isize;
        if !in_view(yl) && !in_view(yr) {
            continue;
        }
        paths.push(PathPlan {
            seg_idx: idx,
            yl,
            yr,
            line_visible: true,
        });
    }
    let active = active_segment_idx(app);
    resolve_path_conflicts(&mut paths, viewport_h, cursor_y, active);

    // Now actually draw.
    for plan in &paths {
        let seg = &app.diff.segments[plan.seg_idx];
        let yl = plan.yl;
        let yr = plan.yr;
        let same_row = yl == yr && in_view(yl);
        let l_filled = yl == cursor_y as isize || same_row;
        let r_filled = yr == cursor_y as isize || same_row;
        let l_glyph = if l_filled { '●' } else { '○' };
        let r_glyph = if r_filled { '●' } else { '○' };

        let (l_dim, l_bright, r_dim, r_bright) = if seg.is_change {
            (
                theme.change_anchor_dim_left,
                theme.change_anchor_bright_left,
                theme.change_anchor_dim_right,
                theme.change_anchor_bright_right,
            )
        } else {
            (
                theme.anchor_soft,
                theme.anchor_bright,
                theme.anchor_soft,
                theme.anchor_bright,
            )
        };
        let l_color = if l_filled { l_bright } else { l_dim };
        let r_color = if r_filled { r_bright } else { r_dim };
        let line_color = if seg.is_change {
            if l_filled || r_filled {
                theme.change_line_bright
            } else {
                theme.change_line_dim
            }
        } else if l_filled || r_filled {
            theme.line_bright
        } else {
            theme.line_dim
        };

        // Anchors are *always* drawn — only the connector line gets hidden
        // when there's a conflict. The user can still see "there's an
        // anchor here", just not its partner across the gutter. We only
        // touch char + fg, leaving the background tint intact.
        if in_view(yl) {
            let cell = &mut grid[yl as usize][0];
            cell.0 = l_glyph;
            cell.1 = l_color;
        }
        if in_view(yr) {
            let cell = &mut grid[yr as usize][4];
            cell.0 = r_glyph;
            cell.1 = r_color;
        }

        if !plan.line_visible {
            continue;
        }

        if yl == yr {
            if in_view(yl) {
                let y = yl as usize;
                grid[y][1].0 = '─';
                grid[y][1].1 = line_color;
                grid[y][2].0 = '─';
                grid[y][2].1 = line_color;
                grid[y][3].0 = '─';
                grid[y][3].1 = line_color;
            }
        } else if yl < yr {
            if in_view(yl) {
                let y = yl as usize;
                grid[y][1].0 = '─';
                grid[y][1].1 = line_color;
                grid[y][2].0 = '┐';
                grid[y][2].1 = line_color;
            }
            let from = (yl + 1).max(0);
            let to = yr.min(viewport_h as isize);
            for y in from..to {
                grid[y as usize][2].0 = '│';
                grid[y as usize][2].1 = line_color;
            }
            if in_view(yr) {
                let y = yr as usize;
                grid[y][2].0 = '└';
                grid[y][2].1 = line_color;
                grid[y][3].0 = '─';
                grid[y][3].1 = line_color;
            }
        } else {
            if in_view(yr) {
                let y = yr as usize;
                grid[y][3].0 = '─';
                grid[y][3].1 = line_color;
                grid[y][2].0 = '┌';
                grid[y][2].1 = line_color;
            }
            let from = (yr + 1).max(0);
            let to = yl.min(viewport_h as isize);
            for y in from..to {
                grid[y as usize][2].0 = '│';
                grid[y as usize][2].1 = line_color;
            }
            if in_view(yl) {
                let y = yl as usize;
                grid[y][2].0 = '┘';
                grid[y][2].1 = line_color;
                grid[y][1].0 = '─';
                grid[y][1].1 = line_color;
            }
        }
    }

    // Convert grid to Lines. Each cell brings its own bg (track tint or
    // Reset). On the alignment row we lighten the bg slightly to show the
    // band without erasing the track stripe.
    let mut lines = Vec::with_capacity(viewport_h);
    for (y, row) in grid.iter().enumerate() {
        let on_align = y == cursor_y;
        let spans: Vec<Span> = row
            .iter()
            .map(|&(c, fg, bg)| {
                let final_bg = if on_align {
                    overlay_alignment(bg, theme)
                } else {
                    bg
                };
                Span::styled(c.to_string(), Style::default().fg(fg).bg(final_bg))
            })
            .collect();
        lines.push(Line::from(spans));
    }
    lines
}

fn overlay_alignment(base: Color, theme: &Theme) -> Color {
    match base {
        Color::Reset => theme.alignment_overlay,
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(20),
            g.saturating_add(25),
            b.saturating_add(40),
        ),
        c => c,
    }
}

#[derive(Debug)]
struct PathPlan {
    seg_idx: usize,
    yl: isize,
    yr: isize,
    /// Whether to draw the L-shaped connector line. Anchors at the endpoints
    /// are drawn regardless; this only controls the middle.
    line_visible: bool,
}

/// Returns the segment index whose render-row range contains the alignment
/// cursor on the anchor side. That segment's path is "of interest" and must
/// stay drawn even when it conflicts with others.
fn active_segment_idx(app: &App) -> Option<usize> {
    let row = match app.anchor_side {
        AnchorSide::Left => app.left_top + app.cursor_y,
        AnchorSide::Right => app.right_top + app.cursor_y,
    };
    for (i, seg) in app.diff.segments.iter().enumerate() {
        let (start, end) = match app.anchor_side {
            AnchorSide::Left => (
                seg.l_render_start,
                seg.l_render_start + seg.l_render_count,
            ),
            AnchorSide::Right => (
                seg.r_render_start,
                seg.r_render_start + seg.r_render_count,
            ),
        };
        if row >= start && row < end {
            return Some(i);
        }
    }
    None
}

/// Greedily hide the most-conflicting path (one with the highest number of
/// shared middle-column rows) until no two visible paths overlap. The
/// `active` segment (if any) is pinned visible — it's the user's selection,
/// so we keep it and hide whatever blocks it instead.
///
/// Iteration is fully deterministic (Vec, not HashMap; ties broken by
/// distance from `cursor_y` then by path index). Without this, paths with
/// equal conflict counts would shuffle frame-to-frame and the gutter would
/// visibly flicker.
fn resolve_path_conflicts(
    paths: &mut [PathPlan],
    viewport_h: usize,
    cursor_y: usize,
    active: Option<usize>,
) {
    let n = paths.len();
    loop {
        let mut row_to_paths: Vec<Vec<usize>> = vec![Vec::new(); viewport_h];
        for (i, p) in paths.iter().enumerate() {
            if !p.line_visible {
                continue;
            }
            // Include anchor rows in the claim — corners (`┐`, `└` etc.)
            // are drawn into the middle column too, so they can collide
            // with another path's vertical line passing through.
            let (span_min, span_max) = if p.yl == p.yr {
                (p.yl, p.yr)
            } else {
                (p.yl.min(p.yr), p.yl.max(p.yr))
            };
            for row in span_min..=span_max {
                if row >= 0 && (row as usize) < viewport_h {
                    row_to_paths[row as usize].push(i);
                }
            }
        }

        let mut conflict_count = vec![0usize; n];
        let mut any_conflict = false;
        for paths_at in &row_to_paths {
            if paths_at.len() > 1 {
                any_conflict = true;
                for &p in paths_at {
                    conflict_count[p] += 1;
                }
            }
        }
        if !any_conflict {
            return;
        }

        // Find the path to hide: highest conflict count first; on a tie,
        // hide the one whose anchors sit *farther* from the cursor row
        // (keeps the path the user is looking at). Final tie-break: higher
        // segment index — anything stable so we don't flip-flop.
        let cy = cursor_y as isize;
        let dist = |p: &PathPlan| -> isize {
            let dl = (p.yl - cy).abs();
            let dr = (p.yr - cy).abs();
            dl.min(dr)
        };
        let mut target: Option<usize> = None;
        for i in 0..n {
            if !paths[i].line_visible {
                continue;
            }
            if Some(paths[i].seg_idx) == active {
                continue;
            }
            if conflict_count[i] == 0 {
                continue;
            }
            target = Some(match target {
                None => i,
                Some(j) => {
                    let key_i = (conflict_count[i], dist(&paths[i]), paths[i].seg_idx);
                    let key_j = (conflict_count[j], dist(&paths[j]), paths[j].seg_idx);
                    if key_i > key_j {
                        i
                    } else {
                        j
                    }
                }
            });
        }
        match target {
            Some(p) => paths[p].line_visible = false,
            None => return,
        }
    }
}

fn render_pane_row(
    theme: &Theme,
    line: Option<&PaneLine>,
    is_left: bool,
    is_alignment: bool,
    text_w: usize,
    lineno_w: usize,
) -> (Line<'static>, Line<'static>) {
    let Some(line) = line else {
        let bg = if is_alignment {
            theme.alignment_overlay
        } else {
            Color::Reset
        };
        let blank = Span::styled(" ".repeat(lineno_w + text_w), Style::default().bg(bg));
        return (Line::from(blank.clone()), Line::from(blank));
    };
    let bg = line_bg(theme, line.kind, is_left, is_alignment);

    let no_text = format!(" {:>width$} ", line.line_no, width = lineno_w.saturating_sub(2));
    let no_style = Style::default().fg(theme.fg_gutter).bg(bg);
    let no_line = Line::from(Span::styled(no_text, no_style));

    let mut spans: Vec<Span> = Vec::new();
    let mut used = 0usize;
    for seg in &line.segments {
        let style = segment_style(theme, line.kind, seg, is_left, bg);
        let text = visible_truncate(&seg.text, text_w.saturating_sub(used));
        used += text.chars().count();
        spans.push(Span::styled(text, style));
        if used >= text_w {
            break;
        }
    }
    if used < text_w {
        spans.push(Span::styled(
            " ".repeat(text_w - used),
            Style::default().bg(bg),
        ));
    }
    (no_line, Line::from(spans))
}

fn segment_style(theme: &Theme, kind: LineKind, seg: &Segment, is_left: bool, row_bg: Color) -> Style {
    let mut s = Style::default().bg(row_bg);
    s = match (kind, seg.kind) {
        (LineKind::Modified, SegmentKind::Changed) => {
            if is_left {
                s.bg(theme.hl_delete).fg(Color::White)
            } else {
                s.bg(theme.hl_insert).fg(Color::White)
            }
        }
        (LineKind::Standalone, _) => {
            if is_left {
                s.fg(theme.fg_standalone_left)
            } else {
                s.fg(theme.fg_standalone_right)
            }
        }
        _ => s,
    };
    s
}

fn line_bg(theme: &Theme, kind: LineKind, is_left: bool, is_alignment: bool) -> Color {
    let base = match kind {
        LineKind::Equal => Color::Reset,
        LineKind::Standalone => {
            if is_left {
                theme.bg_delete
            } else {
                theme.bg_insert
            }
        }
        LineKind::Modified => {
            if is_left {
                theme.bg_mod_left
            } else {
                theme.bg_mod_right
            }
        }
    };
    if is_alignment {
        match base {
            Color::Reset => theme.alignment_overlay,
            Color::Rgb(r, g, b) => Color::Rgb(
                r.saturating_add(15),
                g.saturating_add(20),
                b.saturating_add(30),
            ),
            c => c,
        }
    } else {
        base
    }
}

fn lineno_width(lines: &[PaneLine]) -> usize {
    let max = lines.iter().map(|l| l.line_no).max().unwrap_or(1);
    max.to_string().len() + 2
}

fn visible_truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn border_style(theme: &Theme, focused: bool) -> Style {
    if focused {
        Style::default().fg(theme.diff_focus)
    } else {
        Style::default().fg(theme.diff_dim)
    }
}

fn side_border_style(theme: &Theme, is_left: bool, focused: bool) -> Style {
    let color = match (is_left, focused) {
        (true, true) => theme.border_left_focus,
        (true, false) => theme.border_left_dim,
        (false, true) => theme.border_right_focus,
        (false, false) => theme.border_right_dim,
    };
    Style::default().fg(color)
}

fn status_color(theme: &Theme, status: &str) -> Color {
    // Unmerged states from `git status --porcelain`: "DD", "AU", "UD", "UA",
    // "DU", "AA", "UU". Any pair containing 'U', plus AA/DD which mean both
    // sides did the same thing concurrently.
    let trimmed = status.trim();
    if status.contains('U') || matches!(trimmed, "AA" | "DD") {
        return theme.status_conflict_fg;
    }
    let c = status.chars().next().unwrap_or(' ');
    match c {
        'M' => Color::Yellow,
        'A' => Color::Green,
        'D' => Color::Red,
        'R' | 'C' => Color::Cyan,
        '?' => Color::Magenta,
        _ => Color::Gray,
    }
}

fn short_label(s: &str) -> String {
    let max = 32;
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let tail: String = s.chars().rev().take(max - 1).collect::<Vec<_>>().into_iter().rev().collect();
        format!("…{}", tail)
    }
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let hint = if app.show_help {
        app.strings.status_hint_help_open.clone()
    } else {
        app.strings.status_hint_default.clone()
    };
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", hint),
            Style::default().fg(Color::Rgb(180, 180, 180)),
        ),
        Span::raw("  "),
        Span::styled(
            app.status.clone(),
            Style::default().fg(Color::Rgb(140, 200, 140)),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_help(f: &mut Frame, area: Rect, app: &App) {
    let theme = &app.config.theme;
    let sections = &app.strings.help_sections;
    if sections.is_empty() {
        return;
    }

    // Right-align the keys column to the longest `keys` string across all
    // sections so descriptions line up. Two spaces of gutter on each side
    // plus 4 spaces between keys and desc.
    let keys_w = sections
        .iter()
        .flat_map(|s| s.entries.iter().map(|e| e.keys.chars().count()))
        .max()
        .unwrap_or(8);
    let desc_w = sections
        .iter()
        .flat_map(|s| s.entries.iter().map(|e| e.desc.chars().count()))
        .max()
        .unwrap_or(20);
    let title_w = sections
        .iter()
        .map(|s| s.title.chars().count())
        .max()
        .unwrap_or(0);

    // Body width = max(title, keys + 4 + desc) + horizontal padding (4).
    let entry_w = keys_w + 4 + desc_w;
    let body_w = entry_w.max(title_w) + 4;
    let w = (body_w as u16)
        .clamp(36, 96)
        .min(area.width.saturating_sub(4));

    // Body height: one line per entry, blank line between sections, plus
    // a section header row each, plus a top blank row. Outer +2 for the
    // bordered block.
    let mut body_h = 1usize; // top padding
    for (i, s) in sections.iter().enumerate() {
        if i > 0 {
            body_h += 1; // separator blank line
        }
        body_h += 1; // section title
        body_h += s.entries.len();
    }
    body_h += 1; // bottom padding
    let h = (body_h as u16 + 2).min(area.height.saturating_sub(4));

    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let r = Rect::new(x, y, w, h);
    f.render_widget(Clear, r);

    let bg = theme.help_panel_bg;
    let mut lines: Vec<Line> = Vec::with_capacity(body_h);
    lines.push(Line::raw(""));
    for (i, sec) in sections.iter().enumerate() {
        if i > 0 {
            lines.push(Line::from(Span::styled("", Style::default().bg(bg))));
        }
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(
                sec.title.clone(),
                Style::default()
                    .fg(theme.help_section_fg)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        for entry in &sec.entries {
            let pad = keys_w.saturating_sub(entry.keys.chars().count());
            let pad_str = " ".repeat(pad);
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default().bg(bg)),
                Span::styled(pad_str, Style::default().bg(bg)),
                Span::styled(
                    entry.keys.clone(),
                    Style::default()
                        .fg(theme.help_keys_fg)
                        .bg(bg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("    ", Style::default().bg(bg)),
                Span::styled(
                    entry.desc.clone(),
                    Style::default().fg(theme.help_desc_fg).bg(bg),
                ),
            ]));
        }
    }
    lines.push(Line::raw(""));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(app.strings.title_help.clone())
        .style(Style::default().bg(bg))
        .title_style(
            Style::default()
                .fg(theme.help_section_fg)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(Style::default().fg(theme.help_border_fg).bg(bg));
    let p = Paragraph::new(lines).block(block);
    f.render_widget(p, r);
}
