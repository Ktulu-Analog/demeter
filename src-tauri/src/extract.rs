// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Ce fichier fait partie du projet Demeter.
// Vous pouvez le redistribuer et/ou le modifier selon les termes de la
// licence AGPL-3.0 publiée par la Free Software Foundation.
// ============================================================================

use anyhow::{bail, Result};

const MAX_CHARS: usize = 500_000;

pub fn extract_text(content: &[u8], ext: &str) -> Result<String> {
    let text = match ext {
        "pdf" => extract_pdf(content)?,
        "docx" | "doc" => extract_docx(content)?,
        other => bail!("Format non supporté : .{}", other),
    };

    if text.trim().is_empty() {
        bail!("Impossible d'extraire du texte.");
    }

    if text.len() > MAX_CHARS {
        Ok(format!(
            "{}\n\n[... tronqué à {} caractères ...]",
            &text[..MAX_CHARS],
            MAX_CHARS
        ))
    } else {
        Ok(text)
    }
}

// ── PDF extraction ────────────────────────────────────────────────────────────

fn extract_pdf(content: &[u8]) -> Result<String> {
    use lopdf::Document;

    let doc = Document::load_mem(content)?;
    let mut parts = Vec::new();

    let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();

    for page_num in pages {
        match doc.extract_text(&[page_num]) {
            Ok(text) if !text.trim().is_empty() => parts.push(text),
            _ => {}
        }
    }

    Ok(parts.join("\n\n"))
}

// ── DOCX extraction ───────────────────────────────────────────────────────────

fn extract_docx(content: &[u8]) -> Result<String> {
    use std::io::Cursor;
    use zip::ZipArchive;

    let cursor = Cursor::new(content);
    let mut archive = ZipArchive::new(cursor)?;

    // word/document.xml contains the main body text
    let xml = {
        let mut file = match archive.by_name("word/document.xml") {
            Ok(f) => f,
            Err(_) => bail!("word/document.xml introuvable dans le DOCX"),
        };
        let mut buf = String::new();
        use std::io::Read;
        file.read_to_string(&mut buf)?;
        buf
    };

    Ok(docx_xml_to_text(&xml))
}

fn docx_xml_to_text(xml: &str) -> String {
    // Extract text runs from XML, preserving paragraph breaks
    let mut text = String::new();
    let mut in_w_t = false;
    let mut after_para = false;

    let bytes = xml.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Find end of tag
            let tag_start = i + 1;
            let tag_end = match bytes[tag_start..].iter().position(|&b| b == b'>') {
                Some(p) => tag_start + p,
                None => break,
            };
            let tag = &xml[tag_start..tag_end];

            if tag.starts_with("w:t")
                && (tag.len() == 3
                    || tag.as_bytes().get(3) == Some(&b' ')
                    || tag.as_bytes().get(3) == Some(&b'/'))
            {
                in_w_t = !tag.ends_with('/');
                after_para = false;
            } else if tag.starts_with("/w:t") {
                in_w_t = false;
            } else if tag.starts_with("w:p")
                && (tag.len() == 3
                    || tag.as_bytes().get(3) == Some(&b' ')
                    || tag.as_bytes().get(3) == Some(&b'/')
                    || tag.as_bytes().get(3) == Some(&b'>'))
            {
                if !text.is_empty() && !after_para {
                    text.push('\n');
                    after_para = true;
                }
            }

            i = tag_end + 1;
        } else {
            if in_w_t {
                // Decode XML entities inline
                let ch = bytes[i] as char;
                text.push(ch);
                after_para = false;
            }
            i += 1;
        }
    }

    // Clean up XML entities
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}
