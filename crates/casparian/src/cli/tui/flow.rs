//! TUI flow format and parsing.

use std::fmt;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiFlow {
    pub version: u32,
    #[serde(default)]
    pub env: FlowEnv,
    pub steps: Vec<FlowStep>,
}

impl TuiFlow {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!("Unsupported TUI flow version {}", self.version);
        }
        if self.steps.is_empty() {
            bail!("TUI flow must include at least one step");
        }
        for (idx, step) in self.steps.iter().enumerate() {
            step.validate()
                .with_context(|| format!("invalid step {}", idx))?;
        }
        self.env.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowEnv {
    #[serde(default)]
    pub casparian_home: Option<PathBuf>,
    #[serde(default)]
    pub database: Option<PathBuf>,
    #[serde(default)]
    pub terminal: Option<TerminalSize>,
    #[serde(default)]
    pub seed: Option<u64>,
    #[serde(default)]
    pub fixture: Option<FlowFixture>,
}

impl FlowEnv {
    fn validate(&self) -> Result<()> {
        if let Some(ref terminal) = self.terminal {
            terminal.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowFixture {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalSize {
    pub width: u16,
    pub height: u16,
}

impl TerminalSize {
    fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            bail!(
                "terminal size must be > 0 (got {}x{})",
                self.width,
                self.height
            );
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FlowStep {
    Key {
        key: FlowKey,
        #[serde(default)]
        label: Option<String>,
    },
    Text {
        text: String,
        #[serde(default)]
        label: Option<String>,
    },
    Wait {
        #[serde(default)]
        ticks: Option<u32>,
        #[serde(default)]
        ms: Option<u64>,
        #[serde(default)]
        until: Option<FlowAssertion>,
        #[serde(default)]
        label: Option<String>,
    },
    Assert {
        #[serde(flatten)]
        assert: FlowAssertion,
        #[serde(default)]
        label: Option<String>,
    },
}

impl FlowStep {
    fn validate(&self) -> Result<()> {
        match self {
            FlowStep::Key { .. } => Ok(()),
            FlowStep::Text { .. } => Ok(()),
            FlowStep::Wait {
                ticks, ms, until, ..
            } => {
                let has_wait = ticks.unwrap_or(0) > 0 || ms.unwrap_or(0) > 0;
                if !has_wait && until.as_ref().map(|a| a.is_empty()).unwrap_or(true) {
                    bail!("wait step must set ticks/ms or an until assertion");
                }
                if let Some(assert) = until {
                    if assert.is_empty() {
                        bail!("wait step until assertion cannot be empty");
                    }
                }
                Ok(())
            }
            FlowStep::Assert { assert, .. } => {
                if assert.is_empty() {
                    bail!("assert step must define at least one check");
                }
                Ok(())
            }
        }
    }

    pub fn label(&self) -> Option<&str> {
        match self {
            FlowStep::Key { label, .. }
            | FlowStep::Text { label, .. }
            | FlowStep::Wait { label, .. }
            | FlowStep::Assert { label, .. } => label.as_deref(),
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            FlowStep::Key { .. } => "key",
            FlowStep::Text { .. } => "text",
            FlowStep::Wait { .. } => "wait",
            FlowStep::Assert { .. } => "assert",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlowAssertion {
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub plain_contains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub plain_not_contains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub mask_contains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub mask_not_contains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub layout_contains: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_vec")]
    pub layout_not_contains: Vec<String>,
}

impl FlowAssertion {
    pub fn is_empty(&self) -> bool {
        self.plain_contains.is_empty()
            && self.plain_not_contains.is_empty()
            && self.mask_contains.is_empty()
            && self.mask_not_contains.is_empty()
            && self.layout_contains.is_empty()
            && self.layout_not_contains.is_empty()
    }
}

fn deserialize_string_or_vec<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrVec {
        One(String),
        Many(Vec<String>),
    }

    match StringOrVec::deserialize(deserializer)? {
        StringOrVec::One(value) => Ok(vec![value]),
        StringOrVec::Many(values) => Ok(values),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowKey {
    pub code: FlowKeyCode,
    pub modifiers: FlowModifiers,
}

impl FlowKey {
    pub fn to_key_event(&self) -> KeyEvent {
        KeyEvent::new(self.code.to_key_code(), self.modifiers.to_key_modifiers())
    }

    pub fn from_key_event(key: KeyEvent) -> Option<Self> {
        let mut code = FlowKeyCode::from_key_code(key.code)?;
        let mut modifiers = FlowModifiers::from_key_modifiers(key.modifiers);
        if matches!(code, FlowKeyCode::Tab) && modifiers.shift {
            code = FlowKeyCode::BackTab;
            modifiers.shift = false;
        }
        Some(Self { code, modifiers })
    }

    fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("key cannot be empty");
        }

        let mut modifiers = FlowModifiers::default();
        let mut key_token = trimmed;

        if trimmed.contains('+') || trimmed.contains('-') {
            let parts: Vec<&str> = trimmed.split(|c| c == '+' || c == '-').collect();
            if let Some((last, prefix)) = parts.split_last() {
                key_token = last;
                for part in prefix {
                    let token = part.trim().to_ascii_lowercase();
                    match token.as_str() {
                        "c" | "ctrl" | "control" => modifiers.ctrl = true,
                        "a" | "alt" => modifiers.alt = true,
                        "s" | "shift" => modifiers.shift = true,
                        "" => {}
                        _ => bail!("unknown key modifier '{}'", part),
                    }
                }
            }
        }

        let mut code = FlowKeyCode::parse(key_token)?;
        if matches!(code, FlowKeyCode::Tab) && modifiers.shift {
            code = FlowKeyCode::BackTab;
        }
        if let FlowKeyCode::Char(ch) = code {
            if modifiers.shift && ch.is_ascii_lowercase() {
                code = FlowKeyCode::Char(ch.to_ascii_uppercase());
            }
        }

        Ok(Self { code, modifiers })
    }
}

impl fmt::Display for FlowKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.modifiers.alt {
            parts.push("Alt".to_string());
        }
        if self.modifiers.shift {
            parts.push("Shift".to_string());
        }
        parts.push(self.code.to_string());
        write!(f, "{}", parts.join("+"))
    }
}

impl Serialize for FlowKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for FlowKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        FlowKey::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Default for FlowModifiers {
    fn default() -> Self {
        Self {
            ctrl: false,
            alt: false,
            shift: false,
        }
    }
}

impl FlowModifiers {
    fn to_key_modifiers(&self) -> KeyModifiers {
        let mut modifiers = KeyModifiers::NONE;
        if self.ctrl {
            modifiers |= KeyModifiers::CONTROL;
        }
        if self.alt {
            modifiers |= KeyModifiers::ALT;
        }
        if self.shift {
            modifiers |= KeyModifiers::SHIFT;
        }
        modifiers
    }

    fn from_key_modifiers(modifiers: KeyModifiers) -> Self {
        Self {
            ctrl: modifiers.contains(KeyModifiers::CONTROL),
            alt: modifiers.contains(KeyModifiers::ALT),
            shift: modifiers.contains(KeyModifiers::SHIFT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlowKeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Insert,
    F(u8),
}

impl FlowKeyCode {
    fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            bail!("key token cannot be empty");
        }
        if trimmed.len() == 1 {
            return Ok(FlowKeyCode::Char(trimmed.chars().next().unwrap_or(' ')));
        }
        let normalized = normalize_key_token(trimmed);
        let code = match normalized.as_str() {
            "enter" | "return" => FlowKeyCode::Enter,
            "esc" | "escape" => FlowKeyCode::Esc,
            "tab" => FlowKeyCode::Tab,
            "backtab" => FlowKeyCode::BackTab,
            "up" => FlowKeyCode::Up,
            "down" => FlowKeyCode::Down,
            "left" => FlowKeyCode::Left,
            "right" => FlowKeyCode::Right,
            "home" => FlowKeyCode::Home,
            "end" => FlowKeyCode::End,
            "pageup" | "pgup" => FlowKeyCode::PageUp,
            "pagedown" | "pgdn" => FlowKeyCode::PageDown,
            "backspace" | "bspace" => FlowKeyCode::Backspace,
            "delete" | "del" => FlowKeyCode::Delete,
            "insert" | "ins" => FlowKeyCode::Insert,
            "space" => FlowKeyCode::Char(' '),
            _ => {
                if normalized.starts_with('f') {
                    let digits = normalized.trim_start_matches('f');
                    if let Ok(value) = digits.parse::<u8>() {
                        if value >= 1 && value <= 24 {
                            return Ok(FlowKeyCode::F(value));
                        }
                    }
                }
                bail!("unknown key token '{}'", raw);
            }
        };
        Ok(code)
    }

    fn to_key_code(&self) -> KeyCode {
        match self {
            FlowKeyCode::Char(ch) => KeyCode::Char(*ch),
            FlowKeyCode::Enter => KeyCode::Enter,
            FlowKeyCode::Esc => KeyCode::Esc,
            FlowKeyCode::Tab => KeyCode::Tab,
            FlowKeyCode::BackTab => KeyCode::BackTab,
            FlowKeyCode::Up => KeyCode::Up,
            FlowKeyCode::Down => KeyCode::Down,
            FlowKeyCode::Left => KeyCode::Left,
            FlowKeyCode::Right => KeyCode::Right,
            FlowKeyCode::Home => KeyCode::Home,
            FlowKeyCode::End => KeyCode::End,
            FlowKeyCode::PageUp => KeyCode::PageUp,
            FlowKeyCode::PageDown => KeyCode::PageDown,
            FlowKeyCode::Backspace => KeyCode::Backspace,
            FlowKeyCode::Delete => KeyCode::Delete,
            FlowKeyCode::Insert => KeyCode::Insert,
            FlowKeyCode::F(idx) => KeyCode::F(*idx),
        }
    }

    fn from_key_code(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::Char(ch) => Some(FlowKeyCode::Char(ch)),
            KeyCode::Enter => Some(FlowKeyCode::Enter),
            KeyCode::Esc => Some(FlowKeyCode::Esc),
            KeyCode::Tab => Some(FlowKeyCode::Tab),
            KeyCode::BackTab => Some(FlowKeyCode::BackTab),
            KeyCode::Up => Some(FlowKeyCode::Up),
            KeyCode::Down => Some(FlowKeyCode::Down),
            KeyCode::Left => Some(FlowKeyCode::Left),
            KeyCode::Right => Some(FlowKeyCode::Right),
            KeyCode::Home => Some(FlowKeyCode::Home),
            KeyCode::End => Some(FlowKeyCode::End),
            KeyCode::PageUp => Some(FlowKeyCode::PageUp),
            KeyCode::PageDown => Some(FlowKeyCode::PageDown),
            KeyCode::Backspace => Some(FlowKeyCode::Backspace),
            KeyCode::Delete => Some(FlowKeyCode::Delete),
            KeyCode::Insert => Some(FlowKeyCode::Insert),
            KeyCode::F(idx) => Some(FlowKeyCode::F(idx)),
            _ => None,
        }
    }
}

impl fmt::Display for FlowKeyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlowKeyCode::Char(' ') => write!(f, "Space"),
            FlowKeyCode::Char(ch) => write!(f, "{}", ch),
            FlowKeyCode::Enter => write!(f, "Enter"),
            FlowKeyCode::Esc => write!(f, "Esc"),
            FlowKeyCode::Tab => write!(f, "Tab"),
            FlowKeyCode::BackTab => write!(f, "BackTab"),
            FlowKeyCode::Up => write!(f, "Up"),
            FlowKeyCode::Down => write!(f, "Down"),
            FlowKeyCode::Left => write!(f, "Left"),
            FlowKeyCode::Right => write!(f, "Right"),
            FlowKeyCode::Home => write!(f, "Home"),
            FlowKeyCode::End => write!(f, "End"),
            FlowKeyCode::PageUp => write!(f, "PageUp"),
            FlowKeyCode::PageDown => write!(f, "PageDown"),
            FlowKeyCode::Backspace => write!(f, "Backspace"),
            FlowKeyCode::Delete => write!(f, "Delete"),
            FlowKeyCode::Insert => write!(f, "Insert"),
            FlowKeyCode::F(idx) => write!(f, "F{}", idx),
        }
    }
}

fn normalize_key_token(raw: &str) -> String {
    raw.chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '-' && *ch != '_')
        .collect::<String>()
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flow_key_ctrl() {
        let key = FlowKey::parse("Ctrl+u").expect("parse ctrl+u");
        assert!(key.modifiers.ctrl);
        match key.code {
            FlowKeyCode::Char(ch) => assert_eq!(ch, 'u'),
            _ => panic!("expected char"),
        }
    }

    #[test]
    fn parse_flow_key_named() {
        let key = FlowKey::parse("Enter").expect("parse enter");
        assert!(matches!(key.code, FlowKeyCode::Enter));
    }

    #[test]
    fn parse_flow_key_backtab() {
        let key = FlowKey::parse("Shift+Tab").expect("parse shift+tab");
        assert!(matches!(key.code, FlowKeyCode::BackTab));
    }
}
