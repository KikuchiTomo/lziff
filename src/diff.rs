//! Diff engine: produces an aligned, side-by-side diff with intra-line word highlighting.
//!
//! The data model is shaped for rendering, not for patch application: a [`Diff`] is a flat
//! sequence of [`Row`]s where left and right are aligned (filler rows are inserted on the
//! opposite side for pure insertions/deletions). This is what enables the JetBrains-style
//! side-by-side layout.

use similar::{Algorithm, ChangeTag, TextDiff};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Equal,
    Insert,
    Delete,
    Replace,
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

#[derive(Debug, Clone, Default)]
pub struct Side {
    /// 1-based line number; `None` means filler (no real line on this side).
    pub line_no: Option<usize>,
    /// Word-level segmentation. For `Equal` rows this is a single `Same` segment.
    /// For `Replace` rows this carries the intra-line highlight.
    /// For filler rows this is empty.
    pub segments: Vec<Segment>,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub kind: RowKind,
    pub left: Side,
    pub right: Side,
}

#[derive(Debug, Clone, Default)]
pub struct Diff {
    pub rows: Vec<Row>,
    /// Index ranges of contiguous non-Equal rows, useful for hunk navigation.
    pub hunks: Vec<(usize, usize)>,
}

impl Diff {
    pub fn compute(left_text: &str, right_text: &str) -> Self {
        let left_lines: Vec<&str> = split_lines(left_text);
        let right_lines: Vec<&str> = split_lines(right_text);

        let diff = TextDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(left_text, right_text);

        let mut rows: Vec<Row> = Vec::new();
        let mut left_no = 0usize;
        let mut right_no = 0usize;

        // Walk grouped ops so we can pair Delete+Insert runs into Replace rows for
        // intra-line highlighting (this is what gives the JetBrains feel).
        for op in diff.ops() {
            let mut pending_dels: Vec<(usize, &str)> = Vec::new();
            let mut pending_ins: Vec<(usize, &str)> = Vec::new();
            for change in diff.iter_changes(op) {
                match change.tag() {
                    ChangeTag::Equal => {
                        flush_pending(&mut rows, &mut pending_dels, &mut pending_ins);
                        left_no += 1;
                        right_no += 1;
                        let l = left_lines.get(left_no - 1).copied().unwrap_or("");
                        let r = right_lines.get(right_no - 1).copied().unwrap_or("");
                        rows.push(Row {
                            kind: RowKind::Equal,
                            left: Side {
                                line_no: Some(left_no),
                                segments: vec![Segment {
                                    text: l.to_string(),
                                    kind: SegmentKind::Same,
                                }],
                            },
                            right: Side {
                                line_no: Some(right_no),
                                segments: vec![Segment {
                                    text: r.to_string(),
                                    kind: SegmentKind::Same,
                                }],
                            },
                        });
                    }
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
                }
            }
            flush_pending(&mut rows, &mut pending_dels, &mut pending_ins);
        }

        let hunks = compute_hunks(&rows);
        Diff { rows, hunks }
    }
}

