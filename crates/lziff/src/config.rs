//! Runtime configuration: theme, keymap, layout, behavior, i18n.
//!
//! Loaded from `$XDG_CONFIG_HOME/lziff/config.toml` (or platform equivalent)
//! at startup. Anything missing in the TOML falls back to a built-in default,
//! so users without a config file get a sensible experience and partial
//! configs are valid. Hex colors in TOML use `#rrggbb` strings.
//!
//! Structure-wise we read into an "overlay" type whose every field is
//! optional, then merge it onto a fully-populated `Config::default()` —
//! that way each field has a single source of truth.

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::style::Color;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Quit,
    ToggleHelp,
    CloseHelp,
    ToggleFocus,
    ToggleFilesPanel,
    Reload,
    NextFile,
    PrevFile,
    ScrollDown,
    ScrollUp,
    NextHunk,
    PrevHunk,
    HalfPageDown,
    HalfPageUp,
    Top,
    Bottom,
    Resnap,
    FocusFiles,
    FocusDiff,
    EnterDiff,
    /// Open the comment-input modal anchored at the cursor row.
    /// No-op when the host wasn't started in `--review` mode.
    OpenComment,
    /// Open the submit-review modal (verdict + body + drafts).
    OpenSubmit,
}

#[derive(Debug, Clone, Copy)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl KeyBinding {
    pub fn matches(&self, code: KeyCode, mods: KeyModifiers) -> bool {
        // Ignore extra modifier bits that crossterm sometimes sets (kitty
        // protocol etc.) — we care only about Shift/Ctrl/Alt.
        let m = mods & (KeyModifiers::SHIFT | KeyModifiers::CONTROL | KeyModifiers::ALT);
        self.code == code && self.mods == m
    }
}

#[derive(Debug, Clone)]
pub struct Keymap {
    pub bindings: Vec<(KeyBinding, Action)>,
}

