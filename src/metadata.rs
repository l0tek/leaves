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

/// Small local context for metadata suggestions; no OCR or MOBI text decoding.
pub fn excerpt(format: &str, bytes: &[u8]) -> String {
    let mut text = String::new();
    if format == "epub" {
        if let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) {
            let names = epub_reading_order(&mut archive);
            for name in names {
                if let Some(xml) = zip_text(&mut archive, &name) {
                    if let Ok(doc) = roxmltree::Document::parse(&xml) {
                        let mut section = String::new();
                        for node in doc.descendants().filter(|n| n.is_text() && !n.ancestors().any(|a| a.has_tag_name("script") || a.has_tag_name("style"))) {
                            section.push_str(node.text().unwrap_or_default());
                            section.push(' ');
                            if section.len() >= 16000 { break; }
                        }
                        text.push_str(&clean(&section, 2000));
                        text.push(' ');
                        if text.chars().count() >= 8000 { break; }
                    }
                }
                if text.chars().count() >= 8000 { break; }
            }
        }
    } else if format == "pdf" {
        if let Ok(doc) = lopdf::Document::load_mem(bytes) {
            let pages: Vec<u32> = doc.get_pages().keys().copied().take(3).collect();
            text = doc.extract_text_with_limit(&pages, 2 * 1024 * 1024).unwrap_or_default();
        }
    }
    clean(&text, 8000)
}

// Follow the EPUB spine instead of ZIP entry order (which often starts with navigation).
fn epub_reading_order(archive: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Vec<String> {
    let ordered = (|| -> Option<Vec<String>> {
        let container = zip_text(archive, "META-INF/container.xml")?;
        let container = roxmltree::Document::parse(&container).ok()?;
        let package_path = container.descendants().find(|n| n.has_tag_name("rootfile"))?.attribute("full-path")?;
        let package = zip_text(archive, package_path)?;
        let doc = roxmltree::Document::parse(&package).ok()?;
        let base = std::path::Path::new(package_path).parent()?;
        let mut result = Vec::new();
        for itemref in doc.descendants().filter(|n| n.has_tag_name("itemref") && n.attribute("linear") != Some("no")) {
            let Some(id) = itemref.attribute("idref") else { continue; };
            let Some(item) = doc.descendants().find(|n| n.has_tag_name("item") && n.attribute("id") == Some(id)) else { continue; };
            if item.attribute("media-type") != Some("application/xhtml+xml") || item.attribute("properties").unwrap_or("").split_whitespace().any(|p| p == "nav") { continue; }
            let Some(href) = item.attribute("href") else { continue; };
            let joined = base.join(href.split('#').next().unwrap_or(href));
            let mut parts = Vec::new();
            for part in joined.components() {
                match part { std::path::Component::ParentDir => { parts.pop(); }, std::path::Component::Normal(p) => parts.push(p.to_string_lossy().to_string()), _ => () }
            }
            result.push(parts.join("/"));
            if result.len() == 6 { break; }
        }
        Some(result)
    })().unwrap_or_default();
    if ordered.is_empty() {
        archive.file_names().filter(|n| [".xhtml", ".html", ".htm"].iter().any(|ext| n.to_lowercase().ends_with(ext))).take(6).map(str::to_owned).collect()
    } else { ordered }
}

#[cfg(test)]
mod excerpt_tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn epub_context_follows_spine_and_keeps_later_sections() {
        let mut output = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut output);
            let files = [
                ("nav.xhtml", "<html><body>Falsche Archiv-Reihenfolge</body></html>".to_string()),
                ("META-INF/container.xml", "<container><rootfiles><rootfile full-path=\"OPS/book.opf\"/></rootfiles></container>".to_string()),
                ("OPS/book.opf", "<package><manifest><item id=\"first\" href=\"first.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"second\" href=\"../second.xhtml\" media-type=\"application/xhtml+xml\"/></manifest><spine><itemref idref=\"first\"/><itemref idref=\"second\"/></spine></package>".to_string()),
                ("OPS/first.xhtml", format!("<html><body>{}</body></html>", "Werbung ".repeat(2000))),
                ("second.xhtml", "<html><body><h1>Der richtige Buchanfang</h1><script>nicht aufnehmen</script><p>Wichtiger Inhalt.</p></body></html>".to_string()),
            ];
            for (name, text) in files {
                zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
                zip.write_all(text.as_bytes()).unwrap();
            }
            zip.finish().unwrap();
        }
        let text = excerpt("epub", &output.into_inner());
        assert!(text.contains("Der richtige Buchanfang"));
        assert!(!text.contains("Falsche Archiv-Reihenfolge"));
        assert!(!text.contains("nicht aufnehmen"));
        assert!(text.chars().count() <= 8000);
    }
}
