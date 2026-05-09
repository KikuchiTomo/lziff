use crate::app::{App, Focus, LayoutCache};
use crate::diff::{Row, RowKind, Segment, SegmentKind, Side};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

// Subtle row tints — meant to read as "this line is part of a change" without
// drowning the text. The strong signal lives in the gutter bars and the
// per-token highlight, not in the row background.
const BG_DELETE_LINE: Color = Color::Rgb(48, 22, 26);
const BG_INSERT_LINE: Color = Color::Rgb(20, 44, 26);
// Filler rows sit at terminal-default to make the asymmetry obvious: in a
// 3-vs-24 change block, the short side reads as "3 colored rows then nothing".
const BG_FILLER: Color = Color::Reset;
// Per-token highlights inside a Replace row. Strong enough to pop, restrained
// enough not to look like a Christmas tree.
const HL_INSERT: Color = Color::Rgb(50, 130, 70);
const HL_DELETE: Color = Color::Rgb(150, 60, 70);
const FG_GUTTER: Color = Color::Rgb(120, 120, 120);

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

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(28), Constraint::Min(20)])
        .split(chunks[1]);

    let mut layout = LayoutCache::default();
    render_files(f, body[0], app, &mut layout);
    render_diff(f, body[1], app, &mut layout);
    app.layout = layout;

    render_status(f, chunks[2], app);

    if app.show_help {
        render_help(f, area);
    }
}

