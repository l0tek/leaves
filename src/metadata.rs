//! Best-effort metadata extraction. Malformed/unsupported files never prevent upload.
use std::io::{Cursor, Read};

#[derive(Default, Debug)]
pub struct Metadata { pub title: String, pub author: String, pub subjects: Vec<String> }

pub fn extract(format: &str, bytes: &[u8]) -> Metadata {
    let mut result = match format {
        "epub" => epub(bytes),
        "pdf" => lopdf::Document::load_metadata_mem(bytes).ok().map(|m| Metadata {
            title: m.title.unwrap_or_default(), author: m.author.unwrap_or_default(), subjects: Vec::new(),
        }),
        "mobi" | "azw3" => mobi(bytes),
        _ => None,
    }.unwrap_or_default();
    result.title = clean(&result.title, 300);
    result.author = clean(&result.author, 200);
    result
}

fn clean(text: &str, limit: usize) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).take(limit).collect()
}

fn zip_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Option<String> {
    let file = archive.by_name(name).ok()?;
    // Read only small metadata files, never unpack the archive to disk.
    const MAX_XML: u64 = 2 * 1024 * 1024;
    if file.size() > MAX_XML { return None; }
    let mut text = String::new();
    file.take(MAX_XML + 1).read_to_string(&mut text).ok()?;
    if text.len() as u64 > MAX_XML { return None; }
    Some(text)
}

fn epub(bytes: &[u8]) -> Option<Metadata> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let container = zip_text(&mut archive, "META-INF/container.xml")?;
    let doc = roxmltree::Document::parse(&container).ok()?;
    let path = doc.descendants().find(|n| n.has_tag_name("rootfile"))?.attribute("full-path")?;
    let package = zip_text(&mut archive, path)?;
    let doc = roxmltree::Document::parse(&package).ok()?;
    let metadata = doc.descendants().find(|n| n.has_tag_name("metadata"))?;
    const DC: &str = "http://purl.org/dc/elements/1.1/";
    let title = metadata.children().find(|n| n.has_tag_name((DC, "title")))
        .and_then(|n| n.text()).unwrap_or_default().to_string();
    let authors: Vec<_> = metadata.children().filter(|n| n.has_tag_name((DC, "creator")))
        .filter_map(|n| n.text()).collect();
    let subjects = metadata.children().filter(|n| n.has_tag_name((DC, "subject")))
        .filter_map(|n| n.text()).take(100).map(str::to_string).collect();
    Some(Metadata { title, author: authors.join(", "), subjects })
}

fn be32(bytes: &[u8], offset: usize) -> Option<usize> {
    Some(u32::from_be_bytes(bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?) as usize)
}

fn mobi(bytes: &[u8]) -> Option<Metadata> {
    // Palm database record table -> PalmDOC header -> MOBI header -> EXTH records.
    if bytes.get(60..68)? != b"BOOKMOBI" { return None; }
    let records = u16::from_be_bytes(bytes.get(76..78)?.try_into().ok()?) as usize;
    if records == 0 { return None; }
    let start = be32(bytes, 78)?;
    let end = if records > 1 { be32(bytes, 86)? } else { bytes.len() };
    let record = bytes.get(start..end)?;
    if record.get(16..20)? != b"MOBI" { return None; }
    let header_len = be32(record, 20)?;
    if header_len < 116 { return None; }
    let encoding = be32(record, 28)?;
    let decode = |data: &[u8]| -> String {
        if encoding == 1252 { encoding_rs::WINDOWS_1252.decode(data).0.into_owned() }
        else { String::from_utf8_lossy(data).into_owned() }
    };
    let mut result = Metadata::default();
    if let (Some(offset), Some(len)) = (be32(record, 84), be32(record, 88)) {
        if let Some(title) = offset.checked_add(len).and_then(|end| record.get(offset..end)) { result.title = decode(title); }
    }
    if be32(record, 128)? & 0x40 == 0 { return Some(result); }
    let exth_start = 16usize.checked_add(header_len)?;
    let exth = record.get(exth_start..)?;
    if exth.get(..4)? != b"EXTH" { return Some(result); }
    let length = be32(exth, 4)?;
    let count = be32(exth, 8)?;
    let exth = exth.get(..length)?;
    let mut offset = 12usize;
    let mut authors = Vec::new();
    for _ in 0..count.min(4096) {
        let kind = be32(exth, offset)?;
        let len = be32(exth, offset.checked_add(4)?)?;
        if len < 8 { return Some(result); }
        let end = offset.checked_add(len)?;
        let data = exth.get(offset.checked_add(8)?..end)?;
        match kind {
            100 => authors.push(decode(data)),
            503 => result.title = decode(data),
            _ => (),
        }
        offset = end;
    }
    result.author = authors.join(", ");
    Some(result)
}
