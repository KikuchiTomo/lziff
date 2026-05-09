//! Diff engine: computes per-side line streams that the two panes render
//! independently. There is no unified row model and no filler — the left pane
//! shows only the left file's real lines, the right pane shows only the right
//! file's real lines, and each pane scrolls on its own.
//!
//! The diff still tags every line with its kind (Equal / Modified / Standalone)
//! so the renderer can color and word-highlight; it also records the matched
//! equal-line pairs as anchors that the UI can snap-align on demand.

use similar::{Algorithm, ChangeTag, TextDiff};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    /// Identical on both sides.
    Equal,
    /// Paired with a counterpart line on the other side that's similar.
    /// Carries word-level intra-line highlights.
    Modified,
    /// Pure insert (right) or pure delete (left) — no counterpart.
    Standalone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentKind {
    Same,
    Changed,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub text: String,
    pub kind: SegmentKind,
}

#[derive(Debug, Clone)]
pub struct PaneLine {
    /// 1-based line number on this side.
    pub line_no: usize,
    /// Word-level segmentation. Single `Same` segment for `Equal` lines.
    pub segments: Vec<Segment>,
    pub kind: LineKind,
}

/// One alignment segment in the diff: a contiguous run that's either an
/// Equal block (l_count == r_count) or a Change block (any L, R). The list
/// of segments forms the alignment table — each segment boundary is an
/// "anchor pair" the ribbon connects between the two panes.
#[derive(Debug, Clone, Copy)]
pub struct AlignSegment {
    /// First source-line index covered by this segment on each side.
    pub l_start: usize,
    pub r_start: usize,
    /// Source-line counts.
    pub l_count: usize,
    pub r_count: usize,
    /// Render-row range. Pure delete/insert change segments inject a single
    /// phantom (blank) row on the empty side so every change still has an
    /// anchor on both panes. For Equal segments and mixed changes,
    /// `*_render_count == *_count`.
    pub l_render_start: usize,
    pub l_render_count: usize,
    pub r_render_start: usize,
    pub r_render_count: usize,
    #[allow(dead_code)]
    pub span: usize,
    pub is_change: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub left: Vec<PaneLine>,
    pub right: Vec<PaneLine>,
    pub segments: Vec<AlignSegment>,
    pub anchors: Vec<(usize, usize)>,
    pub left_hunks: Vec<(usize, usize)>,
    pub right_hunks: Vec<(usize, usize)>,
    #[allow(dead_code)]
    pub virtual_len: usize,
    /// Render streams: each entry maps a render row to a source line index
    /// (`Some`) or marks it as a phantom (`None`). Phantoms appear only on
    /// the empty side of pure delete/insert change segments — they give the
    /// ribbon a snap target even when the segment has no real content on
    /// that pane.
    pub left_render: Vec<Option<usize>>,
    pub right_render: Vec<Option<usize>>,
}

impl Diff {
    /// Index of the segment whose left render-range contains `l_render`.
    pub fn segment_for_left_render(&self, l_render: usize) -> Option<usize> {
        for (i, seg) in self.segments.iter().enumerate() {
            if l_render >= seg.l_render_start
                && l_render < seg.l_render_start + seg.l_render_count
            {
                return Some(i);
            }
        }
        None
    }

    pub fn segment_for_right_render(&self, r_render: usize) -> Option<usize> {
        for (i, seg) in self.segments.iter().enumerate() {
            if r_render >= seg.r_render_start
                && r_render < seg.r_render_start + seg.r_render_count
            {
                return Some(i);
            }
        }
        None
    }

