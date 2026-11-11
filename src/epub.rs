//! EPUB 2/3 text import. No files are extracted, scripts run, or network URLs fetched.
use crate::book::{Chapter, Document};
use std::{
    collections::{BTreeMap, HashMap},
    fs::File,
    io::Read,
    path::Path,
};
const ENTRY_LIMIT: u64 = 16 * 1024 * 1024;
const BOOK_LIMIT: usize = 128 * 1024 * 1024;
struct Archive {
    zip: zip::ZipArchive<File>,
    read: usize,
}
impl Archive {
    fn read(&mut self, name: &str) -> Result<String, String> {
        let mut f = self.zip.by_name(name).map_err(|e| {
            if e.to_string().to_lowercase().contains("password") {
                "EPUB 已加密，暂不支持受保护书籍".into()
            } else {
                format!("EPUB 文件 {name} 无法读取：{e}")
            }
        })?;
        if f.size() > ENTRY_LIMIT {
            return Err(format!("EPUB 章节或元数据过大：{name}"));
        }
        let mut bytes = Vec::new();
        f.by_ref()
            .take(ENTRY_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("EPUB 读取失败：{e}"))?;
        self.read = self.read.saturating_add(bytes.len());
        if bytes.len() as u64 > ENTRY_LIMIT || self.read > BOOK_LIMIT {
            return Err("EPUB 解压正文超过大小限制".into());
        }
        crate::book::decode(&bytes, "auto")
    }
    fn exists(&mut self, name: &str) -> bool {
        self.zip.by_name(name).is_ok()
    }
}
fn xml(text: &str) -> Result<roxmltree::Document<'_>, String> {
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|e| format!("EPUB XML 格式错误：{e}"))
}
fn resolve(base: &str, href: &str) -> Result<(String, String), String> {
    let (uri, fragment) = href.split_once('#').unwrap_or((href, ""));
    let uri = uri.split('?').next().unwrap_or(uri);
    let uri = percent_encoding::percent_decode_str(uri)
        .decode_utf8()
        .map_err(|_| "EPUB 路径编码无效")?;
    let fragment = percent_encoding::percent_decode_str(fragment)
        .decode_utf8()
        .map_err(|_| "EPUB 锚点编码无效")?
        .into_owned();
    if uri.contains([':', '\\', '\0']) {
        return Err("EPUB 路径包含外部地址或非法字符".into());
    }
    let mut parts: Vec<&str> = if uri.is_empty() {
        base.split('/').collect()
    } else if uri.starts_with('/') {
        Vec::new()
    } else {
        base.rsplit_once('/')
            .map_or(Vec::new(), |(p, _)| p.split('/').collect())
    };
    if !uri.is_empty() {
        for part in uri.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err("EPUB 路径越出书籍范围".into());
                    }
                }
                p => parts.push(p),
            }
        }
    }
    Ok((parts.join("/"), fragment))
}
#[derive(Default)]
struct Node {
    name: String,
    attrs: BTreeMap<String, String>,
    text: String,
    children: Vec<usize>,
}
struct Html {
    nodes: Vec<Node>,
}
impl Html {
    fn parse(text: &str) -> Result<Self, String> {
        use htmlparser::{ElementEnd, Token};
        let mut nodes = vec![Node::default()];
        let mut stack = vec![0];
        let mut pending = None;
        static IGNORED: std::sync::OnceLock<[regex::Regex; 2]> = std::sync::OnceLock::new();
        let patterns = IGNORED.get_or_init(|| {
            [
                regex::Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").unwrap(),
                regex::Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").unwrap(),
            ]
        });
        let scripts = patterns[0].replace_all(text, "");
        let clean = patterns[1].replace_all(&scripts, "");
        for token in htmlparser::Tokenizer::from(clean.as_ref()) {
            match token.map_err(|e| format!("EPUB XHTML 格式错误：{e}"))? {
                Token::ElementStart { local, .. } => {
                    if stack.len() > 128 || nodes.len() > 500000 {
                        return Err("EPUB 文档结构过于复杂".into());
                    }
                    let i = nodes.len();
                    nodes.push(Node {
                        name: local.as_str().to_ascii_lowercase(),
                        ..Default::default()
                    });
                    nodes[*stack.last().unwrap()].children.push(i);
                    pending = Some(i);
                }
                Token::Attribute { local, value, .. } => {
                    if let Some(i) = pending {
                        nodes[i].attrs.insert(
                            local.as_str().to_ascii_lowercase(),
                            html_escape::decode_html_entities(value.map_or("", |v| v.as_str()))
                                .into_owned(),
                        );
                    }
                }
                Token::ElementEnd {
                    end: ElementEnd::Open,
                    ..
                } => {
                    if let Some(i) = pending.take()
                        && !matches!(
                            nodes[i].name.as_str(),
                            "br" | "hr" | "img" | "meta" | "link" | "input" | "source" | "wbr"
                        )
                    {
                        stack.push(i);
                    }
                }
                Token::ElementEnd {
                    end: ElementEnd::Empty,
                    ..
                } => {
                    pending = None;
                }
                Token::ElementEnd {
                    end: ElementEnd::Close(_, local),
                    ..
                } => {
                    pending = None;
                    if let Some(i) = stack
                        .iter()
                        .rposition(|&i| nodes[i].name == local.as_str().to_ascii_lowercase())
                    {
                        stack.truncate(i.max(1));
                    }
                }
                Token::Text { text } => {
                    let i = nodes.len();
                    nodes.push(Node {
                        text: html_escape::decode_html_entities(text.as_str()).into_owned(),
                        ..Default::default()
                    });
                    nodes[*stack.last().unwrap()].children.push(i);
                }
                Token::Cdata { text, .. } => {
                    let i = nodes.len();
                    nodes.push(Node {
                        text: text.as_str().to_owned(),
                        ..Default::default()
                    });
                    nodes[*stack.last().unwrap()].children.push(i);
                }
                _ => {}
            }
        }
        Ok(Self { nodes })
    }
    fn label(&self, i: usize) -> String {
        fn visit(h: &Html, i: usize, s: &mut String) {
            if matches!(h.nodes[i].name.as_str(), "rt" | "rp")
                || h.nodes[i].attrs.contains_key("hidden")
            {
                return;
            }
            s.push_str(&h.nodes[i].text);
            for &c in &h.nodes[i].children {
                visit(h, c, s)
            }
        }
        let mut s = String::new();
        visit(self, i, &mut s);
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    }
    fn body(&self) -> Result<usize, String> {
        self.nodes
            .iter()
            .position(|n| n.name == "body")
            .ok_or_else(|| "EPUB 内容缺少 body，可能为损坏或受保护书籍".into())
    }
}
#[derive(Default)]
struct Text {
    body: String,
    space: bool,
    anchors: HashMap<String, usize>,
    headings: Vec<Chapter>,
}
impl Text {
    fn line(&mut self) {
        self.space = false;
        if !self.body.is_empty() && !self.body.ends_with('\n') {
            self.body.push('\n');
        }
    }
    fn text(&mut self, s: &str, pre: bool) {
        for c in s.chars() {
            if pre {
                self.body.push(c);
            } else if c.is_ascii_whitespace() || c == '\u{a0}' {
                self.space = true;
            } else {
                if self.space && !self.body.is_empty() && !self.body.ends_with('\n') {
                    self.body.push(' ');
                }
                self.space = false;
                self.body.push(c);
            }
        }
    }
    fn visit(&mut self, h: &Html, i: usize, pre: bool) {
        let n = &h.nodes[i];
        if matches!(
            n.name.as_str(),
            "head" | "script" | "style" | "nav" | "svg" | "rt" | "rp"
        ) || n.attrs.contains_key("hidden")
            || n.attrs.get("aria-hidden").is_some_and(|v| v == "true")
            || n.attrs.get("style").is_some_and(|v| {
                let v = v.to_ascii_lowercase().replace(' ', "");
                v.contains("display:none") || v.contains("visibility:hidden")
            })
        {
            return;
        }
        let block = matches!(
            n.name.as_str(),
            "p" | "div"
                | "section"
                | "article"
                | "li"
                | "blockquote"
                | "pre"
                | "tr"
                | "dt"
                | "dd"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        );
        if block {
            self.line();
        }
        for key in ["id", "name"] {
            if let Some(id) = n.attrs.get(key) {
                self.anchors.entry(id.clone()).or_insert(self.body.len());
            }
        }
        if matches!(n.name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            let title = h.label(i);
            if !title.is_empty() {
                self.headings.push(Chapter {
                    title,
                    start: self.body.len(),
                    level: n.name.as_bytes()[1] - b'1',
                });
            }
        }
        if n.name == "br" {
            self.space = false;
            self.body.push('\n');
        }
        self.text(&n.text, pre || n.name == "pre");
        for &c in &n.children {
            self.visit(h, c, pre || n.name == "pre");
        }
        if block {
            self.line();
        }
    }
}
struct Item {
    path: String,
    media: String,
    nav: bool,
}
struct Section {
    path: String,
    start: usize,
    anchors: HashMap<String, usize>,
    fallback: Vec<Chapter>,
}
struct Toc {
    title: String,
    path: String,
    fragment: String,
    level: u8,
}
fn nav_toc(source: &str, path: &str) -> Result<Vec<Toc>, String> {
    let h = Html::parse(source)?;
    let mut out = Vec::new();
    fn walk(h: &Html, i: usize, path: &str, depth: u8, out: &mut Vec<Toc>) -> Result<(), String> {
        let n = &h.nodes[i];
        if n.name == "a"
            && let Some(href) = n.attrs.get("href")
            && let Ok((path, fragment)) = resolve(path, href)
        {
            let title = h.label(i);
            if !title.is_empty() {
                out.push(Toc {
                    title,
                    path,
                    fragment,
                    level: depth.saturating_sub(1),
                });
            }
        }
        let depth = depth.saturating_add(u8::from(n.name == "ol"));
        for &c in &n.children {
            walk(h, c, path, depth, out)?;
        }
        Ok(())
    }
    if let Some(i) = h.nodes.iter().position(|n| {
        n.name == "nav"
            && (n
                .attrs
                .get("type")
                .is_some_and(|t| t.split_whitespace().any(|v| v == "toc"))
                || n.attrs.get("role").is_some_and(|r| r == "doc-toc"))
    }) {
        walk(&h, i, path, 0, &mut out)?;
    }
    Ok(out)
}
fn ncx_toc(source: &str, path: &str) -> Result<Vec<Toc>, String> {
    let d = xml(source)?;
    let mut out = Vec::new();
    for n in d.descendants().filter(|n| n.has_tag_name("navPoint")) {
        if out.len() >= 50000 || n.ancestors().take(130).count() >= 130 {
            return Err("EPUB 目录结构过于复杂".into());
        }
        let label = n
            .children()
            .find(|n| n.has_tag_name("navLabel"))
            .map(|n| {
                n.descendants()
                    .filter_map(|n| n.text().filter(|_| n.is_text()))
                    .collect::<String>()
            })
            .unwrap_or_default();
        let href = n
            .children()
            .find(|n| n.has_tag_name("content"))
            .and_then(|n| n.attribute("src"));
        if let Some(href) = href
            && let Ok((path, fragment)) = resolve(path, href)
        {
            let title = label.split_whitespace().collect::<Vec<_>>().join(" ");
            if !title.is_empty() {
                out.push(Toc {
                    title,
                    path,
                    fragment,
                    level: n
                        .ancestors()
                        .filter(|n| n.has_tag_name("navPoint"))
                        .count()
                        .saturating_sub(1)
                        .min(255) as u8,
                });
            }
        }
    }
    Ok(out)
}
pub(crate) fn open(path: &Path) -> Result<Document, String> {
    let zip = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| format!("EPUB 压缩包损坏或不受支持：{e}"))?;
    if zip.len() > 50000 {
        return Err("EPUB 文件数量过多".into());
    }
    let mut a = Archive { zip, read: 0 };
    let container = a.read("META-INF/container.xml")?;
    let d = xml(&container)?;
    let opf = d
        .descendants()
        .find(|n| {
            n.has_tag_name("rootfile")
                && n.attribute("media-type") == Some("application/oebps-package+xml")
        })
        .or_else(|| d.descendants().find(|n| n.has_tag_name("rootfile")))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("EPUB container 缺少书籍入口")?;
    let (opf, _) = resolve("", opf)?;
    let package = a.read(&opf)?;
    let d = xml(&package)?;
    let mut items = HashMap::new();
    for n in d.descendants().filter(|n| n.has_tag_name("item")) {
        if let (Some(id), Some(href)) = (n.attribute("id"), n.attribute("href")) {
            let (path, _) = match resolve(&opf, href) {
                Ok(path) => path,
                Err(_)
                    if (href.contains("://") || href.starts_with("data:"))
                        && !matches!(
                            n.attribute("media-type"),
                            Some(
                                "application/xhtml+xml" | "text/html" | "application/x-dtbncx+xml"
                            )
                        ) =>
                {
                    continue;
                }
                Err(e) => return Err(e),
            };
            if items
                .insert(
                    id.to_owned(),
                    Item {
                        path,
                        media: n.attribute("media-type").unwrap_or("").into(),
                        nav: n
                            .attribute("properties")
                            .is_some_and(|p| p.split_whitespace().any(|v| v == "nav")),
                    },
                )
                .is_some()
            {
                return Err("EPUB manifest 包含重复标识".into());
            }
        }
    }
    let spine = d
        .descendants()
        .find(|n| n.has_tag_name("spine"))
        .ok_or("EPUB 缺少阅读顺序 spine")?;
    let ordered: Vec<&Item> = spine
        .children()
        .filter(|n| n.has_tag_name("itemref") && n.attribute("linear") != Some("no"))
        .map(|n| {
            n.attribute("idref")
                .and_then(|id| items.get(id))
                .ok_or_else(|| "EPUB 阅读顺序引用的章节不存在".to_string())
        })
        .collect::<Result<_, _>>()?;
    if a.exists("META-INF/encryption.xml") {
        let encrypted = a.read("META-INF/encryption.xml")?;
        for n in xml(&encrypted)?
            .descendants()
            .filter(|n| n.has_tag_name("CipherReference"))
        {
            if let Some(uri) = n.attribute("URI") {
                let (path, _) = resolve("", uri)?;
                if ordered.iter().any(|i| i.path == path) {
                    return Err("EPUB 正文已加密，暂不支持受保护书籍".into());
                }
            }
        }
    }
    let mut toc = Vec::new();
    if let Some(item) = items.values().find(|i| i.nav)
        && let Ok(content) = a.read(&item.path)
    {
        toc = nav_toc(&content, &item.path).unwrap_or_default();
    }
    if toc.is_empty()
        && let Some(item) = spine
            .attribute("toc")
            .and_then(|id| items.get(id))
            .or_else(|| {
                items
                    .values()
                    .find(|i| i.media == "application/x-dtbncx+xml")
            })
        && let Ok(content) = a.read(&item.path)
    {
        toc = ncx_toc(&content, &item.path).unwrap_or_default();
    }
    let mut body = String::new();
    let mut sections = Vec::new();
    for item in ordered {
        if !matches!(item.media.as_str(), "application/xhtml+xml" | "text/html") {
            continue;
        }
        let source = a.read(&item.path)?;
        let h = Html::parse(&source)?;
        let mut text = Text::default();
        text.visit(&h, h.body()?, false);
        while text.body.ends_with('\n') {
            text.body.pop();
        }
        if text.body.trim().is_empty() {
            continue;
        }
        if !body.is_empty() {
            body.push_str("\n\n");
        }
        let start = body.len();
        if text.headings.is_empty() {
            let title = h
                .nodes
                .iter()
                .position(|n| n.name == "title")
                .map(|i| h.label(i))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| item.path.rsplit('/').next().unwrap_or("章节").to_owned());
            text.headings.push(Chapter {
                title,
                start: 0,
                level: 0,
            });
        }
        for c in &mut text.headings {
            c.start += start;
        }
        sections.push(Section {
            path: item.path.clone(),
            start,
            anchors: text.anchors,
            fallback: text.headings,
        });
        body.push_str(&text.body);
    }
    if body.trim().is_empty() {
        return Err("EPUB 中没有可阅读文字，暂不支持图片型或扫描书籍".into());
    }
    let mut chapters = Vec::new();
    let mut by_path: HashMap<&str, Vec<&Toc>> = HashMap::new();
    for t in &toc {
        by_path.entry(&t.path).or_default().push(t);
    }
    for section in sections {
        let mut native = Vec::new();
        for t in by_path.get(section.path.as_str()).into_iter().flatten() {
            let local = if t.fragment.is_empty() {
                0
            } else {
                section.anchors.get(&t.fragment).copied().unwrap_or(0)
            };
            native.push(Chapter {
                title: t.title.clone(),
                start: (section.start + local).min(body.len()),
                level: t.level,
            });
        }
        chapters.extend(if native.is_empty() {
            section.fallback
        } else {
            native
        });
    }
    if chapters
        .first()
        .is_some_and(|_| chapters.iter().all(|c| c.start > 0))
    {
        chapters.push(Chapter {
            title: "开篇".into(),
            start: 0,
            level: 0,
        });
    }
    chapters.sort_by_key(|c| c.start);
    chapters.dedup_by(|a, b| a.start == b.start && a.title == b.title && a.level == b.level);
    if chapters.is_empty() {
        chapters.push(Chapter {
            title: "全文".into(),
            start: 0,
            level: 0,
        });
    }
    Ok(Document {
        path: path.to_owned(),
        hash: blake3::hash(body.as_bytes()).to_hex().to_string(),
        text: body.into(),
        chapters,
        encoding: "EPUB".into(),
    })
}
