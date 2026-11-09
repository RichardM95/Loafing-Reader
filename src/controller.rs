use crate::{
    AppWindow, ScreenBox,
    native::{self, Action, Reader, Screen, TextEngine},
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};
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
    preview_timer: slint::Timer,
    layout_key: String,
    recorder: thief_reader::hotkeys::Recorder,
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
            ui.set_font_families(strings(families.clone()));
            ui.set_filtered_fonts(strings(families));
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
            preview_timer: slint::Timer::default(),
            layout_key: String::new(),
            recorder: thief_reader::hotkeys::Recorder::default(),
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
            ui.set_key_errors(strings(
                self.settings
                    .hotkeys
                    .iter()
                    .map(|key| thief_reader::hotkeys::registration_error(&errors, key)),
            ));
        }
        errors
    }
    fn shortcut_draft(&self) -> Vec<String> {
        self.ui
            .upgrade()
            .map(|ui| {
                vec![
                    ui.get_previous_key().into(),
                    ui.get_next_key().into(),
                    ui.get_boss_key().into(),
                    ui.get_auto_key().into(),
                    ui.get_rescue_key().into(),
                ]
            })
            .unwrap_or_default()
    }
    fn resume_hotkeys(&self) {
        let registration = self.reader.hotkeys(&self.settings.hotkeys);
        let draft = self.shortcut_draft();
        let mut errors = thief_reader::hotkeys::validate(&draft);
        for (i, error) in errors.iter_mut().enumerate() {
            if error.is_empty() && draft.get(i) == self.settings.hotkeys.get(i) {
                *error = thief_reader::hotkeys::registration_error(&registration, &draft[i]);
            }
        }
        if let Some(ui) = self.ui.upgrade() {
            ui.set_key_errors(strings(errors));
        }
        if !registration.is_empty() {
            self.status(registration.join("，"));
        }
    }
    pub fn cancel_recording(&mut self, index: i32) {
        let Some(target) = self.recorder.target else {
            return;
        };
        if index >= 0 && index as usize != target {
            return;
        }
        self.recorder.cancel();
        if let Some(ui) = self.ui.upgrade() {
            ui.set_recording_index(-1);
            ui.set_recording_preview("".into());
            ui.set_recording_error("".into());
        }
        self.status("已取消录制，原快捷键保持不变");
        self.resume_hotkeys();
    }
    fn begin_recording(&mut self, index: i32) {
        let Some(ui) = self.ui.upgrade() else { return };
        if !(0..5).contains(&index) || ui.get_section() != 4 {
            return;
        }
        self.reader.hotkeys(&[]);
        self.recorder.begin(index as usize);
        ui.set_recording_index(index);
        ui.set_recording_preview("".into());
        ui.set_recording_error("".into());
        self.status("正在录制，当前全局快捷键暂时停用");
    }
    fn record_pressed(&mut self, index: i32, text: &str, modifiers: u32) {
        if self.recorder.target != Some(index as usize) {
            return;
        }
        if let Some(ui) = self.ui.upgrade() {
            match self.recorder.press(text, modifiers) {
                Ok(preview) => {
                    ui.set_recording_preview(preview.into());
                    ui.set_recording_error("".into());
                }
                Err(error) => ui.set_recording_error(error.into()),
            }
        }
    }
    fn record_released(&mut self, index: i32) {
        if self.recorder.target != Some(index as usize) {
            return;
        }
        if let Some((target, key)) = self.recorder.release() {
            if let Some(ui) = self.ui.upgrade() {
                match target {
                    0 => ui.set_previous_key(key.into()),
                    1 => ui.set_next_key(key.into()),
                    2 => ui.set_boss_key(key.into()),
                    3 => ui.set_auto_key(key.into()),
                    4 => ui.set_rescue_key(key.into()),
                    _ => {}
                }
                ui.set_recording_index(-1);
                ui.set_recording_preview("".into());
                ui.set_recording_error("".into());
            }
            self.mark_dirty();
            self.status("组合键已录制，点击应用更改后生效");
            self.resume_hotkeys();
        } else if let Some(ui) = self.ui.upgrade() {
            ui.set_recording_preview("".into());
        }
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
        for section in [1, 2, 4] {
            self.restore_section(section);
        }
    }
    fn restore_section(&self, section: i32) {
        if let Some(ui) = self.ui.upgrade() {
            match section {
                1 => {
                    let s = &self.settings.style;
                    ui.set_family(s.family.clone().into());
                    ui.set_font_size(s.size as i32);
                    ui.set_weight(s.weight);
                    ui.set_italic(s.italic);
                    ui.set_letter_spacing((s.spacing * 10.) as i32);
                    ui.set_line_spacing((s.line_height * 100.) as i32);
                    ui.set_foreground(format!("#{:06X}", s.foreground & 0xffffff).into());
                    ui.set_background_color(format!("#{:06X}", s.background & 0xffffff).into());
                    ui.set_foreground_alpha(((s.foreground >> 24) * 100 + 127) as i32 / 255);
                    ui.set_background_alpha(((s.background >> 24) * 100 + 127) as i32 / 255);
                    ui.set_page_numbers(s.page_numbers);
                    ui.set_spacing_input(s.spacing.to_string().into());
                }
                2 => {
                    ui.set_mouse_mode(self.settings.mouse);
                    ui.set_interval(self.settings.seconds as i32);
                    ui.set_disguise(self.settings.camouflage.clone().into());
                }
                4 => {
                    let keys = &self.settings.hotkeys;
                    if keys.len() >= 5 {
                        ui.set_previous_key(keys[0].clone().into());
                        ui.set_next_key(keys[1].clone().into());
                        ui.set_boss_key(keys[2].clone().into());
                        ui.set_auto_key(keys[3].clone().into());
                        ui.set_rescue_key(keys[4].clone().into());
                    }
                    ui.set_key_errors(strings(vec![String::new(); 5]));
                }
                _ => {}
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
            let reservation = if self.pages.is_empty() {
                self.engine
                    .footer_reservation(&self.settings.style, doc.text.len())
            } else {
                self.engine
                    .footer_reservation(&self.settings.style, self.pages.len())
            };
            let page = self.engine.page_with_footer(
                &doc.text,
                start,
                &self.settings.style,
                rect.width as f32 / dpi,
                rect.height as f32 / dpi,
                &reservation,
            );
            self.end = page.end;
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
            page.display(&footer)
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
                self.preview();
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
                "v3-auto-lines/{}/{:?}/{width}/{height}/{fonts:?}",
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
                let pages = engine.paginate(&doc.text, &style, width, height, || {
                    token.load(Ordering::Relaxed) != id
                })?;
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
    fn draft_style(&self) -> Option<settings::Style> {
        let ui = self.ui.upgrade()?;
        let fg = thief_reader::preferences::editor_color(
            &ui.get_foreground(),
            ui.get_foreground_alpha(),
        );
        let bg = thief_reader::preferences::editor_color(
            &ui.get_background_color(),
            ui.get_background_alpha(),
        );
        ui.set_color_errors(strings([
            fg.as_ref().err().cloned().unwrap_or_default(),
            bg.as_ref().err().cloned().unwrap_or_default(),
        ]));
        if let Ok(v) = fg.as_ref() {
            ui.set_foreground_swatch(slint::Color::from_argb_encoded(*v | 0xff000000));
        }
        if let Ok(v) = bg.as_ref() {
            ui.set_background_swatch(slint::Color::from_argb_encoded(*v | 0xff000000));
        }
        let spacing = ui
            .get_spacing_input()
            .parse::<f32>()
            .ok()
            .filter(|n| n.is_finite() && (-2.0..=10.0).contains(n));
        ui.set_spacing_error(
            if spacing.is_some() {
                ""
            } else {
                "请输入 -2 至 10 之间的字距"
            }
            .into(),
        );
        Some(settings::Style {
            family: ui.get_family().into(),
            size: ui.get_font_size() as f32,
            weight: ui.get_weight(),
            italic: ui.get_italic(),
            spacing: spacing?,
            line_height: ui.get_line_spacing() as f32 / 100.,
            foreground: fg.ok()?,
            background: bg.ok()?,
            page_numbers: ui.get_page_numbers(),
        })
    }
    fn preview(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let Some(style) = self.draft_style() else {
            return;
        };
        let r = self.reader.rect();
        let dpi = self.reader.dpi();
        let display = if let Some(doc) = &self.document {
            let footer = if style.page_numbers { "  …/…" } else { "" };
            self.engine
                .page_with_footer(
                    &doc.text,
                    self.offset,
                    &style,
                    r.width as f32 / dpi,
                    r.height as f32 / dpi,
                    footer,
                )
                .display(footer)
        } else {
            ui.get_disguise().to_string()
        };
        if let Ok(mut pixels) = self.engine.render_aligned(
            &display,
            &style,
            r.width as u32,
            r.height as u32,
            dpi,
            self.document.is_none(),
        ) {
            for p in pixels.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                let a = p[3] as u32;
                for color in &mut p[..3] {
                    *color = (*color as u32 * 255).checked_div(a).unwrap_or(0).min(255) as u8;
                }
            }
            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(
                &pixels,
                r.width as u32,
                r.height as u32,
            );
            ui.set_preview(slint::Image::from_rgba8(buffer));
            ui.set_preview_width(r.width as f32 / dpi);
            ui.set_preview_height(r.height as f32 / dpi);
        }
    }
    fn mark_dirty(&self) {
        if let Some(ui) = self.ui.upgrade() {
            let mut flags: Vec<bool> = (0..6)
                .map(|i| ui.get_dirty().row_data(i).unwrap_or(false))
                .collect();
            flags[ui.get_section() as usize] = true;
            ui.set_dirty(Rc::new(VecModel::from(flags)).into());
            self.update_map();
        }
        self.preview_timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(100),
            || {
                crate::APP.with(|a| {
                    if let Some(c) = a.borrow().as_ref() {
                        c.borrow().preview();
                    }
                });
            },
        );
    }
    fn clear_dirty(&self, section: usize) {
        if let Some(ui) = self.ui.upgrade() {
            let mut flags: Vec<bool> = (0..6)
                .map(|i| ui.get_dirty().row_data(i).unwrap_or(false))
                .collect();
            flags[section] = false;
            ui.set_dirty(Rc::new(VecModel::from(flags)).into());
        }
    }
    fn style(&mut self) {
        let Some(style) = self.draft_style() else {
            self.status("请修正标出的输入");
            return;
        };
        let changed = thief_reader::preferences::layout_changed(&self.settings.style, &style);
        self.settings.style = style;
        if changed {
            self.pages.clear();
            self.layout_key.clear();
            self.layout_generation.fetch_add(1, Ordering::Relaxed);
        }
        self.draw();
        if changed {
            self.index();
        }
        self.persist();
        self.clear_dirty(1);
        self.status("外观已应用");
    }
    fn update_map(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let min_x = self.screens.iter().map(|s| s.area.x).min().unwrap_or(0);
        let min_y = self.screens.iter().map(|s| s.area.y).min().unwrap_or(0);
        let w = (self
            .screens
            .iter()
            .map(|s| s.area.x + s.area.width)
            .max()
            .unwrap_or(1)
            - min_x)
            .max(1) as f32;
        let h = (self
            .screens
            .iter()
            .map(|s| s.area.y + s.area.height)
            .max()
            .unwrap_or(1)
            - min_y)
            .max(1) as f32;
        ui.set_map_ratio(w / h);
        let r = self.reader.rect();
        ui.set_actual_x((r.x - min_x) as f32 / w);
        ui.set_actual_y((r.y - min_y) as f32 / h);
        ui.set_actual_w(r.width as f32 / w);
        ui.set_actual_h(r.height as f32 / h);
        if let Some(screen) = self.screens.get(ui.get_monitor_index() as usize) {
            ui.set_draft_x(
                (screen.area.x - min_x) as f32 / w + ui.get_pos_x() as f32 * screen.scale / w,
            );
            ui.set_draft_y(
                (screen.area.y - min_y) as f32 / h + ui.get_pos_y() as f32 * screen.scale / h,
            );
            ui.set_draft_w(ui.get_reader_width() as f32 * screen.scale / w);
            ui.set_draft_h(ui.get_reader_height() as f32 * screen.scale / h);
        }
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
        ui.set_screen_names(strings(self.screens.iter().enumerate().map(|(i, s)| {
            format!(
                "显示器 {}  ·  {} × {}  ·  {}%",
                i + 1,
                s.area.width,
                s.area.height,
                (s.scale * 100.) as i32
            )
        })));
        if ui.get_dirty().row_data(3).unwrap_or(false) {
            self.update_map();
            return;
        }
        ui.set_monitor_index(index as i32);
        let r = self.reader.rect();
        let screen = &self.screens[index];
        ui.set_pos_x(((r.x - screen.area.x) as f32 / screen.scale) as i32);
        ui.set_pos_y(((r.y - screen.area.y) as f32 / screen.scale) as i32);
        ui.set_reader_width((r.width as f32 / screen.scale) as i32);
        ui.set_reader_height((r.height as f32 / screen.scale) as i32);
        self.update_map();
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
                ui.set_font_families(strings(names.clone()));
                ui.set_filtered_fonts(strings(names));
            }
            self.preview();
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
                self.cancel_recording(-1);
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
                self.cancel_recording(-1);
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
    ui.on_key_caps(|value| strings(thief_reader::hotkeys::keycaps(&value)));
    {
        let c = c.clone();
        ui.on_begin_recording(move |index| c.borrow_mut().begin_recording(index));
    }
    {
        let c = c.clone();
        ui.on_cancel_recording(move |index| c.borrow_mut().cancel_recording(index));
    }
    {
        let c = c.clone();
        ui.on_record_pressed(move |index, text, ctrl, alt, shift, meta| {
            let modifiers =
                u32::from(ctrl) * 2 + u32::from(alt) + u32::from(shift) * 4 + u32::from(meta) * 8;
            c.borrow_mut().record_pressed(index, &text, modifiers);
        });
    }
    {
        let c = c.clone();
        ui.on_record_released(move |index| c.borrow_mut().record_released(index));
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
    connect!(on_draft_changed, |c: &mut Controller| c.mark_dirty());
    connect!(on_filter_fonts, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade() {
            let query = ui.get_font_query().to_lowercase();
            ui.set_filtered_fonts(strings(
                ui.get_font_families()
                    .iter()
                    .filter(|s| s.to_lowercase().contains(&query))
                    .map(String::from),
            ));
        }
    });
    connect!(on_center_position, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade()
            && let Some(s) = c.screens.get(ui.get_monitor_index() as usize)
        {
            ui.set_pos_x(
                ((s.area.width as f32 / s.scale - ui.get_reader_width() as f32) / 2.) as i32,
            );
            ui.set_pos_y(
                ((s.area.height as f32 / s.scale - ui.get_reader_height() as f32) / 2.) as i32,
            );
            c.mark_dirty();
        }
    });
    connect!(on_reset_position, |c: &mut Controller| {
        if let Some(ui) = c.ui.upgrade() {
            ui.set_pos_x(24);
            ui.set_pos_y(24);
            c.mark_dirty();
        }
    });
    {
        let c = c.clone();
        let weak = ui.as_weak();
        ui.on_apply_current(move || {
            let Some(ui) = weak.upgrade() else { return };
            match ui.get_section() {
                1 => c.borrow_mut().style(),
                2 => {
                    let mut c = c.borrow_mut();
                    c.settings.mouse = ui.get_mouse_mode();
                    c.settings.seconds = ui.get_interval().max(1) as u32;
                    c.settings.camouflage = ui.get_disguise().into();
                    c.reader.mouse(c.settings.mouse);
                    if c.automatic {
                        c.reader.automatic(Some(c.settings.seconds));
                    }
                    c.draw();
                    c.persist();
                    c.clear_dirty(2);
                    c.status("阅读操作已应用");
                }
                3 => ui.invoke_apply_position(),
                4 => ui.invoke_apply_keys(),
                _ => {}
            }
        });
    }
    {
        let c = c.clone();
        ui.on_revert_current(move || {
            let mut c = c.borrow_mut();
            c.cancel_recording(-1);
            if let Some(ui) = c.ui.upgrade() {
                let section = ui.get_section();
                c.restore_section(section);
                c.clear_dirty(section as usize);
                if section == 3 {
                    let r = c.reader.rect();
                    let index = c
                        .screens
                        .iter()
                        .position(|screen| {
                            r.x >= screen.area.x
                                && r.x < screen.area.x + screen.area.width
                                && r.y >= screen.area.y
                                && r.y < screen.area.y + screen.area.height
                        })
                        .unwrap_or(0);
                    if !c.screens.is_empty() {
                        c.screen_model(index);
                    }
                }
                c.preview();
                c.status("已还原未应用的更改");
            }
        });
    }
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
                if let Some(ui) = c.ui.upgrade() {
                    ui.set_monitor_index(index);
                    if let Some(screen) = c.screens.get(index as usize) {
                        ui.set_pos_x(
                            ((screen.area.width as f32 / screen.scale
                                - ui.get_reader_width() as f32)
                                / 2.) as i32,
                        );
                        ui.set_pos_y(
                            ((screen.area.height as f32 / screen.scale
                                - ui.get_reader_height() as f32)
                                / 2.) as i32,
                        );
                    }
                }
                c.mark_dirty();
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
            c.clear_dirty(3);
            c.geometry();
            c.persist();
            c.status("窗口位置已应用");
        }
    });
    connect!(on_apply_keys, |c: &mut Controller| {
        c.cancel_recording(-1);
        let validation = thief_reader::hotkeys::validate(&c.shortcut_draft());
        if validation.iter().any(|e| !e.is_empty()) {
            if let Some(ui) = c.ui.upgrade() {
                ui.set_key_errors(strings(validation));
            }
            c.status("请修正标出的快捷键");
            return;
        }
        if let Some(ui) = c.ui.upgrade() {
            let old = c.settings.hotkeys.clone();
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
            let errors =
                thief_reader::preferences::register_keys(&old, &c.settings.hotkeys, |keys| {
                    c.reader.hotkeys(keys)
                })
                .err()
                .unwrap_or_default();
            ui.set_key_errors(strings(
                c.settings
                    .hotkeys
                    .iter()
                    .map(|key| thief_reader::hotkeys::registration_error(&errors, key)),
            ));
            c.status(if errors.is_empty() {
                "快捷键已保存".into()
            } else {
                errors.join("，")
            });
            if errors.is_empty() {
                c.clear_dirty(4);
                c.persist();
            } else {
                c.settings.hotkeys = old;
            }
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