    /// Map a *left render row* to the right render row that corresponds to
    /// it: Equal pairs map 1:1, Modified pairs at the same offset, Standalone
    /// extras clamp to the last paired row, and phantom rows pair with their
    /// counterpart segment's start (the real content on the other side).
    /// Used by click-snap to position the other pane.
    pub fn corresponding_right_for_left(&self, l_render: usize) -> usize {
        for seg in &self.segments {
            let l_end = seg.l_render_start + seg.l_render_count;
            if l_render < l_end {
                let off = l_render - seg.l_render_start;
                if seg.is_change {
                    let pair = seg.l_render_count.min(seg.r_render_count);
                    if off < pair {
                        return seg.r_render_start + off;
                    }
                    return seg.r_render_start + seg.r_render_count.saturating_sub(1);
                }
                return seg.r_render_start + off;
            }
        }
        self.right_render.len().saturating_sub(1)
    }

    pub fn corresponding_left_for_right(&self, r_render: usize) -> usize {
        for seg in &self.segments {
            let r_end = seg.r_render_start + seg.r_render_count;
            if r_render < r_end {
                let off = r_render - seg.r_render_start;
                if seg.is_change {
                    let pair = seg.l_render_count.min(seg.r_render_count);
                    if off < pair {
                        return seg.l_render_start + off;
                    }
                    return seg.l_render_start + seg.l_render_count.saturating_sub(1);
                }
                return seg.l_render_start + off;
            }
        }
        self.left_render.len().saturating_sub(1)
    }

    /// Inverse of `map` for a left-side line index. Currently unused (the
    /// renderer uses independent per-pane scrolls), kept for the
    /// snap-scroll mode planned later.
    #[allow(dead_code)]
    pub fn virtual_of_left(&self, l: usize) -> usize {
        let mut v0 = 0usize;
        for seg in &self.segments {
            let l_end = seg.l_start + seg.l_count;
            if l < l_end {
                let off = l - seg.l_start;
                // Within both equal segments (1:1) and change segments
                // (1:1 from the top until clamp at L-1), the v that places
                // line `l` at the alignment row is simply v0 + off.
                return v0 + off;
            }
            v0 += seg.span;
        }
        self.virtual_len.saturating_sub(1)
    }

    #[allow(dead_code)]
    pub fn virtual_of_right(&self, r: usize) -> usize {
        let mut v0 = 0usize;
        for seg in &self.segments {
            let r_end = seg.r_start + seg.r_count;
            if r < r_end {
                let off = r - seg.r_start;
                return v0 + off;
            }
            v0 += seg.span;
        }
        self.virtual_len.saturating_sub(1)
    }

    /// Map a virtual scroll position to per-side line offsets. Unused by
    /// the current renderer; kept for the synchronized snap-scroll mode.
    #[allow(dead_code)]
    ///
    /// Snap-aligned model: in equal segments both sides advance 1:1. In a
    /// change segment with `L` left-lines and `R` right-lines (span = max),
    /// the shorter side advances 1:1 through its own change content and then
    /// **clamps at its last change line** while the longer side keeps
    /// scrolling. When the longer side finishes the segment, both sides
    /// advance simultaneously to the next equal anchor — a one-step "snap".
    /// No blanks, no repetition; each source line shown exactly once.
    pub fn map(&self, mut v: usize) -> (usize, usize) {
        if self.virtual_len == 0 {
            return (0, 0);
        }
        if v >= self.virtual_len {
            return (self.left.len(), self.right.len());
        }
        for seg in &self.segments {
            if v < seg.span {
                let (l_off, r_off) = if seg.is_change {
                    let l_max = seg.l_count.saturating_sub(1);
                    let r_max = seg.r_count.saturating_sub(1);
                    (v.min(l_max), v.min(r_max))
                } else {
                    (v, v)
                };
                return (seg.l_start + l_off, seg.r_start + r_off);
            }
            v -= seg.span;
        }
        (self.left.len(), self.right.len())
    }
}

