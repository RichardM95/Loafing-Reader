//! Offline UI inspection: no native Reader, tray, hotkeys, or OS input.
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{fs::File, io::BufWriter, rc::Rc};
slint::include_modules!();
fn strings(v: &[&str]) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        v.iter().map(|s| SharedString::from(*s)).collect::<Vec<_>>(),
    ))
    .into()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    slint::platform::set_platform(Box::new(i_slint_backend_testing::TestingBackend::new(
        i_slint_backend_testing::TestingBackendOptions {
            mock_time: true,
            threading: false,
            renderer_name: Some("skia".into()),
        },
    )))?;
    std::fs::create_dir_all("dist/screenshots")?;
    let ui = AppWindow::new()?;
    ui.on_key_caps(|value| {
        Rc::new(VecModel::from(
            thief_reader::hotkeys::keycaps(&value)
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        ))
        .into()
    });
    ui.set_book_name("第一本小说：很长的书名也应保持清晰，目录与正文搜索".into());
    ui.set_progress(0.427);
    ui.set_rows(strings(&[
        "第一章  新的开始",
        "第二章  林间的故事",
        "第三章  漫长的一天",
    ]));
    ui.set_selected(1);
    ui.set_family("Microsoft YaHei".into());
    ui.set_foreground("#FFFFFF".into());
    ui.set_background_color("#000000".into());
    ui.set_screen_names(strings(&[
        "显示器 1 · 1920 × 1080 · 100%",
        "显示器 2 · 2560 × 1440 · 150%",
        "显示器 3 · 1080 × 1920 · 100%",
    ]));
    ui.set_screens(
        Rc::new(VecModel::from(vec![
            ScreenBox {
                name: "1".into(),
                x: 0.,
                y: 0.2,
                w: 0.35,
                h: 0.6,
            },
            ScreenBox {
                name: "2".into(),
                x: 0.35,
                y: 0.1,
                w: 0.45,
                h: 0.8,
            },
            ScreenBox {
                name: "3".into(),
                x: 0.8,
                y: 0.,
                w: 0.2,
                h: 1.,
            },
        ]))
        .into(),
    );
    ui.set_actual_x(0.12);
    ui.set_actual_y(0.5);
    ui.set_actual_w(0.15);
    ui.set_actual_h(0.025);
    ui.set_draft_x(0.42);
    ui.set_draft_y(0.5);
    ui.set_draft_w(0.15);
    ui.set_draft_h(0.025);
    let engine = thief_reader::native::TextEngine::new()?;
    let mut pixels = engine.render(
        "第一章  新的开始。午后的阳光穿过窗边，落在翻开的书页上。  42/1200",
        &thief_reader::settings::Style::default(),
        856,
        47,
        1.,
    )?;
    for p in pixels.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
        let a = p[3] as u32;
        for c in &mut p[..3] {
            *c = (*c as u32 * 255).checked_div(a).unwrap_or(0).min(255) as u8;
        }
    }
    ui.set_preview(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
        slint::Rgba8Pixel,
    >::clone_from_slice(
        &pixels, 856, 47
    )));
    let quick = std::env::args().any(|arg| arg == "--hotkeys-only");
    let about_only = std::env::args().any(|arg| arg == "--about-only");
    let mut count = 0;
    for dark in [false, true] {
        ui.global::<Palette>().set_color_scheme(if dark {
            slint::language::ColorScheme::Dark
        } else {
            slint::language::ColorScheme::Light
        });
        for (width, height) in [(1000, 720), (650, 520)] {
            for scale in [1.0, 1.5, 2.0] {
                if (quick || about_only) && scale != 1.0 {
                    continue;
                }
                ui.window()
                    .dispatch_event(slint::platform::WindowEvent::ScaleFactorChanged {
                        scale_factor: scale,
                    });
                ui.window().set_size(slint::PhysicalSize::new(
                    (width as f32 * scale) as u32,
                    (height as f32 * scale) as u32,
                ));
                // Testing adapter converts sizes at 1x. Supply the logical viewport explicitly.
                ui.window()
                    .dispatch_event(slint::platform::WindowEvent::Resized {
                        size: slint::LogicalSize::new(width as f32, height as f32),
                    });
                ui.set_foreground_swatch(slint::Color::from_rgb_u8(255, 255, 255));
                ui.set_background_swatch(slint::Color::from_rgb_u8(0, 0, 0));
                ui.set_dirty(
                    Rc::new(VecModel::from(vec![false, true, true, true, true, false])).into(),
                );
                ui.set_key_errors(strings(if width == 650 && scale == 2.0 {
                    &[
                        "",
                        "",
                        "Ctrl+Alt+M 已被其他程序占用，请选择其他组合键。",
                        "",
                        "",
                    ]
                } else {
                    &["", "", "", "", ""]
                }));
                ui.set_family(if width==650 && scale==2.0 {"这是用于检验截断行为的超长中文字体名称 Microsoft YaHei UI Extended Family Name"} else {"Microsoft YaHei"}.into());
                for section in 0..6 {
                    if (quick && section != 4) || (about_only && section != 5) {
                        continue;
                    }
                    ui.set_recording_index(-1);
                    ui.set_section(section);
                    if section == 4 && width == 1000 && scale == 2.0 {
                        ui.set_recording_index(0);
                        ui.set_recording_preview("Ctrl+Alt".into());
                    }
                    i_slint_backend_testing::mock_elapsed_time(std::time::Duration::from_millis(
                        150,
                    ));
                    let shot = ui.window().take_snapshot()?;
                    assert_eq!(shot.width(), (width as f32 * scale) as u32);
                    assert_eq!(shot.height(), (height as f32 * scale) as u32);

                    let path = format!(
                        "dist/screenshots/{}-{}-{}-{}.png",
                        if dark { "dark" } else { "light" },
                        width,
                        (scale * 100.) as i32,
                        section
                    );
                    let mut encoder = png::Encoder::new(
                        BufWriter::new(File::create(path)?),
                        shot.width(),
                        shot.height(),
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    encoder.write_header()?.write_image_data(shot.as_bytes())?;
                    count += 1;
                }
            }
        }
    }
    println!("Rendered {count} headless screenshots; no native windows or system input.");
    Ok(())
}
