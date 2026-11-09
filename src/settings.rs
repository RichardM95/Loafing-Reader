use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Style {
    pub family: String,
    pub size: f32,
    pub weight: i32,
    pub italic: bool,
    pub spacing: f32,
    pub line_height: f32,
    pub foreground: u32,
    pub background: u32,
    pub page_numbers: bool,
}
impl Default for Style {
    fn default() -> Self {
        Self {
            family: "Microsoft YaHei".into(),
            size: 14.,
            weight: 400,
            italic: false,
            spacing: 0.,
            line_height: 1.,
            foreground: 0xffffffff,
            background: 0x80000000,
            page_numbers: true,
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Memo {
    pub offset: usize,
    pub rule: String,
    pub encoding: String,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
impl Default for Rect {
    fn default() -> Self {
        Self {
            x: 356,
            y: 429,
            width: 856,
            height: 47,
        }
    }
}
impl Rect {
    pub fn fit(self, work: Rect) -> Rect {
        let width = self.width.clamp(1, work.width.max(1));
        let height = self.height.clamp(1, work.height.max(1));
        Rect {
            width,
            height,
            x: self.x.clamp(work.x, work.x + work.width - width),
            y: self.y.clamp(work.y, work.y + work.height - height),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub style: Style,
    pub rect: Rect,
    pub monitor: String,
    pub placements: BTreeMap<String, Rect>,
    pub current: String,
    pub books: BTreeMap<String, Memo>,
    pub fonts: Vec<String>,
    pub hotkeys: Vec<String>,
    pub seconds: u32,
    pub mouse: bool,
    pub camouflage: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            style: Style::default(),
            rect: Rect::default(),
            monitor: String::new(),
            placements: BTreeMap::new(),
            current: String::new(),
            books: BTreeMap::new(),
            fonts: Vec::new(),
            hotkeys: [
                "Ctrl+Alt+,",
                "Ctrl+Alt+.",
                "Ctrl+Alt+M",
                "Ctrl+Alt+P",
                "Ctrl+Alt+R",
            ]
            .map(str::to_owned)
            .to_vec(),
            seconds: 5,
            mouse: false,
            camouflage: "Hello".into(),
        }
    }
}
pub fn data_dir() -> PathBuf {
    std::env::var_os("THIEF_READER_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("Thief-Reader")
        })
}
pub fn load(dir: &Path) -> Result<Settings, String> {
    let path = dir.join("settings.json");
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = std::fs::read(&path).map_err(|e| e.to_string())?;
    serde_json::from_slice(&data).map_err(|e| format!("配置文件读取失败 {}，原文件保留", e))
}
pub fn save(dir: &Path, settings: &Settings) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = dir.join("settings.json");
    let temporary = dir.join("settings.json.tmp");
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    use std::io::Write;
    let mut file = std::fs::File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(temporary, target).map_err(|e| e.to_string())
}
pub fn parse_color(text: &str) -> Option<u32> {
    let text = text.trim().trim_start_matches('#');
    if let Some(rgba) = text.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let n: Vec<f32> = rgba
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if n.len() == 4 {
            return Some(
                ((n[3].clamp(0., 1.) * 255.).round() as u32) << 24
                    | (n[0] as u32) << 16
                    | (n[1] as u32) << 8
                    | n[2] as u32,
            );
        }
    }
    match text.len() {
        3 => u32::from_str_radix(text, 16).ok().map(|x| {
            0xff000000
                | (((x >> 8) & 15) * 0x110000)
                | (((x >> 4) & 15) * 0x1100)
                | ((x & 15) * 0x11)
        }),
        6 => u32::from_str_radix(text, 16).ok().map(|x| x | 0xff000000),
        8 => u32::from_str_radix(text, 16).ok(),
        _ => None,
    }
}
