//! Validation and conversion for unapplied settings. No window or persistence effects.
use crate::settings::{Style, parse_color};
pub fn editor_color(rgb: &str, percent: i32) -> Result<u32, String> {
    if rgb.len() != 7 || !rgb.starts_with('#') {
        return Err("请输入 #RRGGBB 格式颜色".into());
    }
    let color = parse_color(&format!("#FF{}", &rgb[1..])).ok_or("请输入有效的十六进制颜色")?;
    Ok((color & 0x00ff_ffff) | (((percent.clamp(0, 100) as u32 * 255 + 50) / 100) << 24))
}
pub fn layout_changed(a: &Style, b: &Style) -> bool {
    a.family != b.family
        || a.size != b.size
        || a.weight != b.weight
        || a.italic != b.italic
        || a.spacing != b.spacing
        || a.line_height != b.line_height
        || a.page_numbers != b.page_numbers
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_validate_without_side_effects() {
        assert_eq!(editor_color("#123456", 0).unwrap(), 0x00123456);
        assert_eq!(editor_color("#123456", 50).unwrap(), 0x80123456);
        assert_eq!(editor_color("#FFFFFF", 100).unwrap(), 0xffffffff);
        assert!(editor_color("#GGGGGG", 20).is_err());
        assert!(editor_color("bad", 20).is_err());
    }
    #[test]
    fn colors_do_not_repaginate() {
        let a = Style::default();
        let mut b = a.clone();
        b.foreground = 0;
        b.background = 0;
        assert!(!layout_changed(&a, &b));
        b.size += 1.;
        assert!(layout_changed(&a, &b));
    }
}

/// Restore effective registrations if any candidate fails. Caller persists only success.
pub fn register_keys(
    previous: &[String],
    candidate: &[String],
    mut register: impl FnMut(&[String]) -> Vec<String>,
) -> Result<(), Vec<String>> {
    let errors = register(candidate);
    if errors.is_empty() {
        Ok(())
    } else {
        let _ = register(previous);
        Err(errors)
    }
}
#[cfg(test)]
mod shortcut_tests {
    use super::*;
    #[test]
    fn failure_restores_previous_bindings() {
        let previous = vec!["Ctrl+Alt+M".into()];
        let candidate = vec!["invalid".into()];
        let mut calls = vec![];
        let result = register_keys(&previous, &candidate, |keys| {
            calls.push(keys.to_vec());
            if keys == candidate {
                vec!["invalid 无效".into()]
            } else {
                vec![]
            }
        });
        assert!(result.is_err());
        assert_eq!(calls, vec![candidate, previous]);
    }
    #[test]
    fn success_registers_once() {
        let mut count = 0;
        assert!(
            register_keys(&[], &[], |_| {
                count += 1;
                vec![]
            })
            .is_ok()
        );
        assert_eq!(count, 1);
    }
}