fn split_lines(s: &str) -> Vec<&str> {
    // Preserve empty trailing line semantics for line numbering: split_terminator keeps
    // numbering aligned with similar's line iterator behavior.
    s.split_terminator('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect()
}

/// Emit screen rows for a change block.
///
/// Each real source line appears exactly once. The first `min(L, R)` rows pair
/// a delete with an insert (Replace, with intra-line word diff when similar);
/// any leftover deletes or inserts emit one row each with filler on the
/// opposite side. The longer side keeps scrolling through real content while
/// the shorter side falls off into empty space — that's the asymmetric scroll
/// experience.
fn flush_pending(
    rows: &mut Vec<Row>,
    dels: &mut Vec<(usize, &str)>,
    ins: &mut Vec<(usize, &str)>,
) {
    let pair_count = dels.len().min(ins.len());
    for i in 0..pair_count {
        let (lno, l) = dels[i];
        let (rno, r) = ins[i];
        let (lsegs, rsegs) = if similarity(l, r) >= 0.30 {
            word_diff(l, r)
        } else {
            (
                vec![Segment {
                    text: l.to_string(),
                    kind: SegmentKind::Changed,
                }],
                vec![Segment {
                    text: r.to_string(),
                    kind: SegmentKind::Changed,
                }],
            )
        };
        rows.push(Row {
            kind: RowKind::Replace,
            left: Side {
                line_no: Some(lno),
                segments: lsegs,
            },
            right: Side {
                line_no: Some(rno),
                segments: rsegs,
            },
        });
    }
    for &(lno, l) in &dels[pair_count..] {
        rows.push(Row {
            kind: RowKind::Delete,
            left: Side {
                line_no: Some(lno),
                segments: vec![Segment {
                    text: l.to_string(),
                    kind: SegmentKind::Changed,
                }],
            },
            right: Side::default(),
        });
    }
    for &(rno, r) in &ins[pair_count..] {
        rows.push(Row {
            kind: RowKind::Insert,
            left: Side::default(),
            right: Side {
                line_no: Some(rno),
                segments: vec![Segment {
                    text: r.to_string(),
                    kind: SegmentKind::Changed,
                }],
            },
        });
    }
    dels.clear();
    ins.clear();
}

/// Token-level Jaccard similarity. Empty-vs-empty is 1.0; either-empty is 0.0.
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
    let mut shared = 0i32;
    let mut bcount: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    for t in &bt {
        *bcount.entry(t.as_str()).or_default() += 1;
    }
    for (k, &av) in &acount {
        if let Some(&bv) = bcount.get(k) {
            shared += av.min(bv);
        }
    }
    let denom = (at.len() + bt.len()) as f32;
    (2.0 * shared as f32) / denom
}

/// Intra-line diff at the word/grapheme level. Returns segments for left and right.
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

/// Token boundaries: word characters group together; whitespace groups; punctuation/symbol
/// graphemes are individual tokens. This matches editor expectations for "word diff".
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

fn compute_hunks(rows: &[Row]) -> Vec<(usize, usize)> {
    let mut hunks = Vec::new();
    let mut start: Option<usize> = None;
    for (i, row) in rows.iter().enumerate() {
        match (row.kind, start) {
            (RowKind::Equal, Some(s)) => {
                hunks.push((s, i));
                start = None;
            }
            (RowKind::Equal, None) => {}
            (_, None) => start = Some(i),
            (_, Some(_)) => {}
        }
    }
    if let Some(s) = start {
        hunks.push((s, rows.len()));
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_inputs_have_no_hunks() {
        let d = Diff::compute("a\nb\nc\n", "a\nb\nc\n");
        assert!(d.hunks.is_empty());
        assert_eq!(d.rows.len(), 3);
        assert!(d.rows.iter().all(|r| r.kind == RowKind::Equal));
    }

    #[test]
    fn pure_insert_has_filler_left() {
        let d = Diff::compute("a\nc\n", "a\nb\nc\n");
        let inserts: Vec<_> = d.rows.iter().filter(|r| r.kind == RowKind::Insert).collect();
        assert_eq!(inserts.len(), 1);
        assert!(inserts[0].left.line_no.is_none());
        let txt: String = inserts[0]
            .right
            .segments
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(txt, "b");
    }

    #[test]
    fn asymmetric_block_pairs_then_fills_with_inserts() {
        // 1 delete vs 4 inserts: 1 Replace pair + 3 pure Insert rows (filler-left).
        let d = Diff::compute("equal\nold\ntail\n", "equal\nnew1\nnew2\nnew3\nnew4\ntail\n");
        let block: Vec<_> = d
            .rows
            .iter()
            .filter(|r| r.kind != RowKind::Equal)
            .collect();
        assert_eq!(block.len(), 4);
        assert_eq!(block[0].kind, RowKind::Replace);
        for row in &block[1..] {
            assert_eq!(row.kind, RowKind::Insert);
            assert!(row.left.line_no.is_none());
            assert!(row.right.line_no.is_some());
        }
    }

    #[test]
    fn replace_carries_word_diff() {
        let d = Diff::compute("hello world\n", "hello brave world\n");
        let row = d.rows.iter().find(|r| r.kind == RowKind::Replace).unwrap();
        assert!(row.right.segments.iter().any(|s| s.kind == SegmentKind::Changed));
        assert!(row.left.segments.iter().any(|s| s.kind == SegmentKind::Same));
    }
}
