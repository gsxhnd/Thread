//! Multi-format project export (txt / md / epub / docx).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::db::new_id;
use crate::error::{AppError, AppResult};
use crate::models::ExportResult;
use crate::services::chapter::chapter_tree;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPayload {
    pub project_id: String,
    pub format: String,
    pub scope: ExportScope,
    pub target_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportScope {
    pub kind: String, // all | volume | chapters
    pub volume_id: Option<String>,
    pub chapter_ids: Option<Vec<String>>,
}

struct ExportChapter {
    volume_title: String,
    title: String,
    content: String,
}

pub fn export_run(
    app: &AppHandle,
    conn: &Connection,
    payload: ExportPayload,
) -> AppResult<ExportResult> {
    let task_id = new_id();
    let _ = app.emit(
        "export://progress",
        serde_json::json!({ "taskId": task_id, "percent": 5 }),
    );

    let project_title: String = conn.query_row(
        "SELECT title FROM projects WHERE id = ?1 AND deleted_at IS NULL",
        params![payload.project_id],
        |r| r.get(0),
    )?;

    let chapters = collect_chapters(conn, &payload.project_id, &payload.scope)?;
    let _ = app.emit(
        "export://progress",
        serde_json::json!({ "taskId": task_id, "percent": 40 }),
    );

    let target = PathBuf::from(&payload.target_path);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    match payload.format.as_str() {
        "txt" => write_txt(&target, &project_title, &chapters)?,
        "md" => write_md(&target, &project_title, &chapters)?,
        "epub" => write_epub(&target, &project_title, &chapters)?,
        "docx" => write_docx(&target, &project_title, &chapters)?,
        other => {
            return Err(AppError::Validation(format!(
                "unsupported format: {other}"
            )));
        }
    }

    let _ = app.emit(
        "export://progress",
        serde_json::json!({ "taskId": task_id, "percent": 100 }),
    );

    Ok(ExportResult {
        path: target.to_string_lossy().to_string(),
    })
}

fn collect_chapters(
    conn: &Connection,
    project_id: &str,
    scope: &ExportScope,
) -> AppResult<Vec<ExportChapter>> {
    let tree = chapter_tree(conn, project_id)?;
    let mut out = Vec::new();
    for vw in tree {
        if scope.kind == "volume" {
            if let Some(vid) = &scope.volume_id {
                if &vw.volume.id != vid {
                    continue;
                }
            }
        }
        for ch in &vw.chapters {
            if scope.kind == "chapters" {
                if let Some(ids) = &scope.chapter_ids {
                    if !ids.contains(&ch.id) {
                        continue;
                    }
                }
            }
            let content: String = conn.query_row(
                "SELECT content FROM chapters WHERE id = ?1",
                params![ch.id],
                |r| r.get(0),
            )?;
            out.push(ExportChapter {
                volume_title: vw.volume.title.clone(),
                title: ch.title.clone(),
                content,
            });
        }
    }
    Ok(out)
}

fn write_txt(path: &Path, title: &str, chapters: &[ExportChapter]) -> AppResult<()> {
    let mut body = format!("{title}\n\n");
    let mut last_volume = String::new();
    for ch in chapters {
        if ch.volume_title != last_volume {
            body.push_str(&format!("\n【{}】\n\n", ch.volume_title));
            last_volume = ch.volume_title.clone();
        }
        body.push_str(&format!("{}\n\n{}\n\n", ch.title, strip_md(&ch.content)));
    }
    fs::write(path, body)?;
    Ok(())
}

fn write_md(path: &Path, title: &str, chapters: &[ExportChapter]) -> AppResult<()> {
    let mut body = format!("# {title}\n\n");
    let mut last_volume = String::new();
    for ch in chapters {
        if ch.volume_title != last_volume {
            body.push_str(&format!("## {}\n\n", ch.volume_title));
            last_volume = ch.volume_title.clone();
        }
        body.push_str(&format!("### {}\n\n{}\n\n", ch.title, ch.content));
    }
    fs::write(path, body)?;
    Ok(())
}

fn write_epub(path: &Path, title: &str, chapters: &[ExportChapter]) -> AppResult<()> {
    let file = fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file("mimetype", opts)?;
    zip.write_all(b"application/epub+zip")?;

    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("META-INF/container.xml", opts)?;
    zip.write_all(
        br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )?;

    let mut manifest = String::new();
    let mut spine = String::new();
    let mut toc = String::from("<navMap>\n");
    for (i, ch) in chapters.iter().enumerate() {
        let id = format!("chap{i}");
        let href = format!("chap{i}.xhtml");
        manifest.push_str(&format!(
            r#"    <item id="{id}" href="{href}" media-type="application/xhtml+xml"/>\n"#
        ));
        spine.push_str(&format!(r#"    <itemref idref="{id}"/>\n"#));
        toc.push_str(&format!(
            r#"  <navPoint id="nav{i}" playOrder="{ord}"><navLabel><text>{title}</text></navLabel><content src="{href}"/></navPoint>\n"#,
            ord = i + 1,
            title = xml_escape(&ch.title),
            href = href
        ));

        let xhtml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{title}</title></head>
<body>
<h1>{title}</h1>
<p>{body}</p>
</body>
</html>"#,
            title = xml_escape(&ch.title),
            body = xml_escape(&strip_md(&ch.content)).replace('\n', "</p><p>")
        );
        zip.start_file(format!("OEBPS/{href}"), opts)?;
        zip.write_all(xhtml.as_bytes())?;
    }
    toc.push_str("</navMap>");

    let opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:language>zh</dc:language>
    <dc:identifier id="BookId">urn:uuid:{uuid}</dc:identifier>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
{manifest}  </manifest>
  <spine toc="ncx">
{spine}  </spine>
</package>"#,
        title = xml_escape(title),
        uuid = new_id(),
    );
    zip.start_file("OEBPS/content.opf", opts)?;
    zip.write_all(opf.as_bytes())?;

    let ncx = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="urn:uuid:{uuid}"/></head>
  <docTitle><text>{title}</text></docTitle>
{toc}
</ncx>"#,
        uuid = new_id(),
        title = xml_escape(title),
    );
    zip.start_file("OEBPS/toc.ncx", opts)?;
    zip.write_all(ncx.as_bytes())?;
    zip.finish()?;
    Ok(())
}

fn write_docx(path: &Path, title: &str, chapters: &[ExportChapter]) -> AppResult<()> {
    // Minimal WordprocessingML package.
    let file = fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("[Content_Types].xml", opts)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#,
    )?;

    zip.start_file("_rels/.rels", opts)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#,
    )?;

    let mut paragraphs = format!(
        r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#,
        xml_escape(title)
    );
    for ch in chapters {
        paragraphs.push_str(&format!(
            r#"<w:p><w:r><w:t>{}</w:t></w:r></w:p>"#,
            xml_escape(&ch.title)
        ));
        for line in strip_md(&ch.content).lines() {
            paragraphs.push_str(&format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                xml_escape(line)
            ));
        }
    }

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
{paragraphs}
  </w:body>
</w:document>"#
    );
    zip.start_file("word/document.xml", opts)?;
    zip.write_all(document.as_bytes())?;
    zip.finish()?;
    Ok(())
}

fn strip_md(md: &str) -> String {
    md.replace("**", "")
        .replace('*', "")
        .replace('#', "")
        .replace('`', "")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
