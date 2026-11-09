//! Keyboard recording and canonical Windows hotkey mapping, independent of windows and persistence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub modifiers: u32,
    pub vk: u32,
}
const NAMED: &[(&str, u32)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0d),
    ("Escape", 0x1b),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2d),
    ("Delete", 0x2e),
    ("Semicolon", 0xba),
    ("Plus", 0xbb),
    (",", 0xbc),
    ("Minus", 0xbd),
    (".", 0xbe),
    ("Slash", 0xbf),
    ("Backtick", 0xc0),
    ("LeftBracket", 0xdb),
    ("Backslash", 0xdc),
    ("RightBracket", 0xdd),
    ("Quote", 0xde),
];
fn key_name(vk: u32) -> Option<String> {
    if (0x30..=0x39).contains(&vk) || (0x41..=0x5a).contains(&vk) {
        return char::from_u32(vk).map(|c| c.to_string());
    }
    if (0x70..=0x87).contains(&vk) {
        return Some(format!("F{}", vk - 0x6f));
    }
    NAMED
        .iter()
        .find(|(_, v)| *v == vk)
        .map(|(name, _)| name.to_string())
}
pub fn parse(text: &str) -> Option<Hotkey> {
    let mut modifiers = 0;
    let mut vk = None;
    for token in text.split('+') {
        let token = token.trim();
        let modifier = match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => Some(2),
            "alt" => Some(1),
            "shift" => Some(4),
            "win" | "meta" => Some(8),
            _ => None,
        };
        if let Some(m) = modifier {
            modifiers |= m;
            continue;
        }
        if vk.is_some() {
            return None;
        }
        vk = if token.len() == 1 && token.as_bytes()[0].is_ascii_alphanumeric() {
            Some(token.as_bytes()[0].to_ascii_uppercase() as u32)
        } else if let Some(n) = token
            .to_ascii_uppercase()
            .strip_prefix('F')
            .and_then(|n| n.parse::<u32>().ok())
            .filter(|n| (1..=24).contains(n))
        {
            Some(0x6f + n)
        } else {
            NAMED
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(token))
                .map(|(_, v)| *v)
        };
        vk?;
    }
    Some(Hotkey { modifiers, vk: vk? })
}
fn prefix(modifiers: u32) -> Vec<String> {
    [(2, "Ctrl"), (1, "Alt"), (4, "Shift"), (8, "Win")]
        .iter()
        .filter(|(m, _)| modifiers & m != 0)
        .map(|(_, name)| name.to_string())
        .collect()
}
fn format(key: Hotkey) -> Option<String> {
    let mut parts = prefix(key.modifiers);
    parts.push(key_name(key.vk)?);
    Some(parts.join("+"))
}
pub fn canonical(text: &str) -> Option<String> {
    format(parse(text)?)
}
pub fn keycaps(text: &str) -> Vec<String> {
    text.split('+')
        .filter(|t| !t.is_empty())
        .map(|t| {
            match t {
                "Left" => "←",
                "Right" => "→",
                "Up" => "↑",
                "Down" => "↓",
                "Space" => "空格",
                "PageUp" => "PgUp",
                "PageDown" => "PgDn",
                "Plus" => "+",
                "Minus" => "-",
                "Semicolon" => ";",
                "Slash" => "/",
                "Backslash" => "\\",
                "Quote" => "'",
                "LeftBracket" => "[",
                "RightBracket" => "]",
                "Backtick" => "`",
                other => other,
            }
            .to_string()
        })
        .collect()
}
pub fn validate(keys: &[String]) -> Vec<String> {
    let parsed: Vec<_> = keys.iter().map(|s| parse(s)).collect();
    parsed
        .iter()
        .enumerate()
        .map(|(i, key)| {
            if key.is_none() {
                "请重新录制有效组合键".into()
            } else if parsed.iter().enumerate().any(|(j, k)| i != j && k == key) {
                "此组合键已分配给其他操作".into()
            } else {
                String::new()
            }
        })
        .collect()
}
fn event_vk(text: &str) -> Result<Option<u32>, String> {
    let mut chars = text.chars();
    let c = chars.next().ok_or("未识别到按键")?;
    if chars.next().is_some() {
        return Err("请按单个主键与修饰键的组合".into());
    }
    let v = c as u32;
    if (0x10..=0x18).contains(&v) {
        return Ok(None);
    }
    let named = match v {
        8 => Some(8),
        9 => Some(9),
        10 | 13 => Some(13),
        0x7f => Some(0x2e),
        0xf700 => Some(0x26),
        0xf701 => Some(0x28),
        0xf702 => Some(0x25),
        0xf703 => Some(0x27),
        0xf704..=0xf71b => Some(0x70 + v - 0xf704),
        0xf727 => Some(0x2d),
        0xf729 => Some(0x24),
        0xf72b => Some(0x23),
        0xf72c => Some(0x21),
        0xf72d => Some(0x22),
        _ => None,
    };
    if let Some(vk) = named {
        return Ok(Some(vk));
    }
    #[cfg(windows)]
    if v <= 0xffff {
        let scan = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW(v as u16) };
        if scan != -1 {
            let vk = (scan as u16 & 255) as u32;
            if key_name(vk).is_some() {
                return Ok(Some(vk));
            }
        }
    }
    Err("暂不支持此按键，请使用字母、数字、功能键或常用键".into())
}
#[derive(Default)]
pub struct Recorder {
    pub target: Option<usize>,
    pending: Option<String>,
}
impl Recorder {
    pub fn begin(&mut self, index: usize) {
        self.target = Some(index);
        self.pending = None;
    }
    pub fn cancel(&mut self) {
        self.target = None;
        self.pending = None;
    }
    pub fn press(&mut self, text: &str, modifiers: u32) -> Result<String, String> {
        if let Some(key) = &self.pending {
            return Ok(key.clone());
        }
        if self.target.is_none() {
            return Err("尚未开始录制".into());
        }
        let Some(vk) = event_vk(text)? else {
            return Ok(prefix(modifiers).join("+"));
        };
        let key = format(Hotkey { modifiers, vk }).ok_or("不支持此按键")?;
        self.pending = Some(key.clone());
        Ok(key)
    }
    pub fn release(&mut self) -> Option<(usize, String)> {
        let key = self.pending.take()?;
        let index = self.target.take()?;
        Some((index, key))
    }
}

/// Match a complete chord so M and Minus cannot share another row's error.
pub fn registration_error(errors: &[String], key: &str) -> String {
    errors
        .iter()
        .find(|error| {
            error
                .strip_prefix(key)
                .is_some_and(|suffix| suffix.starts_with(' '))
        })
        .cloned()
        .unwrap_or_default()
}
