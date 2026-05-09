//! User-facing strings, separated from the rendering code so we can swap
//! languages and (later) load them from external files.
//!
//! The struct fields are owned `String`s rather than `&'static str` so that
//! plugin/config layers can override individual messages without rebuilding.

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
    pub help_header: String,
    pub help_lines: Vec<String>,
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
            title_files_panel: " Files ".into(),
            title_help: " Help ".into(),
            status_no_changes: "no changes".into(),
            status_no_diff: "(no diff)".into(),
            status_auto_reloaded: "auto-reloaded".into(),
            status_hint_default:
                "j/k scroll  J/K hunk  click select+snap  =  resync  n/p file  Tab focus  ? help  q quit"
                    .into(),
            status_hint_help_open: "press ? to close help".into(),
            help_header: "  Keys".into(),
            help_lines: vec![
                "  j / k       scroll both panes 1:1".into(),
                "  J / K       jump to next / prev change hunk".into(),
                "  ctrl-d/u    half-page down / up".into(),
                "  =           re-snap non-anchor pane to alignment row".into(),
                "  click       select that line; the *other* pane snaps".into(),
                "  wheel       scroll pane under pointer".into(),
                "  n / p       next / prev file".into(),
                "  Tab         toggle focus (Files / Diff)".into(),
                "  r           manual reload".into(),
                "  ? / esc     toggle / close this help".into(),
                "  q          quit".into(),
            ],
        }
    }

    pub fn japanese() -> Self {
        Self {
            title_old_prefix: " [旧] ".into(),
            title_new_prefix: " [新] ".into(),
            title_files_panel: " ファイル ".into(),
            title_help: " ヘルプ ".into(),
            status_no_changes: "差分なし".into(),
            status_no_diff: "(差分なし)".into(),
            status_auto_reloaded: "自動再読込".into(),
            status_hint_default:
                "j/k スクロール  J/K ハンク  クリック選択  =  再同期  n/p ファイル  Tab フォーカス  ? ヘルプ  q 終了"
                    .into(),
            status_hint_help_open: "? でヘルプを閉じる".into(),
            help_header: "  キー".into(),
            help_lines: vec![
                "  j / k       両ペインを 1:1 でスクロール".into(),
                "  J / K       次 / 前の変更ブロックへ".into(),
                "  ctrl-d/u    半ページ 上 / 下".into(),
                "  =           非アンカー側を整列行に再同期".into(),
                "  click       その行を選択、反対ペインがスナップ".into(),
                "  wheel       ポインタ下のペインをスクロール".into(),
                "  n / p       次 / 前のファイル".into(),
                "  Tab         フォーカス切替 (ファイル / Diff)".into(),
                "  r           手動再読込".into(),
                "  ? / esc     ヘルプの開閉".into(),
                "  q          終了".into(),
            ],
        }
    }
}