impl Diff {
    pub fn compute(left_text: &str, right_text: &str, similarity_threshold: f32) -> Self {
        let left_lines: Vec<&str> = split_lines(left_text);
        let right_lines: Vec<&str> = split_lines(right_text);

        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(left_text, right_text);

        let mut out = Diff::default();
        let mut left_no = 0usize;
        let mut right_no = 0usize;

        // Segment accumulator. We emit a segment whenever the kind transitions
        // (equal ↔ change) so the alignment table comes out as a strict
        // interleaving of equal and change segments.
        let mut eq_run = 0usize;
        let mut eq_l_start = 0usize;
        let mut eq_r_start = 0usize;
        let mut pending_dels: Vec<(usize, &str)> = Vec::new();
        let mut pending_ins: Vec<(usize, &str)> = Vec::new();
        let mut ch_l_start = 0usize;
        let mut ch_r_start = 0usize;
        let mut in_change = false;

        for op in diff.ops() {
            for change in diff.iter_changes(op) {
                match change.tag() {
                    ChangeTag::Equal => {
                        if in_change {
                            flush_change_segment(
                                &mut out,
                                &mut pending_dels,
                                &mut pending_ins,
                                ch_l_start,
                                ch_r_start,
                                similarity_threshold,
                            );
                            in_change = false;
                            eq_l_start = out.left.len();
                            eq_r_start = out.right.len();
                            eq_run = 0;
                        }
                        if eq_run == 0 {
                            eq_l_start = out.left.len();
                            eq_r_start = out.right.len();
                        }
                        left_no += 1;
                        right_no += 1;
                        let l_idx = out.left.len();
                        let r_idx = out.right.len();
                        let l = left_lines.get(left_no - 1).copied().unwrap_or("");
                        let r = right_lines.get(right_no - 1).copied().unwrap_or("");
                        out.left.push(PaneLine {
                            line_no: left_no,
                            segments: vec![Segment {
                                text: l.to_string(),
                                kind: SegmentKind::Same,
                            }],
                            kind: LineKind::Equal,
                        });
                        out.right.push(PaneLine {
                            line_no: right_no,
                            segments: vec![Segment {
                                text: r.to_string(),
                                kind: SegmentKind::Same,
                            }],
                            kind: LineKind::Equal,
                        });
                        out.anchors.push((l_idx, r_idx));
                        eq_run += 1;
                    }
                    tag @ (ChangeTag::Delete | ChangeTag::Insert) => {
                        if !in_change {
                            if eq_run > 0 {
                                out.segments.push(AlignSegment {
                                    l_start: eq_l_start,
                                    r_start: eq_r_start,
                                    l_count: eq_run,
                                    r_count: eq_run,
                                    l_render_start: 0,
                                    l_render_count: 0,
                                    r_render_start: 0,
                                    r_render_count: 0,
                                    span: eq_run,
                                    is_change: false,
                                });
                                out.virtual_len += eq_run;
                                eq_run = 0;
                            }
                            ch_l_start = out.left.len();
                            ch_r_start = out.right.len();
                            in_change = true;
                        }
                        match tag {
                            ChangeTag::Delete => {
                                left_no += 1;
                                let l = left_lines.get(left_no - 1).copied().unwrap_or("");
                                pending_dels.push((left_no, l));
                            }
                            ChangeTag::Insert => {
                                right_no += 1;
                                let r = right_lines.get(right_no - 1).copied().unwrap_or("");
                                pending_ins.push((right_no, r));
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }
        // Flush trailing segment.
        if in_change {
            flush_change_segment(
                &mut out,
                &mut pending_dels,
                &mut pending_ins,
                ch_l_start,
                ch_r_start,
                similarity_threshold,
            );
        } else if eq_run > 0 {
            out.segments.push(AlignSegment {
                l_start: eq_l_start,
                r_start: eq_r_start,
                l_count: eq_run,
                r_count: eq_run,
                l_render_start: 0,
                l_render_count: 0,
                r_render_start: 0,
                r_render_count: 0,
                span: eq_run,
                is_change: false,
            });
            out.virtual_len += eq_run;
        }

        // Now lay out render rows. Pure delete/insert change segments get a
        // single phantom row on the empty side so every segment has a snap
        // anchor on both panes.
        let mut l_pos = 0usize;
        let mut r_pos = 0usize;
        for seg in &mut out.segments {
            seg.l_render_start = l_pos;
            seg.r_render_start = r_pos;
            let l_render = if seg.is_change && seg.l_count == 0 { 1 } else { seg.l_count };
            let r_render = if seg.is_change && seg.r_count == 0 { 1 } else { seg.r_count };
            seg.l_render_count = l_render;
            seg.r_render_count = r_render;
            l_pos += l_render;
            r_pos += r_render;
        }
        out.left_render = Vec::with_capacity(l_pos);
        out.right_render = Vec::with_capacity(r_pos);
        for seg in &out.segments {
            if seg.is_change && seg.l_count == 0 {
                out.left_render.push(None);
            } else {
                for i in 0..seg.l_count {
                    out.left_render.push(Some(seg.l_start + i));
                }
            }
            if seg.is_change && seg.r_count == 0 {
                out.right_render.push(None);
            } else {
                for i in 0..seg.r_count {
                    out.right_render.push(Some(seg.r_start + i));
                }
            }
        }

        out.left_hunks = compute_hunks(&out.left);
        out.right_hunks = compute_hunks(&out.right);
        out
    }
}

fn split_lines(s: &str) -> Vec<&str> {
    s.split_terminator('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// Emit a Change AlignSegment plus per-side PaneLines for a flushed change run.
/// Pairs the first `min(L,R)` lines as Modified (with word diff when similar),
/// rest are Standalone. The segment's span is `max(L,R)` so within it the
/// shorter side advances at fractional rate while the longer side advances
/// 1:1 per virtual scroll unit — that's the "speed difference".
fn flush_change_segment(
    out: &mut Diff,
    dels: &mut Vec<(usize, &str)>,
    ins: &mut Vec<(usize, &str)>,
    l_start: usize,
    r_start: usize,
    similarity_threshold: f32,
) {
    let l_count = dels.len();
    let r_count = ins.len();
    if l_count == 0 && r_count == 0 {
        return;
    }
    let pair_count = l_count.min(r_count);
    for i in 0..pair_count {
        let (lno, l_text) = dels[i];
        let (rno, r_text) = ins[i];
        let (lsegs, rsegs) = if similarity(l_text, r_text) >= similarity_threshold {
            word_diff(l_text, r_text)
        } else {
            (
                vec![Segment {
                    text: l_text.to_string(),
                    kind: SegmentKind::Changed,
                }],
                vec![Segment {
                    text: r_text.to_string(),
                    kind: SegmentKind::Changed,
                }],
            )
        };
        out.left.push(PaneLine {
            line_no: lno,
            segments: lsegs,
            kind: LineKind::Modified,
        });
        out.right.push(PaneLine {
            line_no: rno,
            segments: rsegs,
            kind: LineKind::Modified,
        });
    }
    for &(lno, l_text) in &dels[pair_count..] {
        out.left.push(PaneLine {
            line_no: lno,
            segments: vec![Segment {
                text: l_text.to_string(),
                kind: SegmentKind::Changed,
            }],
            kind: LineKind::Standalone,
        });
    }
    for &(rno, r_text) in &ins[pair_count..] {
        out.right.push(PaneLine {
            line_no: rno,
            segments: vec![Segment {
                text: r_text.to_string(),
                kind: SegmentKind::Changed,
            }],
            kind: LineKind::Standalone,
        });
    }
    let span = l_count.max(r_count);
    out.segments.push(AlignSegment {
        l_start,
        r_start,
        l_count,
        r_count,
        l_render_start: 0,
        l_render_count: 0,
        r_render_start: 0,
        r_render_count: 0,
        span,
        is_change: true,
    });
    out.virtual_len += span;
    dels.clear();
    ins.clear();
}

/// Token-level Jaccard-ish similarity, weighted by token count.
fn similarity(a: &str, b: &str) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let at = tokenize(a);
    let bt = tokenize(b);
    if at.is_empty() || bt.is_empty() {
        return 0.0;
    }
    let mut acount: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    for t in &at {
        *acount.entry(t.as_str()).or_default() += 1;
    }
    let mut bcount: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    for t in &bt {
        *bcount.entry(t.as_str()).or_default() += 1;
    }
    let mut shared = 0i32;
    for (k, &av) in &acount {
        if let Some(&bv) = bcount.get(k) {
            shared += av.min(bv);
        }
    }
    let denom = (at.len() + bt.len()) as f32;
    (2.0 * shared as f32) / denom
}

/// Intra-line word/grapheme diff. Same approach as before — just lifted out
/// to operate per Modified-pair.
fn word_diff(left: &str, right: &str) -> (Vec<Segment>, Vec<Segment>) {
    let l_tokens = tokenize(left);
    let r_tokens = tokenize(right);
    let l_refs: Vec<&str> = l_tokens.iter().map(String::as_str).collect();
    let r_refs: Vec<&str> = r_tokens.iter().map(String::as_str).collect();
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_slices(&l_refs, &r_refs);

    let mut l_out: Vec<Segment> = Vec::new();
    let mut r_out: Vec<Segment> = Vec::new();
    for change in diff.iter_all_changes() {
        let tok = change.value().to_string();
        match change.tag() {
            ChangeTag::Equal => {
                push_segment(&mut l_out, &tok, SegmentKind::Same);
                push_segment(&mut r_out, &tok, SegmentKind::Same);
            }
            ChangeTag::Delete => push_segment(&mut l_out, &tok, SegmentKind::Changed),
            ChangeTag::Insert => push_segment(&mut r_out, &tok, SegmentKind::Changed),
        }
    }
    (l_out, r_out)
}

fn push_segment(out: &mut Vec<Segment>, text: &str, kind: SegmentKind) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut() {
        if last.kind == kind {
            last.text.push_str(text);
            return;
        }
    }
    out.push(Segment {
        text: text.to_string(),
        kind,
    });
}

fn tokenize(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut cur_kind: Option<TokKind> = None;
    for g in s.graphemes(true) {
        let k = classify(g);
        match cur_kind {
            Some(prev) if prev == k && k != TokKind::Symbol => cur.push_str(g),
            _ => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push_str(g);
                cur_kind = Some(k);
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokKind {
    Word,
    Space,
    Symbol,
}

fn classify(g: &str) -> TokKind {
    let c = g.chars().next().unwrap_or(' ');
    if c.is_whitespace() {
        TokKind::Space
    } else if c.is_alphanumeric() || c == '_' {
        TokKind::Word
    } else {
        TokKind::Symbol
    }
}

fn compute_hunks(side: &[PaneLine]) -> Vec<(usize, usize)> {
    let mut hunks = Vec::new();
    let mut start: Option<usize> = None;
    for (i, line) in side.iter().enumerate() {
        match (line.kind, start) {
            (LineKind::Equal, Some(s)) => {
                hunks.push((s, i));
                start = None;
            }
            (LineKind::Equal, None) => {}
            (_, None) => start = Some(i),
            (_, Some(_)) => {}
        }
    }
    if let Some(s) = start {
        hunks.push((s, side.len()));
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_inputs_produce_only_equal_lines() {
        let d = Diff::compute("a\nb\nc\n", "a\nb\nc\n", 0.30);
        assert_eq!(d.left.len(), 3);
        assert_eq!(d.right.len(), 3);
        assert!(d.left.iter().all(|l| l.kind == LineKind::Equal));
        assert!(d.right.iter().all(|l| l.kind == LineKind::Equal));
        assert_eq!(d.anchors.len(), 3);
        assert!(d.left_hunks.is_empty());
    }

    #[test]
    fn left_and_right_have_independent_lengths() {
        // 1 delete vs 4 inserts: left has 3 lines, right has 6 — no filler.
        let d = Diff::compute("eq\nold\ntail\n", "eq\nn1\nn2\nn3\nn4\ntail\n", 0.30);
        assert_eq!(d.left.len(), 3);
        assert_eq!(d.right.len(), 6);
    }

    #[test]
    fn modified_pair_carries_word_diff() {
        let d = Diff::compute("hello world\n", "hello brave world\n", 0.30);
        assert!(d
            .left
            .iter()
            .any(|l| l.kind == LineKind::Modified));
        let r = d
            .right
            .iter()
            .find(|l| l.kind == LineKind::Modified)
            .unwrap();
        assert!(r.segments.iter().any(|s| s.kind == SegmentKind::Changed));
    }

    #[test]
    fn standalone_inserts_have_no_pair() {
        let d = Diff::compute("a\nc\n", "a\nb\nc\n", 0.30);
        let standalone: Vec<_> = d
            .right
            .iter()
            .filter(|l| l.kind == LineKind::Standalone)
            .collect();
        assert_eq!(standalone.len(), 1);
    }

    #[test]
    fn change_segment_span_is_max_l_r() {
        // 3 deletes vs 24 inserts -> change segment with span = 24.
        let mut left = String::from("eq_top\n");
        for i in 0..3 {
            left.push_str(&format!("L{i}\n"));
        }
        left.push_str("eq_bot\n");
        let mut right = String::from("eq_top\n");
        for i in 0..24 {
            right.push_str(&format!("R{i}\n"));
        }
        right.push_str("eq_bot\n");
        let d = Diff::compute(&left, &right, 0.30);
        let change = d.segments.iter().find(|s| s.is_change).unwrap();
        assert_eq!(change.l_count, 3);
        assert_eq!(change.r_count, 24);
        assert_eq!(change.span, 24);
        // Snap-aligned model: left advances 1:1 through its 3 change lines,
        // then clamps at D_last. Right advances 1:1 throughout. At the
        // segment boundary both snap by +1 to the next equal anchor.
        let v_change_start = d.segments[0].span; // 1 (after eq_top)
        let (l0, r0) = d.map(v_change_start);
        let (l2, r2) = d.map(v_change_start + 2); // last change row on left
        let (l_clamp, r_clamp) = d.map(v_change_start + 7); // left clamped, right ahead
        let (l_end_change, r_end_change) = d.map(v_change_start + 23); // last in change seg
        let (l_after, r_after) = d.map(v_change_start + 24); // first of next equal — SNAP

        // Inside change, both advance 1:1 for the first L=3.
        assert_eq!(r2 - r0, 2);
        assert_eq!(l2 - l0, 2);
        // After L scrolls, left clamps at D_last while right keeps going.
        assert_eq!(l_clamp, l2);
        assert!(r_clamp > r2);
        // At the very last virtual position in the change segment, left is
        // still clamped at D_last.
        assert_eq!(l_end_change, l2);
        // Snap: at the next virtual position (next equal segment), both panes
        // advance by exactly 1 to the next equal anchor — same content on
        // both sides.
        assert_eq!(l_after, l_end_change + 1);
        assert_eq!(r_after, r_end_change + 1);
        assert_eq!(d.left[l_after].kind, LineKind::Equal);
        assert_eq!(d.right[r_after].kind, LineKind::Equal);
    }

    #[test]
    fn equal_segment_advances_one_to_one() {
        let d = Diff::compute("a\nb\nc\n", "a\nb\nc\n", 0.30);
        let (l0, r0) = d.map(0);
        let (l1, r1) = d.map(1);
        let (l2, r2) = d.map(2);
        assert_eq!((l0, r0), (0, 0));
        assert_eq!((l1, r1), (1, 1));
        assert_eq!((l2, r2), (2, 2));
    }
}
