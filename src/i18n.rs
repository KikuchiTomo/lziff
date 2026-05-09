//! User-facing strings, separated from the rendering code so we can swap
//! languages and (later) load them from external files.
//!
//! Help is structured rather than a flat list of lines — the renderer can
//! lay out sections, key columns, and descriptions independently and the
//! translator never has to fight with whitespace alignment in a string.

#[derive(Debug, Clone)]
pub struct HelpEntry {
    pub keys: String,
    pub desc: String,
}

#[derive(Debug, Clone)]
pub struct HelpSection {
    pub title: String,
    pub entries: Vec<HelpEntry>,
}

#[derive(Debug, Clone)]
pub struct Strings {
    pub title_old_prefix: String,
    pub title_new_prefix: String,
    pub title_files_panel: String,
    pub title_help: String,
    /// Reserved for an empty-state message when the file list is empty.
    /// Not surfaced in the current renderer, but kept so plugin authors and
    /// localized config layers can wire it in.
    #[allow(dead_code)]
    pub status_no_changes: String,
    #[allow(dead_code)]
    pub status_no_diff: String,
    pub status_auto_reloaded: String,
    pub status_hint_default: String,
    pub status_hint_help_open: String,
    pub help_sections: Vec<HelpSection>,
}

impl Strings {
    pub fn for_lang(lang: &str) -> Self {
        match lang {
            "ja" | "ja-JP" | "ja_JP" => Self::japanese(),
            _ => Self::english(),
        }
    }

    pub fn english() -> Self {
        Self {
            title_old_prefix: " [Old] ".into(),
            title_new_prefix: " [New] ".into(),
            title_files_panel: " [Files] ".into(),
            title_help: " [Help] ".into(),
            status_no_changes: "no changes".into(),
            status_no_diff: "(no diff)".into(),
            status_auto_reloaded: "auto-reloaded".into(),
            status_hint_default:
                "j/k scroll  J/K hunk  click select+snap  =  resync  n/p file  Tab focus  ? help  q quit"
                    .into(),
            status_hint_help_open: "press ? to close help".into(),
            help_sections: vec![
                HelpSection {
                    title: "Navigation".into(),
                    entries: vec![
                        e("j / k", "scroll both panes 1:1"),
                        e("J / K", "jump to next / prev change hunk"),
                        e("ctrl-d / ctrl-u", "half-page down / up"),
                        e("g / G", "top / bottom"),
                        e("n / p", "next / prev file"),
                    ],
                },
                HelpSection {
                    title: "Alignment".into(),
                    entries: vec![
                        e("click", "select that line; the *other* pane snaps"),
                        e("wheel", "scroll pane under pointer"),
                        e("=", "re-snap non-anchor pane to alignment row"),
                    ],
                },
                HelpSection {
                    title: "Display".into(),
                    entries: vec![
                        e("Tab", "toggle focus (Files / Diff)"),
                        e("F", "show / hide the files panel"),
                        e("r", "manual reload"),
                    ],
                },
                HelpSection {
                    title: "App".into(),
                    entries: vec![
                        e("? / Esc", "toggle / close this help"),
                        e("q", "quit"),
                    ],
                },
            ],
        }
    }

    pub fn japanese() -> Self {
        Self {
            title_old_prefix: " [旧] ".into(),
            title_new_prefix: " [新] ".into(),
            title_files_panel: " [ファイル] ".into(),
            title_help: " [ヘルプ] ".into(),
            status_no_changes: "差分なし".into(),
            status_no_diff: "(差分なし)".into(),
            status_auto_reloaded: "自動再読込".into(),
            status_hint_default:
                "j/k スクロール  J/K ハンク  クリック選択  =  再同期  n/p ファイル  Tab フォーカス  ? ヘルプ  q 終了"
                    .into(),
            status_hint_help_open: "? でヘルプを閉じる".into(),
            help_sections: vec![
                HelpSection {
                    title: "ナビゲーション".into(),
                    entries: vec![
                        e("j / k", "両ペインを 1:1 でスクロール"),
                        e("J / K", "次 / 前の変更ブロックへ"),
                        e("ctrl-d / ctrl-u", "半ページ 上 / 下"),
                        e("g / G", "先頭 / 末尾"),
                        e("n / p", "次 / 前のファイル"),
                    ],
                },
                HelpSection {
                    title: "整列".into(),
                    entries: vec![
                        e("click", "その行を選択 — 反対ペインがスナップ"),
                        e("wheel", "ポインタ下のペインをスクロール"),
                        e("=", "非アンカー側を整列行に再同期"),
                    ],
                },
                HelpSection {
                    title: "表示".into(),
                    entries: vec![
                        e("Tab", "フォーカス切替 (ファイル / Diff)"),
                        e("F", "ファイルパネルの表示切替"),
                        e("r", "手動再読込"),
                    ],
                },
                HelpSection {
                    title: "アプリ".into(),
                    entries: vec![
                        e("? / Esc", "ヘルプの開閉"),
                        e("q", "終了"),
                    ],
                },
            ],
        }
    }
}

fn e(keys: &str, desc: &str) -> HelpEntry {
    HelpEntry {
        keys: keys.into(),
        desc: desc.into(),
    }
}
