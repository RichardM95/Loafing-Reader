//! Win7 DirectWrite custom collections and cluster spacing.
use std::cell::Cell;
use windows::{
    Win32::{Foundation::*, Graphics::DirectWrite::*},
    core::*,
};

#[implement(IDWriteFontCollectionLoader)]
pub struct FontLoader(pub Vec<String>);
impl IDWriteFontCollectionLoader_Impl for FontLoader_Impl {
    fn CreateEnumeratorFromKey(
        &self,
        factory: Ref<IDWriteFactory>,
        _: *const core::ffi::c_void,
        _: u32,
    ) -> Result<IDWriteFontFileEnumerator> {
        let factory = factory.as_ref().ok_or_else(|| Error::from(E_POINTER))?;
        let files = self
            .0
            .iter()
            .map(|path| unsafe {
                factory.CreateFontFileReference(PCWSTR(crate::native::wide(path).as_ptr()), None)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(FontFiles {
            files,
            next: Cell::new(0),
        }
        .into())
    }
}
#[implement(IDWriteFontFileEnumerator)]
struct FontFiles {
    files: Vec<IDWriteFontFile>,
    next: Cell<usize>,
}
impl IDWriteFontFileEnumerator_Impl for FontFiles_Impl {
    fn MoveNext(&self) -> Result<BOOL> {
        let i = self.next.get();
        if i < self.files.len() {
            self.next.set(i + 1);
            Ok(BOOL(1))
        } else {
            Ok(BOOL(0))
        }
    }
    fn GetCurrentFontFile(&self) -> Result<IDWriteFontFile> {
        self.next
            .get()
            .checked_sub(1)
            .and_then(|i| self.files.get(i))
            .cloned()
            .ok_or_else(|| Error::from(E_FAIL))
    }
}
pub fn family_names(collection: &IDWriteFontCollection) -> Result<Vec<String>> {
    unsafe {
        (0..collection.GetFontFamilyCount())
            .map(|i| {
                let name = collection.GetFontFamily(i)?.GetFamilyNames()?;
                let len = name.GetStringLength(0)? as usize;
                let mut buf = vec![0; len + 1];
                name.GetString(0, &mut buf)?;
                Ok(String::from_utf16_lossy(&buf[..len]))
            })
            .collect()
    }
}

#[implement(IDWriteInlineObject)]
struct SpacedCluster {
    layout: IDWriteTextLayout,
    metrics: DWRITE_INLINE_OBJECT_METRICS,
    overhang: DWRITE_OVERHANG_METRICS,
    shift: f32,
    after: DWRITE_BREAK_CONDITION,
}
impl IDWriteInlineObject_Impl for SpacedCluster_Impl {
    fn Draw(
        &self,
        context: *const core::ffi::c_void,
        renderer: Ref<IDWriteTextRenderer>,
        x: f32,
        y: f32,
        _: BOOL,
        _: BOOL,
        _: Ref<IUnknown>,
    ) -> Result<()> {
        unsafe {
            self.layout.Draw(
                Some(context),
                renderer.as_ref().ok_or_else(|| Error::from(E_POINTER))?,
                x + self.shift,
                y,
            )
        }
    }
    fn GetMetrics(&self) -> Result<DWRITE_INLINE_OBJECT_METRICS> {
        Ok(self.metrics)
    }
    fn GetOverhangMetrics(&self) -> Result<DWRITE_OVERHANG_METRICS> {
        Ok(self.overhang)
    }
    fn GetBreakConditions(
        &self,
        before: *mut DWRITE_BREAK_CONDITION,
        after: *mut DWRITE_BREAK_CONDITION,
    ) -> Result<()> {
        if before.is_null() || after.is_null() {
            return Err(Error::from(E_POINTER));
        }
        unsafe {
            *before = DWRITE_BREAK_CONDITION_NEUTRAL;
            *after = self.after;
        }
        Ok(())
    }
}
/// Inline objects retain DirectWrite shaping/fallback, and participate in its line breaking.
/// Merge native shaping clusters with grapheme boundaries so combining/ZWJ sequences stay intact.
pub fn apply_spacing(
    factory: &IDWriteFactory,
    layout: &IDWriteTextLayout,
    text: &[u16],
    format: &IDWriteTextFormat,
    spacing: f32,
) -> Result<()> {
    use unicode_segmentation::UnicodeSegmentation;
    unsafe {
        let raw = String::from_utf16_lossy(text);
        let mut boundaries = std::collections::HashSet::new();
        let mut n = 0;
        for g in raw.graphemes(true) {
            n += g.encode_utf16().count();
            boundaries.insert(n);
        }
        let mut count = 0;
        let _ = layout.GetClusterMetrics(None, &mut count);
        let mut clusters = vec![DWRITE_CLUSTER_METRICS::default(); count as usize];
        layout.GetClusterMetrics(Some(&mut clusters), &mut count)?;
        let mut start = 0;
        let mut end = 0;
        for c in clusters {
            end += c.length as usize;
            if !boundaries.contains(&end) {
                continue;
            }
            let slice = &text[start..end];
            if slice.iter().any(|c| *c == 10 || *c == 13) {
                start = end;
                continue;
            }
            let inner = factory.CreateTextLayout(slice, format, 100000., 100000.)?;
            inner.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)?;
            inner.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)?;
            inner.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            let mut m = DWRITE_TEXT_METRICS::default();
            inner.GetMetrics(&mut m)?;
            let mut lines = [DWRITE_LINE_METRICS::default(); 1];
            let mut line_count = 0;
            inner.GetLineMetrics(Some(&mut lines), &mut line_count)?;
            inner.SetMaxWidth(m.widthIncludingTrailingWhitespace.max(1.))?;
            inner.SetMaxHeight(lines[0].height)?;
            let mut overhang = inner.GetOverhangMetrics()?;
            overhang.left = (overhang.left - spacing).max(0.);
            overhang.right = overhang.right.max(0.);
            overhang.top = overhang.top.max(0.);
            overhang.bottom = overhang.bottom.max(0.);
            let object: IDWriteInlineObject = SpacedCluster {
                layout: inner,
                metrics: DWRITE_INLINE_OBJECT_METRICS {
                    width: (m.widthIncludingTrailingWhitespace + spacing).max(0.),
                    height: lines[0].height,
                    baseline: lines[0].baseline,
                    supportsSideways: BOOL(0),
                },
                overhang,
                shift: spacing,
                after: if c._bitfield & 1 != 0 {
                    DWRITE_BREAK_CONDITION_CAN_BREAK
                } else {
                    DWRITE_BREAK_CONDITION_MAY_NOT_BREAK
                },
            }
            .into();
            layout.SetInlineObject(
                &object,
                DWRITE_TEXT_RANGE {
                    startPosition: start as u32,
                    length: (end - start) as u32,
                },
            )?;
            start = end;
        }
        Ok(())
    }
}
