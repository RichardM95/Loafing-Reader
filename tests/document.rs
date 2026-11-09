use thief_reader::book::*;
#[test]
fn decoding_preserves_paragraphs() {
    assert_eq!(
        decode(b"\xef\xbb\xbfhello\r\nworld", "auto").unwrap(),
        "hello\nworld"
    );
    let (bytes, _, _) = encoding_rs::GBK.encode("第一章 春天");
    assert_eq!(decode(&bytes, "gbk").unwrap(), "第一章 春天");
}
#[test]
fn chapter_titles_exclude_mentions_and_keep_repeated_names() {
    let s = "第一卷 春\n第一章 出发\n他说，第二章还没有写。\n第二章 出发\n第二卷 夏\n第一章 重来\n";
    let c = chapters(s, None).unwrap();
    assert_eq!(c.len(), 5);
    assert_eq!(c[0].level, 0);
    assert_eq!(c[1].level, 1);
    assert_eq!(&s[c[3].start..].lines().next().unwrap(), &"第二卷 夏");
}
#[test]
fn custom_rules_report_errors_and_find_titles() {
    assert!(chapters("text", Some("[")).is_err());
    assert_eq!(
        chapters("001 开始\n正文\n002 继续", Some(r"^\d{3} "))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(chapters("只有正文", None).unwrap()[0].title, "全文");
}
#[test]
fn questions_and_quotes_are_valid_chapter_titles() {
    let text = "第一章 你是谁？\n正文\n第二章 “你好”！\n正文\n";
    assert_eq!(chapters(text, None).unwrap().len(), 2);
}

#[test]
fn utf16_and_big5_preserve_non_latin_text() {
    let expected = "第一章 新開始\n第二段文字";
    let mut raw = vec![0xff, 0xfe];
    for unit in expected.replace('\n', "\r\n").encode_utf16() {
        raw.extend(unit.to_le_bytes());
    }
    assert_eq!(decode(&raw, "auto").unwrap(), expected);
    let (raw, _, bad) = encoding_rs::BIG5.encode(expected);
    assert!(!bad);
    assert_eq!(decode(&raw, "big5").unwrap(), expected);
}