impl Keymap {
    pub fn lookup(&self, code: KeyCode, mods: KeyModifiers) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(b, _)| b.matches(code, mods))
            .map(|(_, a)| *a)
    }

    fn default() -> Self {
        let b = |s: &str, a: Action| (parse_keybinding(s).unwrap(), a);
        Self {
            bindings: vec![
                b("q", Action::Quit),
                b("?", Action::ToggleHelp),
                b("Esc", Action::CloseHelp),
                b("Tab", Action::ToggleFocus),
                b("F", Action::ToggleFilesPanel),
                b("r", Action::Reload),
                b("n", Action::NextFile),
                b("p", Action::PrevFile),
                b("=", Action::Resnap),
                b("j", Action::ScrollDown),
                b("Down", Action::ScrollDown),
                b("k", Action::ScrollUp),
                b("Up", Action::ScrollUp),
                b("J", Action::NextHunk),
                b("K", Action::PrevHunk),
                b("ctrl-d", Action::HalfPageDown),
                b("ctrl-u", Action::HalfPageUp),
                b("g", Action::Top),
                b("G", Action::Bottom),
                b("c", Action::OpenComment),
                b("S", Action::OpenSubmit),
                b("Enter", Action::EnterDiff),
                b("h", Action::FocusFiles),
                b("Left", Action::FocusFiles),
                b("l", Action::FocusDiff),
                b("Right", Action::FocusDiff),
            ],
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub bg_delete: Color,
    pub bg_insert: Color,
    pub bg_mod_left: Color,
    pub bg_mod_right: Color,
    pub hl_delete: Color,
    pub hl_insert: Color,
    pub fg_gutter: Color,
    pub anchor_bright: Color,
    pub anchor_soft: Color,
    pub track_bg_left: Color,
    pub track_bg_right: Color,
    pub border_left_focus: Color,
    pub border_left_dim: Color,
    pub border_right_focus: Color,
    pub border_right_dim: Color,
    pub change_anchor_bright_left: Color,
    pub change_anchor_dim_left: Color,
    pub change_anchor_bright_right: Color,
    pub change_anchor_dim_right: Color,
    pub line_bright: Color,
    pub line_dim: Color,
    pub change_line_bright: Color,
    pub change_line_dim: Color,
    pub alignment_overlay: Color,
    pub title_bg: Color,
    pub title_fg: Color,
    pub title_old: Color,
    pub title_new: Color,
    pub fg_standalone_left: Color,
    pub fg_standalone_right: Color,
    pub diff_focus: Color,
    pub diff_dim: Color,
    pub list_focus_bg: Color,
    pub list_dim_bg: Color,
    pub help_section_fg: Color,
    pub help_keys_fg: Color,
    pub help_desc_fg: Color,
    pub help_border_fg: Color,
    pub help_panel_bg: Color,
    pub status_conflict_fg: Color,
}

impl Theme {
    fn default() -> Self {
        Self {
            bg_delete: Color::Rgb(48, 22, 26),
            bg_insert: Color::Rgb(20, 44, 26),
            bg_mod_left: Color::Rgb(48, 22, 26),
            bg_mod_right: Color::Rgb(20, 44, 26),
            hl_delete: Color::Rgb(150, 60, 70),
            hl_insert: Color::Rgb(50, 130, 70),
            fg_gutter: Color::Rgb(120, 120, 120),
            anchor_bright: Color::Rgb(245, 245, 250),
            anchor_soft: Color::Rgb(190, 195, 210),
            track_bg_left: Color::Rgb(60, 28, 32),
            track_bg_right: Color::Rgb(26, 50, 32),
            border_left_focus: Color::Rgb(220, 100, 110),
            border_left_dim: Color::Rgb(140, 70, 80),
            border_right_focus: Color::Rgb(110, 200, 130),
            border_right_dim: Color::Rgb(70, 130, 90),
            change_anchor_bright_left: Color::Rgb(230, 110, 120),
            change_anchor_dim_left: Color::Rgb(170, 80, 90),
            change_anchor_bright_right: Color::Rgb(120, 210, 140),
            change_anchor_dim_right: Color::Rgb(80, 160, 100),
            line_bright: Color::Rgb(245, 245, 250),
            line_dim: Color::Rgb(190, 195, 210),
            change_line_bright: Color::Rgb(190, 170, 120),
            change_line_dim: Color::Rgb(140, 130, 90),
            alignment_overlay: Color::Rgb(40, 50, 70),
            title_bg: Color::Rgb(60, 90, 140),
            title_fg: Color::White,
            title_old: Color::Rgb(200, 120, 120),
            title_new: Color::Rgb(120, 200, 140),
            fg_standalone_left: Color::Rgb(230, 190, 195),
            fg_standalone_right: Color::Rgb(190, 230, 195),
            diff_focus: Color::Rgb(120, 160, 220),
            diff_dim: Color::DarkGray,
            list_focus_bg: Color::Rgb(50, 70, 100),
            list_dim_bg: Color::Rgb(50, 50, 50),
            help_section_fg: Color::Rgb(225, 200, 130),
            help_keys_fg: Color::Rgb(170, 210, 245),
            help_desc_fg: Color::Rgb(210, 215, 220),
            help_border_fg: Color::Rgb(120, 160, 220),
            help_panel_bg: Color::Rgb(20, 24, 32),
            status_conflict_fg: Color::Rgb(245, 130, 80),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LayoutConfig {
    pub files_panel_width_pct: u16,
    pub ribbon_width: u16,
    /// Default cursor row as 1/N of viewport height. e.g. 3 means cursor
    /// initially sits at viewport_h / 3.
    pub target_y_divisor: usize,
}

impl LayoutConfig {
    fn default() -> Self {
        Self {
            files_panel_width_pct: 20,
            ribbon_width: 5,
            target_y_divisor: 3,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BehaviorConfig {
    pub tick_ms: u64,
    pub scroll_step: usize,
    pub half_page_step: usize,
    /// Token-level Jaccard threshold below which a Modified pair drops the
    /// per-token highlight (the lines are treated as fully changed). Stops
    /// the Christmas-tree effect on unrelated pairs.
    pub similarity_threshold: f32,
}

impl BehaviorConfig {
    fn default() -> Self {
        Self {
            tick_ms: 250,
            scroll_step: 3,
            half_page_step: 15,
            similarity_threshold: 0.30,
        }
    }
}

#[derive(Debug, Clone)]
pub struct I18nConfig {
    pub lang: String,
}

impl I18nConfig {
    fn default() -> Self {
        let lang = std::env::var("LZIFF_LANG")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        let lang = if lang.starts_with("ja") { "ja" } else { "en" };
        Self {
            lang: lang.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub theme: Theme,
    pub keymap: Keymap,
    pub layout: LayoutConfig,
    pub behavior: BehaviorConfig,
    pub i18n: I18nConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            keymap: Keymap::default(),
            layout: LayoutConfig::default(),
            behavior: BehaviorConfig::default(),
            i18n: I18nConfig::default(),
        }
    }
}

impl Config {
    /// Look for a `config.toml` under `$XDG_CONFIG_HOME/lziff/` (or the
    /// platform-specific config dir) and overlay it onto the defaults.
    /// Missing file or parse error → defaults silently.
    pub fn load() -> Self {
        Self::load_inner().unwrap_or_default()
    }

    fn load_inner() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let raw: RawConfig = toml::from_str(&text)
            .with_context(|| format!("parse {}", path.display()))?;
        Ok(raw.apply(Self::default()))
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir().ok_or_else(|| anyhow::anyhow!("config dir unavailable"))?;
    Ok(dir.join("lziff").join("config.toml"))
}

// ---------------------------------------------------------------------------
// TOML overlay types: every field optional, applied onto the defaults.

#[derive(Debug, Default, Deserialize)]
struct RawConfig {
    #[serde(default)]
    theme: RawTheme,
    #[serde(default)]
    keymap: Option<toml::Table>,
    #[serde(default)]
    layout: RawLayout,
    #[serde(default)]
    behavior: RawBehavior,
    #[serde(default)]
    i18n: RawI18n,
}

impl RawConfig {
    fn apply(self, mut cfg: Config) -> Config {
        self.theme.apply(&mut cfg.theme);
        if let Some(map) = self.keymap {
            apply_keymap(map, &mut cfg.keymap);
        }
        self.layout.apply(&mut cfg.layout);
        self.behavior.apply(&mut cfg.behavior);
        self.i18n.apply(&mut cfg.i18n);
        cfg
    }
}

macro_rules! raw_color_struct {
    ($name:ident { $($field:ident),* $(,)? }) => {
        #[derive(Debug, Default, Deserialize)]
        struct $name {
            $( #[serde(default)] $field: Option<HexColor>, )*
        }
        impl $name {
            fn apply(self, t: &mut Theme) {
                $( if let Some(c) = self.$field { t.$field = c.0; } )*
            }
        }
    };
}

raw_color_struct!(RawTheme {
    bg_delete,
    bg_insert,
    bg_mod_left,
    bg_mod_right,
    hl_delete,
    hl_insert,
    fg_gutter,
    anchor_bright,
    anchor_soft,
    track_bg_left,
    track_bg_right,
    border_left_focus,
    border_left_dim,
    border_right_focus,
    border_right_dim,
    change_anchor_bright_left,
    change_anchor_dim_left,
    change_anchor_bright_right,
    change_anchor_dim_right,
    line_bright,
    line_dim,
    change_line_bright,
    change_line_dim,
    alignment_overlay,
    title_bg,
    title_fg,
    title_old,
    title_new,
    fg_standalone_left,
    fg_standalone_right,
    diff_focus,
    diff_dim,
    list_focus_bg,
    list_dim_bg,
    help_section_fg,
    help_keys_fg,
    help_desc_fg,
    help_border_fg,
    help_panel_bg,
    status_conflict_fg,
});

#[derive(Debug, Default, Deserialize)]
struct RawLayout {
    #[serde(default)]
    files_panel_width_pct: Option<u16>,
    #[serde(default)]
    ribbon_width: Option<u16>,
    #[serde(default)]
    target_y_divisor: Option<usize>,
}

impl RawLayout {
    fn apply(self, l: &mut LayoutConfig) {
        if let Some(v) = self.files_panel_width_pct {
            l.files_panel_width_pct = v;
        }
        if let Some(v) = self.ribbon_width {
            l.ribbon_width = v;
        }
        if let Some(v) = self.target_y_divisor {
            l.target_y_divisor = v;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawBehavior {
    #[serde(default)]
    tick_ms: Option<u64>,
    #[serde(default)]
    scroll_step: Option<usize>,
    #[serde(default)]
    half_page_step: Option<usize>,
    #[serde(default)]
    similarity_threshold: Option<f32>,
}

impl RawBehavior {
    fn apply(self, b: &mut BehaviorConfig) {
        if let Some(v) = self.tick_ms {
            b.tick_ms = v;
        }
        if let Some(v) = self.scroll_step {
            b.scroll_step = v;
        }
        if let Some(v) = self.half_page_step {
            b.half_page_step = v;
        }
        if let Some(v) = self.similarity_threshold {
            b.similarity_threshold = v;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawI18n {
    #[serde(default)]
    lang: Option<String>,
}

impl RawI18n {
    fn apply(self, i: &mut I18nConfig) {
        if let Some(v) = self.lang {
            i.lang = v;
        }
    }
}

/// `keymap` is a free-form TOML table mapping action names ("scroll_down") to
/// either a single key string ("j") or an array of key strings (["j", "Down"]).
/// Anything in the table replaces the *whole* binding list for that action,
/// so users can override by listing only what they want.
fn apply_keymap(map: toml::Table, keymap: &mut Keymap) {
    let mut overrides: std::collections::HashMap<Action, Vec<KeyBinding>> =
        std::collections::HashMap::new();
    for (key, value) in map {
        let Some(action) = parse_action(&key) else {
            continue;
        };
        let strings: Vec<String> = match value {
            toml::Value::String(s) => vec![s],
            toml::Value::Array(arr) => arr
                .into_iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => continue,
        };
        let parsed: Vec<KeyBinding> = strings.iter().filter_map(|s| parse_keybinding(s)).collect();
        if !parsed.is_empty() {
            overrides.insert(action, parsed);
        }
    }
    // Strategy:
    // 1. Drop every existing binding whose action is being overridden.
    // 2. Drop every existing binding whose *key* now belongs to a new
    //    binding — otherwise an old `Esc → CloseHelp` would still shadow a
    //    user's `Esc → Quit`.
    // 3. Append the new bindings.
    let overridden_actions: std::collections::HashSet<Action> = overrides.keys().copied().collect();
    keymap
        .bindings
        .retain(|(_, a)| !overridden_actions.contains(a));
    let new_keys: std::collections::HashSet<(KeyCode, KeyModifiers)> = overrides
        .values()
        .flat_map(|v| v.iter().map(|kb| (kb.code, kb.mods)))
        .collect();
    keymap
        .bindings
        .retain(|(b, _)| !new_keys.contains(&(b.code, b.mods)));
    for (action, bindings) in overrides {
        for b in bindings {
            keymap.bindings.push((b, action));
        }
    }
}

fn parse_action(s: &str) -> Option<Action> {
    Some(match s {
        "quit" => Action::Quit,
        "toggle_help" => Action::ToggleHelp,
        "close_help" => Action::CloseHelp,
        "toggle_focus" => Action::ToggleFocus,
        "toggle_files_panel" => Action::ToggleFilesPanel,
        "reload" => Action::Reload,
        "next_file" => Action::NextFile,
        "prev_file" => Action::PrevFile,
        "scroll_down" => Action::ScrollDown,
        "scroll_up" => Action::ScrollUp,
        "next_hunk" => Action::NextHunk,
        "prev_hunk" => Action::PrevHunk,
        "half_page_down" => Action::HalfPageDown,
        "half_page_up" => Action::HalfPageUp,
        "top" => Action::Top,
        "bottom" => Action::Bottom,
        "resnap" => Action::Resnap,
        "focus_files" => Action::FocusFiles,
        "focus_diff" => Action::FocusDiff,
        "enter_diff" => Action::EnterDiff,
        "open_comment" => Action::OpenComment,
        "open_submit" => Action::OpenSubmit,
        _ => return None,
    })
}

/// Parse a key string like "j", "ctrl-d", "shift-Tab", "Up".
fn parse_keybinding(s: &str) -> Option<KeyBinding> {
    let mut mods = KeyModifiers::empty();
    let mut last = s;
    while let Some((prefix, rest)) = last.split_once('-') {
        match prefix.to_ascii_lowercase().as_str() {
            "ctrl" | "c" => mods |= KeyModifiers::CONTROL,
            "shift" | "s" => mods |= KeyModifiers::SHIFT,
            "alt" | "meta" | "a" | "m" => mods |= KeyModifiers::ALT,
            _ => break,
        }
        last = rest;
    }
    let code = parse_keycode(last)?;
    // Single-char letters with explicit case already imply shift; don't
    // double-set it for "shift-J", just normalize.
    if let KeyCode::Char(c) = code {
        if c.is_ascii_uppercase() {
            mods |= KeyModifiers::SHIFT;
        }
    }
    Some(KeyBinding { code, mods })
}

fn parse_keycode(s: &str) -> Option<KeyCode> {
    Some(match s {
        "Enter" | "Return" => KeyCode::Enter,
        "Esc" | "Escape" => KeyCode::Esc,
        "Tab" => KeyCode::Tab,
        "BackTab" => KeyCode::BackTab,
        "Up" => KeyCode::Up,
        "Down" => KeyCode::Down,
        "Left" => KeyCode::Left,
        "Right" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" | "PgUp" => KeyCode::PageUp,
        "PageDown" | "PgDn" => KeyCode::PageDown,
        "Backspace" => KeyCode::Backspace,
        "Delete" | "Del" => KeyCode::Delete,
        "Insert" | "Ins" => KeyCode::Insert,
        "Space" => KeyCode::Char(' '),
        s if s.starts_with('F') && s.len() > 1 => {
            let n: u8 = s[1..].parse().ok()?;
            KeyCode::F(n)
        }
        s => {
            let mut chars = s.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            KeyCode::Char(c)
        }
    })
}

// `serde::Deserialize` for hex colors via newtype.
#[derive(Debug, Clone, Copy)]
struct HexColor(Color);

impl<'de> Deserialize<'de> for HexColor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        parse_hex_color(&s)
            .map(HexColor)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid color: {s}")))
    }
}

fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_load_cleanly() {
        let cfg = Config::default();
        assert!(cfg.keymap.bindings.iter().any(|(_, a)| *a == Action::Quit));
    }

    #[test]
    fn parses_named_keys() {
        let kb = parse_keybinding("ctrl-d").unwrap();
        assert!(kb.mods.contains(KeyModifiers::CONTROL));
        assert_eq!(kb.code, KeyCode::Char('d'));

        let kb = parse_keybinding("Up").unwrap();
        assert_eq!(kb.code, KeyCode::Up);
    }

    #[test]
    fn shifted_uppercase_implies_shift() {
        let kb = parse_keybinding("J").unwrap();
        assert!(kb.mods.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn hex_color_parses() {
        assert_eq!(parse_hex_color("#102030"), Some(Color::Rgb(16, 32, 48)));
        assert_eq!(parse_hex_color("ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert!(parse_hex_color("#abc").is_none());
    }

    #[test]
    fn toml_overlay_applies() {
        // Note the doubled `##` so raw-string delimiters don't clash with
        // the `"#112233"` hex color value embedded inside.
        let toml = r##"
            [theme]
            bg_delete = "#112233"

            [behavior]
            scroll_step = 7
            similarity_threshold = 0.5

            [keymap]
            quit = ["q", "Esc"]
        "##;
        let raw: RawConfig = toml::from_str(toml).unwrap();
        let cfg = raw.apply(Config::default());
        assert_eq!(cfg.theme.bg_delete, Color::Rgb(0x11, 0x22, 0x33));
        assert_eq!(cfg.behavior.scroll_step, 7);
        assert!((cfg.behavior.similarity_threshold - 0.5).abs() < 1e-6);
        // Quit override replaces both old bindings; "q" still works, "Esc" added.
        assert!(cfg
            .keymap
            .lookup(KeyCode::Esc, KeyModifiers::empty())
            .map(|a| a == Action::Quit)
            .unwrap_or(false));
    }
}
