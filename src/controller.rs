use crate::{
    AppWindow, ScreenBox,
    native::{self, Action, Reader, Screen, TextEngine},
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use thief_reader::{
    book::{self, Document},
    settings::{self, Rect, Settings},
};

pub struct Controller {
    ui: slint::Weak<AppWindow>,
    settings: Settings,
    directory: PathBuf,
    reader: Reader,
    engine: TextEngine,
    document: Option<Document>,
    offset: usize,
    end: usize,
    pages: Vec<usize>,
    history: Vec<usize>,
    rows: Vec<usize>,
    recent: Vec<String>,
    is_recent: bool,
    automatic: bool,
    screens: Vec<Screen>,
    load_generation: u64,
    search_generation: u64,
    layout_generation: Arc<AtomicU64>,
    save_timer: slint::Timer,
    index_timer: slint::Timer,
    layout_key: String,
}
fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}
impl Controller {
    pub fn new(ui: slint::Weak<AppWindow>) -> Result<Self, String> {
        let directory = settings::data_dir();
        let settings = settings::load(&directory)?;
        let reader = Reader::new(settings.rect).map_err(|e| e.to_string())?;
        let mut engine = TextEngine::new().map_err(|e| e.to_string())?;
        let families = engine
            .load_fonts(&settings.fonts)
            .map_err(|e| e.to_string())?;
        if let Some(ui) = ui.upgrade() {
            ui.set_font_families(strings(families));
        }
        Ok(Self {
            ui,
            settings,
            directory,
            reader,
            engine,
            document: None,
            offset: 0,
            end: 0,
            pages: vec![],
            history: vec![],
            rows: vec![],
            recent: vec![],
            is_recent: false,
            automatic: false,
            screens: vec![],
            load_generation: 0,
            search_generation: 0,
            layout_generation: Arc::new(AtomicU64::new(0)),
            save_timer: slint::Timer::default(),
            index_timer: slint::Timer::default(),
            layout_key: String::new(),
        })
    }
    fn status(&self, text: impl Into<SharedString>) {
        if let Some(ui) = self.ui.upgrade() {
            ui.set_status(text.into());
        }
    }
    fn register_hotkeys(&self) -> Vec<String> {
        let errors = self.reader.hotkeys(&self.settings.hotkeys);
        if let Some(ui) = self.ui.upgrade() {
            ui.set_key_errors(strings(self.settings.hotkeys.iter().map(|key| {
                errors
                    .iter()
                    .find(|error| error.starts_with(key))
                    .cloned()
                    .unwrap_or_default()
            })));
        }
        errors
    }
    pub fn startup(&mut self) {
        self.sync_style();
        self.refresh_screens(false);
        self.reader.mouse(self.settings.mouse);
        let errors = self.register_hotkeys();
        if !errors.is_empty() {
            self.status(errors.join("，"));
        }
        self.reader.show(true);
        self.draw();
        if !self.settings.current.is_empty() {
            self.open(PathBuf::from(&self.settings.current), None);
        }
    }
    fn sync_style(&self) {
        if let Some(ui) = self.ui.upgrade() {
            let s = &self.settings.style;
            ui.set_family(s.family.clone().into());
            ui.set_font_size(s.size as i32);
            ui.set_weight(s.weight);
            ui.set_italic(s.italic);
            ui.set_letter_spacing((s.spacing * 10.) as i32);
            ui.set_line_spacing((s.line_height * 100.) as i32);
            ui.set_foreground(format!("#{:08X}", s.foreground).into());
            ui.set_background_color(format!("#{:08X}", s.background).into());
            ui.set_multiline(s.multiline);
            ui.set_page_numbers(s.page_numbers);
            ui.set_mouse_mode(self.settings.mouse);
            ui.set_interval(self.settings.seconds as i32);
            ui.set_disguise(self.settings.camouflage.clone().into());
            let keys = &self.settings.hotkeys;
            if keys.len() >= 5 {
                ui.set_previous_key(keys[0].clone().into());
                ui.set_next_key(keys[1].clone().into());
                ui.set_boss_key(keys[2].clone().into());
                ui.set_auto_key(keys[3].clone().into());
                ui.set_rescue_key(keys[4].clone().into());
            }
        }
    }
    pub fn persist(&mut self) {
        if let Some(doc) = &self.document {
            self.settings
                .books
                .entry(doc.path.to_string_lossy().into_owned())
                .or_default()
                .offset = self.offset;
        }
        if let Err(e) = settings::save(&self.directory, &self.settings) {
            self.status(e);
        }
    }
    fn save_later(&self) {
        self.save_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(750),
            || {
                crate::APP.with(|app| {
                    if let Some(c) = app.borrow().as_ref() {
                        c.borrow_mut().persist();
                    }
                });
            },
        );
    }
    pub fn open(&mut self, path: PathBuf, encoding: Option<String>) {
        self.persist();
        self.load_generation += 1;
        self.search_generation += 1;
        let generation = self.load_generation;
        let key = path.to_string_lossy().into_owned();
        let memo = self.settings.books.get(&key).cloned().unwrap_or_default();
        let encoding = encoding.unwrap_or_else(|| {
            if memo.encoding.is_empty() {
                "auto".into()
            } else {
                memo.encoding.clone()
            }
        });
        self.status("正在读取与识别章节…");
        std::thread::spawn(move || {
            let result = book::open(&path, &encoding, Some(&memo.rule));
            let _ = slint::invoke_from_event_loop(move || {
                crate::APP.with(|app| {
                    if let Some(c) = app.borrow().as_ref() {
                        let mut c = c.borrow_mut();
                        if c.load_generation != generation {
                            return;
                        }
                        match result {
                            Ok(doc) => {
                                c.settings.current = doc.path.to_string_lossy().into_owned();
                                c.offset = book::boundary(&doc.text, memo.offset);
                                c.history.clear();
                                c.pages.clear();
                                c.document = Some(doc);
                                if let Some(ui) = c.ui.upgrade() {
                                    ui.set_rule(memo.rule.into());
                                    ui.set_encoding(encoding.into());
                                }
                                c.show_chapters();
                                c.draw();
                                c.index();
                                c.save_later();
                            }
                            Err(e) => c.status(e),
                        }
                    }
                })
            });
        });
    }
    pub fn draw(&mut self) {
        let rect = self.reader.rect();
        if rect.width < 1 || rect.height < 1 {
            return;
        }
        let dpi = self.reader.dpi();
        let display = if let Some(doc) = &self.document {
            let start = if self.pages.is_empty() {
                self.offset
            } else {
                self.pages[self
                    .pages
                    .partition_point(|x| *x <= self.offset)
                    .saturating_sub(1)]
            };
            let page = self.engine.page(
                &doc.text,
                start,
                &self.settings.style,
                rect.width as f32 / dpi,
                rect.height as f32 / dpi,
            );
            self.end = page.end;
            let footer = if self.settings.style.page_numbers {
                if self.pages.is_empty() {
                    "  …/…".into()
                } else {
                    format!(
                        "  {}/{}",
                        self.pages.partition_point(|x| *x <= self.offset).max(1),
                        self.pages.len()
                    )
                }
            } else {
                String::new()
            };
            if let Some(ui) = self.ui.upgrade() {
                ui.set_book_name(
                    doc.path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                        .into(),
                );
                ui.set_progress(self.offset as f32 / doc.text.len().max(1) as f32);
            }
            page.text + &footer
        } else {
            self.settings.camouflage.clone()
        };
        match self.engine.render_aligned(
            &display,
            &self.settings.style,
            rect.width as u32,
            rect.height as u32,
            dpi,
            self.document.is_none(),
        ) {
            Ok(pixels) => {
                if let Err(e) = self.reader.present(&pixels, rect.width, rect.height) {
                    self.status(e.to_string());
                }
                if let Some(ui) = self.ui.upgrade() {
                    let mut rgba = pixels;
                    for p in rgba.as_chunks_mut::<4>().0 {
                        p.swap(0, 2);
                        let a = p[3] as u32;
                        for color in &mut p[..3] {
                            *color =
                                (*color as u32 * 255).checked_div(a).unwrap_or(0).min(255) as u8;
                        }
                    }
                    let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                        &rgba,
                        rect.width as u32,
                        rect.height as u32,
                    );
                    ui.set_preview(slint::Image::from_rgba8(buffer));
                }
            }
            Err(e) => self.status(e.to_string()),
        }
    }
    fn index(&mut self) {
        let Some(doc) = self.document.clone() else {
            return;
        };
        let style = self.settings.style.clone();
        let fonts = self.settings.fonts.clone();
        let rect = self.reader.rect();
        let dpi = self.reader.dpi();
        let (width, height) = (rect.width as f32 / dpi, rect.height as f32 / dpi);
        let mut layout_style = style.clone();
        layout_style.foreground = 0;
        layout_style.background = 0;
        let cache_key = blake3::hash(
            format!(
                "v2/{}/{:?}/{width}/{height}/{fonts:?}",
                doc.hash, layout_style
            )
            .as_bytes(),
        )
        .to_hex()
        .to_string();
        if self.layout_key == cache_key {
            return;
        }
        self.layout_key = cache_key.clone();
        self.pages.clear();
        let token = self.layout_generation.clone();
        let id = token.fetch_add(1, Ordering::Relaxed) + 1;
        let cache = self
            .directory
            .join("cache")
            .join(format!("{cache_key}.json"));
        self.status("正文已就绪，正在计算准确页码…");
        std::thread::spawn(move || {
            unsafe {
                let _ = windows::Win32::System::Threading::SetThreadPriority(
                    windows::Win32::System::Threading::GetCurrentThread(),
                    windows::Win32::System::Threading::THREAD_PRIORITY_BELOW_NORMAL,
                );
            }
            let result = (|| -> Result<Vec<usize>, String> {
                if let Ok(data) = std::fs::read(&cache)
                    && let Ok(pages) = serde_json::from_slice::<Vec<usize>>(&data)
                    && pages.first() == Some(&0)
                    && pages.windows(2).all(|x| x[0] < x[1])
                    && pages
                        .iter()
                        .all(|x| *x < doc.text.len() && doc.text.is_char_boundary(*x))
                {
                    return Ok(pages);
                }
                let mut engine = TextEngine::new().map_err(|e| e.to_string())?;
                engine.load_fonts(&fonts).map_err(|e| e.to_string())?;
                let mut pages = Vec::new();
                let mut position = 0;
                while position < doc.text.len() {
                    if token.load(Ordering::Relaxed) != id {
                        return Err("cancelled".into());
                    }
                    pages.push(position);
                    let next = engine.page(&doc.text, position, &style, width, height).end;
                    if next <= position {
                        return Err("分页未前进".into());
                    }
                    position = next;
                }
                if let Some(parent) = cache.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Ok(bytes) = serde_json::to_vec(&pages) {
                    let _ = std::fs::write(&cache, bytes);
                }
                Ok(pages)
            })();
            let _ = slint::invoke_from_event_loop(move || {
                crate::APP.with(|app| {
                    if let Some(c) = app.borrow().as_ref() {
                        let mut c = c.borrow_mut();
                        if c.layout_generation.load(Ordering::Relaxed) != id {
                            return;
                        }
                        match result {
                            Ok(pages) => {
                                c.pages = pages;
                                c.status(format!("目录与分页已就绪，共 {} 页", c.pages.len()));
                                c.draw();
                            }
                            Err(e) if e != "cancelled" => c.status(e),
                            _ => (),
                        }
                    }
                })
            });
        });
    }
    fn show_chapters(&mut self) {
        self.is_recent = false;
        self.search_generation += 1;
        if let Some(doc) = &self.document {
            self.rows = doc.chapters.iter().map(|x| x.start).collect();
            if let Some(ui) = self.ui.upgrade() {
                ui.set_rows(strings(doc.chapters.iter().map(|x| {
                    format!("{}{}", if x.level == 0 { "" } else { "    " }, x.title)
                })));
                ui.set_selected(
                    doc.chapters
                        .partition_point(|x| x.start <= self.offset)
                        .saturating_sub(1) as i32,
                );
            }
        }
    }
    fn find(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let Some(doc) = self.document.clone() else {
            return;
        };
        let query = ui.get_query().to_string();
        if query.is_empty() {
            self.show_chapters();
            return;
        }
        self.search_generation += 1;
        let generation = self.search_generation;
        self.status("正在搜索…");
        std::thread::spawn(move || {
            let found = book::search(&doc, &query, 2000);
            let _ = slint::invoke_from_event_loop(move || {
                crate::APP.with(|app| {
                    if let Some(c) = app.borrow().as_ref() {
                        let mut c = c.borrow_mut();
                        if c.search_generation != generation {
                            return;
                        }
                        c.is_recent = false;
                        c.rows = found.iter().map(|x| x.0).collect();
                        if let Some(ui) = c.ui.upgrade() {
                            ui.set_rows(strings(found.into_iter().map(|x| x.1)));
                        }
                        c.status(format!("{} 条搜索结果，最多显示 2000 条", c.rows.len()));
                    }
                })
            });
        });
    }
    fn apply_rule(&mut self, apply: bool) {
        let Some(ui) = self.ui.upgrade() else { return };
        let rule = ui.get_rule().to_string();
        let Some(doc) = &mut self.document else {
            return;
        };
        match book::chapters(&doc.text, Some(&rule)) {
            Ok(chapters) => {
                let count = chapters.len();
                if apply {
                    doc.chapters = chapters;
                    self.settings
                        .books
                        .entry(self.settings.current.clone())
                        .or_default()
                        .rule = rule;
                    self.show_chapters();
                    self.save_later();
                } else {
                    self.rows = chapters.iter().map(|x| x.start).collect();
                    self.is_recent = false;
                    ui.set_rows(strings(chapters.into_iter().map(|x| x.title)));
                }
                self.status(format!(
                    "{} 个章节{}",
                    count,
                    if apply {
                        "，已应用"
                    } else {
                        "，预览未保存"
                    }
                ));
            }
            Err(e) => self.status(format!("规则错误 {e}")),
        }
    }
    fn style(&mut self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let (Some(foreground), Some(background)) = (
            settings::parse_color(&ui.get_foreground()),
            settings::parse_color(&ui.get_background_color()),
        ) else {
            self.status("颜色格式应为 #AARRGGBB");
            return;
        };
        let old = self.settings.style.clone();
        let s = &mut self.settings.style;
        s.family = ui.get_family().to_string();
        s.size = ui.get_font_size() as f32;
        s.weight = ui.get_weight();
        s.italic = ui.get_italic();
        s.spacing = ui.get_letter_spacing() as f32 / 10.;
        s.line_height = ui.get_line_spacing() as f32 / 100.;
        s.foreground = foreground;
        s.background = background;
        s.multiline = ui.get_multiline();
        s.page_numbers = ui.get_page_numbers();
        let mut a = old;
        let mut b = s.clone();
        a.foreground = 0;
        a.background = 0;
        b.foreground = 0;
        b.background = 0;
        let changed = format!("{a:?}") != format!("{b:?}");
        self.settings.mouse = ui.get_mouse_mode();
        self.reader.mouse(self.settings.mouse);
        self.settings.camouflage = ui.get_disguise().into();
        if changed {
            self.pages.clear();
            self.layout_key.clear();
            self.layout_generation.fetch_add(1, Ordering::Relaxed);
            self.index_timer.start(
                slint::TimerMode::SingleShot,
                std::time::Duration::from_millis(400),
                || {
                    crate::APP.with(|app| {
                        if let Some(c) = app.borrow().as_ref() {
                            c.borrow_mut().index();
                        }
                    })
                },
            );
        }
        self.draw();
        self.save_later();
    }
    fn refresh_screens(&mut self, rescue: bool) {
        self.screens = native::screens();
        if self.screens.is_empty() {
            return;
        }
        let (cx, cy) = native::cursor();
        let index = if rescue {
            self.screens.iter().position(|s| {
                cx >= s.area.x
                    && cx < s.area.x + s.area.width
                    && cy >= s.area.y
                    && cy < s.area.y + s.area.height
            })
        } else {
            self.screens
                .iter()
                .position(|s| s.id == self.settings.monitor)
        }
        .unwrap_or_else(|| self.screens.iter().position(|s| s.primary).unwrap_or(0));
        let screen = &self.screens[index];
        let mut rect = self.reader.rect();
        if rescue {
            rect.x = screen.area.x + (screen.area.width - rect.width) / 2;
            rect.y = screen.area.y + (screen.area.height - rect.height) / 2;
        } else if let Some(local) = self.settings.placements.get(&screen.id) {
            rect = Rect {
                x: screen.area.x + (local.x as f32 * screen.scale) as i32,
                y: screen.area.y + (local.y as f32 * screen.scale) as i32,
                width: (local.width as f32 * screen.scale) as i32,
                height: (local.height as f32 * screen.scale) as i32,
            };
        }
        if self.settings.monitor.is_empty() && self.settings.placements.is_empty() {
            rect = Rect {
                x: screen.area.x + (rect.x as f32 * screen.scale) as i32,
                y: screen.area.y + (rect.y as f32 * screen.scale) as i32,
                width: (rect.width as f32 * screen.scale) as i32,
                height: (rect.height as f32 * screen.scale) as i32,
            };
            self.settings.monitor = screen.id.clone();
            self.settings.placements.insert(
                screen.id.clone(),
                Rect {
                    x: ((rect.x - screen.area.x) as f32 / screen.scale) as i32,
                    y: ((rect.y - screen.area.y) as f32 / screen.scale) as i32,
                    width: (rect.width as f32 / screen.scale) as i32,
                    height: (rect.height as f32 / screen.scale) as i32,
                },
            );
        }
        self.reader.place(rect.fit(screen.area));
        if rescue {
            self.reader.show(true);
            self.settings.monitor = screen.id.clone();
        }
        self.screen_model(index);
        self.draw();
    }
    fn screen_model(&self, index: usize) {
        let Some(ui) = self.ui.upgrade() else { return };
        let min_x = self.screens.iter().map(|s| s.area.x).min().unwrap_or(0);
        let min_y = self.screens.iter().map(|s| s.area.y).min().unwrap_or(0);
        let max_x = self
            .screens
            .iter()
            .map(|s| s.area.x + s.area.width)
            .max()
            .unwrap_or(1);
        let max_y = self
            .screens
            .iter()
            .map(|s| s.area.y + s.area.height)
            .max()
            .unwrap_or(1);
        let width = (max_x - min_x).max(1) as f32;
        let height = (max_y - min_y).max(1) as f32;
        ui.set_screens(
            Rc::new(VecModel::from(
                self.screens
                    .iter()
                    .enumerate()
                    .map(|(i, s)| ScreenBox {
                        name: format!(
                            "{}  {}×{}  {}%",
                            i + 1,
                            s.area.width,
                            s.area.height,
                            (s.scale * 100.) as i32
                        )
                        .into(),
                        x: (s.area.x - min_x) as f32 / width,
                        y: (s.area.y - min_y) as f32 / height,
                        w: s.area.width as f32 / width,
                        h: s.area.height as f32 / height,
                    })
                    .collect::<Vec<_>>(),
            ))
            .into(),
        );
        ui.set_monitor_index(index as i32);
        let r = self.reader.rect();
        let screen = &self.screens[index];
        ui.set_pos_x(((r.x - screen.area.x) as f32 / screen.scale) as i32);
        ui.set_pos_y(((r.y - screen.area.y) as f32 / screen.scale) as i32);
        ui.set_reader_width((r.width as f32 / screen.scale) as i32);
        ui.set_reader_height((r.height as f32 / screen.scale) as i32);
    }
    fn geometry(&mut self) {
        let r = self.reader.rect();
        self.settings.rect = r;
        let index = self
            .screens
            .iter()
            .position(|s| {
                r.x >= s.area.x
                    && r.x < s.area.x + s.area.width
                    && r.y >= s.area.y
                    && r.y < s.area.y + s.area.height
            })
            .unwrap_or(0);
        if let Some(screen) = self.screens.get(index) {
            self.settings.monitor = screen.id.clone();
            self.settings.placements.insert(
                screen.id.clone(),
                Rect {
                    x: ((r.x - screen.area.x) as f32 / screen.scale) as i32,
                    y: ((r.y - screen.area.y) as f32 / screen.scale) as i32,
                    width: (r.width as f32 / screen.scale) as i32,
                    height: (r.height as f32 / screen.scale) as i32,
                },
            );
            self.screen_model(index);
        }
        self.draw();
        self.index();
        self.save_later();
    }
    fn import_font(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("字体", &["ttf", "otf", "ttc"])
            .pick_file()
        else {
            return;
        };
        let result = (|| -> Result<(), String> {
            let data = std::fs::read(&path).map_err(|e| e.to_string())?;
            let dir = self.directory.join("fonts");
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let target = dir.join(format!(
                "{}-{}",
                blake3::hash(&data).to_hex(),
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            std::fs::write(&target, data).map_err(|e| e.to_string())?;
            let mut fonts = self.settings.fonts.clone();
            fonts.push(target.to_string_lossy().into_owned());
            fonts.sort();
            fonts.dedup();
            let names = self.engine.load_fonts(&fonts).map_err(|e| e.to_string())?;
            self.settings.fonts = fonts;
            if let Some(ui) = self.ui.upgrade() {
                ui.set_font_families(strings(names));
            }
            self.draw();
            self.index();
            self.save_later();
            Ok(())
        })();
        match result {
            Ok(()) => self.status("字体已导入，可在字体列表中选择"),
            Err(e) => self.status(e),
        }
    }
    pub fn action(&mut self, action: Action) {
        match action {
            Action::Next => {
                if self.document.is_some()
                    && self.end > self.offset
                    && self.end < self.document.as_ref().unwrap().text.len()
                {
                    self.history.push(self.offset);
                    self.offset = self.end;
                    self.draw();
                    self.save_later();
                } else if self.automatic {
                    self.automatic = false;
                    self.reader.automatic(None);
                }
            }
            Action::Previous => {
                if let Some(offset) = self.history.pop() {
                    self.offset = offset;
                } else if !self.pages.is_empty() {
                    let index = self
                        .pages
                        .partition_point(|x| *x < self.offset)
                        .saturating_sub(1);
                    self.offset = self.pages[index];
                }
                self.draw();
                self.save_later();
            }
            Action::Boss => {
                let visible = self.reader.visible();
                self.reader.show(!visible);
                if visible {
                    self.automatic = false;
                    self.reader.automatic(None);
                    if let Some(ui) = self.ui.upgrade() {
                        let _ = ui.hide();
                    }
                }
            }
            Action::Auto => {
                self.automatic = !self.automatic;
                self.reader.automatic(if self.automatic {
                    Some(self.settings.seconds)
                } else {
                    None
                });
                self.status(if self.automatic {
                    "自动翻页已开启"
                } else {
                    "自动翻页已暂停"
                });
            }
            Action::Rescue => {
                self.refresh_screens(true);
                self.geometry();
            }
            Action::Control => {
                if let Some(ui) = self.ui.upgrade() {
                    let _ = ui.show();
                }
            }
            Action::Quit => {
                self.persist();
                self.layout_generation.fetch_add(1, Ordering::Relaxed);
                let _ = slint::quit_event_loop();
            }
            Action::Geometry => self.geometry(),
            Action::Topology => {
                self.refresh_screens(false);
                self.index();
            }
        }
    }
}
pub fn connect(ui: &AppWindow, c: &Rc<RefCell<Controller>>) {
    macro_rules! connect {
        ($on:ident,$body:expr) => {{
            let c = c.clone();
            ui.$on(move || {
                let mut c = c.borrow_mut();
                ($body)(&mut c);
            });
        }};
    }
    connect!(
        on_open_book,
        |c: &mut Controller| if let Some(path) = rfd::FileDialog::new()
            .add_filter("TXT 小说", &["txt"])
            .pick_file()
        {
            c.open(path, None);
        }
    );
    connect!(on_recent_book, |c: &mut Controller| {
        c.is_recent = true;
        c.recent = c.settings.books.keys().cloned().collect();
        if let Some(ui) = c.ui.upgrade() {
            ui.set_rows(strings(c.recent.clone()));
            ui.set_section(0);
        }
    });
    connect!(on_show_chapters, |c: &mut Controller| c.show_chapters());
    connect!(on_search, |c: &mut Controller| c.find());
    {
        let c = c.clone();
        ui.on_jump(move |index| {
            let mut c = c.borrow_mut();
            if index < 0 {
                return;
            }
            if c.is_recent {
                if let Some(path) = c.recent.get(index as usize).cloned() {
                    c.open(PathBuf::from(path), None);
                }
            } else if let Some(offset) = c.rows.get(index as usize).copied() {
                c.offset = offset;
                c.history.clear();
                c.draw();
                c.save_later();
            }
        });
    }
    connect!(on_previous, |c: &mut Controller| c.action(Action::Previous));
    connect!(on_next, |c: &mut Controller| c.action(Action::Next));
    connect!(on_toggle_auto, |c: &mut Controller| c.action(Action::Auto));
    connect!(on_toggle_reader, |c: &mut Controller| c
        .action(Action::Boss));
    connect!(on_rescue, |c: &mut Controller| c.action(Action::Rescue));
    connect!(on_quit, |c: &mut Controller| c.action(Action::Quit));
    connect!(on_apply_style, |c: &mut Controller| c.style());
    connect!(on_import_font, |c: &mut Controller| c.import_font());
    {
        let c = c.clone();
        ui.on_apply_rule(move |apply| c.borrow_mut().apply_rule(apply));
    }
    {
        let c = c.clone();
        ui.on_select_screen(move |index| {
            let c = c.borrow();
            if index >= 0 && (index as usize) < c.screens.len() {
                c.screen_model(index as usize);
            }
        });
    }
    connect!(on_apply_position, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade()
            && let Some(s) = c.screens.get(ui.get_monitor_index() as usize)
        {
            c.reader.place(
                Rect {
                    x: s.area.x + (ui.get_pos_x() as f32 * s.scale) as i32,
                    y: s.area.y + (ui.get_pos_y() as f32 * s.scale) as i32,
                    width: (ui.get_reader_width() as f32 * s.scale) as i32,
                    height: (ui.get_reader_height() as f32 * s.scale) as i32,
                }
                .fit(s.area),
            );
            c.geometry();
        }
    });
    connect!(on_apply_keys, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade() {
            c.settings.hotkeys = vec![
                ui.get_previous_key(),
                ui.get_next_key(),
                ui.get_boss_key(),
                ui.get_auto_key(),
                ui.get_rescue_key(),
            ]
            .into_iter()
            .map(String::from)
            .collect();
            let errors = c.register_hotkeys();
            c.status(if errors.is_empty() {
                "快捷键已保存".into()
            } else {
                errors.join("，")
            });
            c.save_later();
        }
    });
    connect!(on_save_options, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade() {
            c.settings.seconds = ui.get_interval().max(1) as u32;
            c.persist();
            c.status("已保存");
        }
    });
    connect!(
        on_change_encoding,
        |c: &mut Controller| if let Some(ui) = c.ui.upgrade() {
            let encoding = ui.get_encoding().to_string();
            let key = c.settings.current.clone();
            c.settings.books.entry(key.clone()).or_default().encoding = encoding.clone();
            if !key.is_empty() {
                c.open(PathBuf::from(key), Some(encoding));
            }
        }
    );
}
