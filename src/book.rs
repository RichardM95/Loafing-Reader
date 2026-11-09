use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Chapter {
    pub title: String,
    pub start: usize,
    pub level: u8,
}

#[derive(Clone)]
pub struct Document {
    pub path: PathBuf,
    pub text: std::sync::Arc<str>,
    pub hash: String,
    pub chapters: Vec<Chapter>,
    pub encoding: String,
}

pub fn decode(raw: &[u8], label: &str) -> Result<String, String> {
    let encoding = if label != "auto" {
        encoding_rs::Encoding::for_label(label.as_bytes()).ok_or("未知编码")?
    } else if raw.starts_with(&[0xff, 0xfe]) {
        encoding_rs::UTF_16LE
    } else if raw.starts_with(&[0xfe, 0xff]) {
        encoding_rs::UTF_16BE
    } else if std::str::from_utf8(raw).is_ok() {
        encoding_rs::UTF_8
    } else {
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(raw, true);
        detector.guess(Some(b"cn"), true)
    };
    let (text, _, errors) = encoding.decode(raw);
    if errors {
        return Err(format!(
            "{} 解码出现无法识别的字符，请手动选择编码",
            encoding.name()
        ));
    }
    Ok(text
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}

pub fn open(path: &Path, encoding: &str, rule: Option<&str>) -> Result<Document, String> {
    let raw = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = decode(&raw, encoding)?;
    let chapters = chapters(&text, rule)?;
    Ok(Document {
        path: path.to_owned(),
        hash: blake3::hash(text.as_bytes()).to_hex().to_string(),
        text: text.into(),
        chapters,
        encoding: encoding.into(),
    })
}

pub fn chapters(text: &str, rule: Option<&str>) -> Result<Vec<Chapter>, String> {
    let custom = rule
        .filter(|s| !s.trim().is_empty())
        .map(Regex::new)
        .transpose()
        .map_err(|e| e.to_string())?;
    let normal = Regex::new(r"(?i)^(?:正文\s*)?(?:第\s*[0-9０-９零〇○一二两兩三四五六七八九十百千万萬亿億壹贰貳叁參肆伍陆陸柒捌玖拾佰仟]+\s*[章回节節卷部篇集话話]|(?:chapter|part|book|volume)\s+(?:[0-9]+|[ivxlcdm]+)\b|(?:序章|序言|楔子|引子|前言|尾声|尾聲|后记|後記|番外)(?:$|[\s：:（(]))").unwrap();
    let volume = Regex::new(r"(?i)^(?:第\s*\S+?\s*[卷部篇]|(?:part|book|volume)\s)").unwrap();
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let title = line.trim();
        let matches = if let Some(rule) = &custom {
            rule.is_match(title)
        } else {
            title.chars().count() <= 80 && normal.is_match(title)
        };
        if matches {
            out.push(Chapter {
                title: title.into(),
                start: offset + line.len() - line.trim_start().len(),
                level: u8::from(!volume.is_match(title)),
            });
        }
        offset += line.len();
    }
    if out.is_empty() {
        out.push(Chapter {
            title: "全文".into(),
            start: 0,
            level: 0,
        });
    }
    Ok(out)
}

pub fn boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub fn search(doc: &Document, needle: &str, limit: usize) -> Vec<(usize, String)> {
    if needle.is_empty() {
        return Vec::new();
    }
    doc.text
        .match_indices(needle)
        .take(limit)
        .map(|(offset, _)| {
            let start = boundary(&doc.text, offset.saturating_sub(60));
            let end = boundary(&doc.text, (offset + needle.len() + 90).min(doc.text.len()));
            let chapter = doc
                .chapters
                .partition_point(|c| c.start <= offset)
                .saturating_sub(1);
            (
                offset,
                format!(
                    "{}  ·  {}",
                    doc.chapters[chapter].title,
                    doc.text[start..end].replace('\n', " ")
                ),
            )
        })
        .collect()
}