fn render_title(f: &mut Frame, area: Rect, app: &App) {
    let (l, r) = app.header_label();
    let (a, d, m) = app.change_summary();
    let title = Line::from(vec![
        Span::styled(
            " vdiff ",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(60, 90, 140))
                .fg(Color::White),
        ),
        Span::raw(" "),
        Span::styled(l, Style::default().fg(Color::Rgb(200, 120, 120))),
        Span::raw("  →  "),
        Span::styled(r, Style::default().fg(Color::Rgb(120, 200, 140))),
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
    let focused = matches!(app.focus, Focus::Files);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Files ")
        .border_style(border_style(focused));
    layout.files_inner = block.inner(area);

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|e| {
            let color = status_color(&e.status);
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", e.status), Style::default().fg(color)),
                Span::raw(e.display.clone()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(if focused {
                    Color::Rgb(50, 70, 100)
                } else {
                    Color::Rgb(50, 50, 50)
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
    let focused = matches!(app.focus, Focus::Diff);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Diff ")
        .border_style(border_style(focused));
    let inner = block.inner(area);
    layout.diff_inner = inner;
    f.render_widget(block, area);

    if app.diff.rows.is_empty() {
        let msg = if app.entries.is_empty() {
            "no changes"
        } else {
            "(no diff)"
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let viewport_h = inner.height as usize;
    if viewport_h == 0 {
        return;
    }

    // Auto-follow: keep cursor in view.
    let mut top = layout.diff_top_row;
    if app.cursor_row < top {
        top = app.cursor_row;
    } else if app.cursor_row >= top + viewport_h {
        top = app.cursor_row + 1 - viewport_h;
    }
    layout.diff_top_row = top;

    let lineno_w = lineno_width(&app.diff.rows);
    // 5 columns: gutter(L) | text(L) | bar(LR) | gutter(R) | text(R)
    let bar_w: u16 = 2;
    let gutter_w = lineno_w as u16;
    let total_fixed = gutter_w * 2 + bar_w;
    let text_w = inner.width.saturating_sub(total_fixed) / 2;

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(gutter_w),
            Constraint::Length(text_w),
            Constraint::Length(bar_w),
            Constraint::Length(gutter_w),
            Constraint::Min(0),
        ])
        .split(inner);

    let mut left_no_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut left_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut bar_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut right_no_lines: Vec<Line> = Vec::with_capacity(viewport_h);
    let mut right_lines: Vec<Line> = Vec::with_capacity(viewport_h);

    for i in 0..viewport_h {
        let row_idx = top + i;
        let Some(row) = app.diff.rows.get(row_idx) else {
            left_no_lines.push(Line::raw(""));
            left_lines.push(Line::raw(""));
            bar_lines.push(Line::raw(""));
            right_no_lines.push(Line::raw(""));
            right_lines.push(Line::raw(""));
            continue;
        };
        let is_cursor = row_idx == app.cursor_row;

        let (lno, ltxt) = render_side(&row.left, row, true, is_cursor, text_w as usize, lineno_w);
        let (rno, rtxt) = render_side(&row.right, row, false, is_cursor, text_w as usize, lineno_w);
        left_no_lines.push(lno);
        left_lines.push(ltxt);
        bar_lines.push(render_bar(row, is_cursor));
        right_no_lines.push(rno);
        right_lines.push(rtxt);
    }

    f.render_widget(Paragraph::new(left_no_lines), cols[0]);
    f.render_widget(Paragraph::new(left_lines), cols[1]);
    f.render_widget(Paragraph::new(bar_lines), cols[2]);
    f.render_widget(Paragraph::new(right_no_lines), cols[3]);
    f.render_widget(Paragraph::new(right_lines), cols[4]);
}

fn render_side(
    side: &Side,
    row: &Row,
    is_left: bool,
    is_cursor: bool,
    width: usize,
    lineno_w: usize,
) -> (Line<'static>, Line<'static>) {
    let is_filler = side.line_no.is_none();
    let bg = row_bg(row.kind, is_left, is_filler);
    let cursor_mod = if is_cursor {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };
    let cursor_bg_mod = if is_cursor {
        Some(cursor_overlay(bg))
    } else {
        None
    };
    let line_bg = cursor_bg_mod.unwrap_or(bg);

    // Line number column.
    let no_text = side
        .line_no
        .map(|n| format!(" {:>width$} ", n, width = lineno_w.saturating_sub(2)))
        .unwrap_or_else(|| " ".repeat(lineno_w));
    let no_line = Line::from(Span::styled(
        no_text,
        Style::default()
            .fg(FG_GUTTER)
            .bg(line_bg)
            .add_modifier(cursor_mod),
    ));

    // Text column with intra-line highlight.
    let mut spans: Vec<Span> = Vec::new();
    if is_filler {
        // Filler is intentionally bare so the asymmetry of a change block reads
        // as "real lines on this side, nothing on that side".
        spans.push(Span::styled(" ".repeat(width), Style::default().bg(line_bg)));
    } else {
        let mut used = 0usize;
        for seg in &side.segments {
            let style = segment_style(row.kind, seg, is_left, line_bg, cursor_mod);
            let text = visible_truncate(&seg.text, width.saturating_sub(used));
            used += text.chars().count();
            spans.push(Span::styled(text, style));
            if used >= width {
                break;
            }
        }
        if used < width {
            spans.push(Span::styled(
                " ".repeat(width - used),
                Style::default().bg(line_bg),
            ));
        }
    }
    (no_line, Line::from(spans))
}

/// 2-char bar between the two text panes. Vertically stacked rows of the same
/// kind merge into a continuous colored stripe — that's the visual cue that the
/// 3 deletes on the left and 24 inserts on the right are *one* change block.
fn render_bar(row: &Row, is_cursor: bool) -> Line<'static> {
    let (lc, rc) = match row.kind {
        RowKind::Equal => (' ', ' '),
        RowKind::Insert => (' ', '▐'),
        RowKind::Delete => ('▌', ' '),
        RowKind::Replace => ('▌', '▐'),
    };
    let l_color = if matches!(row.kind, RowKind::Delete | RowKind::Replace) {
        Color::Rgb(180, 70, 80)
    } else {
        Color::Reset
    };
    let r_color = if matches!(row.kind, RowKind::Insert | RowKind::Replace) {
        Color::Rgb(70, 160, 90)
    } else {
        Color::Reset
    };
    let bg = if is_cursor {
        Color::Rgb(40, 50, 70)
    } else {
        Color::Reset
    };
    Line::from(vec![
        Span::styled(lc.to_string(), Style::default().fg(l_color).bg(bg)),
        Span::styled(rc.to_string(), Style::default().fg(r_color).bg(bg)),
    ])
}

fn segment_style(
    kind: RowKind,
    seg: &Segment,
    is_left: bool,
    row_bg: Color,
    cursor_mod: Modifier,
) -> Style {
    let mut s = Style::default().bg(row_bg).add_modifier(cursor_mod);
    s = match (kind, seg.kind) {
        (RowKind::Replace, SegmentKind::Changed) => {
            if is_left {
                s.bg(HL_DELETE).fg(Color::White)
            } else {
                s.bg(HL_INSERT).fg(Color::White)
            }
        }
        (RowKind::Insert, _) => s.fg(Color::Rgb(190, 230, 195)),
        (RowKind::Delete, _) => s.fg(Color::Rgb(230, 190, 195)),
        _ => s,
    };
    s
}

fn row_bg(kind: RowKind, is_left: bool, is_filler: bool) -> Color {
    if is_filler {
        return BG_FILLER;
    }
    match (kind, is_left) {
        (RowKind::Equal, _) => Color::Reset,
        (RowKind::Insert, true) => BG_FILLER,
        (RowKind::Insert, false) => BG_INSERT_LINE,
        (RowKind::Delete, true) => BG_DELETE_LINE,
        (RowKind::Delete, false) => BG_FILLER,
        (RowKind::Replace, true) => BG_DELETE_LINE,
        (RowKind::Replace, false) => BG_INSERT_LINE,
    }
}

fn cursor_overlay(bg: Color) -> Color {
    match bg {
        Color::Reset => Color::Rgb(35, 40, 55),
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(20),
            g.saturating_add(20),
            b.saturating_add(35),
        ),
        c => c,
    }
}

fn lineno_width(rows: &[Row]) -> usize {
    let max = rows
        .iter()
        .filter_map(|r| r.left.line_no.max(r.right.line_no))
        .max()
        .unwrap_or(1);
    let digits = max.to_string().len();
    digits + 2
}

fn visible_truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Rgb(120, 160, 220))
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn status_color(status: &str) -> Color {
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

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let hint = if app.show_help {
        "press ? to close help"
    } else {
        "j/k move  J/K hunk  n/p file  Tab focus  click/wheel mouse  r reload  ? help  q quit"
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

fn render_help(f: &mut Frame, area: Rect) {
    let w = 60u16.min(area.width.saturating_sub(4));
    let h = 18u16.min(area.height.saturating_sub(4));
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let r = Rect::new(x, y, w, h);
    f.render_widget(Clear, r);
    let lines = vec![
        Line::from(Span::styled(
            "  Keys",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::raw("  j / k       move cursor"),
        Line::raw("  J / K       next / prev hunk"),
        Line::raw("  ctrl-d/u    half-page down / up"),
        Line::raw("  g / G       top / bottom"),
        Line::raw("  n / p       next / prev file"),
        Line::raw("  Tab         toggle focus (files / diff)"),
        Line::raw("  click       focus & select row"),
        Line::raw("  wheel       scroll under pointer"),
        Line::raw("  r           reload manually (auto-reload runs always)"),
        Line::raw("  ? / esc     toggle / close this help"),
        Line::raw("  q          quit"),
    ];
    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Help ")
            .border_style(Style::default().fg(Color::Rgb(120, 160, 220))),
    );
    f.render_widget(p, r);
}
