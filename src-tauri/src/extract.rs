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
use std::io::{Read, Seek};
use std::path::Path;

const MAX_CHARS: usize = 500_000;
/// Borne de lecture pour les formats texte bruts (on lit au plus ce volume sur
/// disque — le texte est de toute façon tronqué à `MAX_CHARS`).
const MAX_TEXT_READ_BYTES: usize = 2 * 1024 * 1024;

fn decode_text(content: &[u8]) -> String {
    // Texte brut — UTF-8 avec fallback latin-1
    match std::str::from_utf8(content) {
        Ok(s) => s.to_string(),
        Err(_) => content.iter().map(|&b| b as char).collect(),
    }
}

fn unsupported_msg(other: &str) -> String {
    format!("Format non supporté : .{other}. Formats acceptés : pdf, docx, doc, txt, md, csv.")
}

/// Applique les vérifications finales communes (vide + troncature UTF-8 sûre).
fn finalize_text(text: String) -> Result<String> {
    if text.trim().is_empty() {
        bail!("Le fichier ne contient pas de texte extractible.");
    }
    if text.len() > MAX_CHARS {
        let end = text.floor_char_boundary(MAX_CHARS);
        Ok(format!(
            "{}\n\n[... tronqué à {} caractères ...]",
            &text[..end],
            MAX_CHARS
        ))
    } else {
        Ok(text)
    }
}

/// Extraction depuis un tampon en mémoire (flux mono-fichier existant).
pub fn extract_text(content: &[u8], ext: &str) -> Result<String> {
    let text = match ext {
        "pdf" => extract_pdf(content)?,
        "docx" | "doc" => extract_docx(content)?,
        "txt" | "md" | "markdown" | "rst" | "csv" => decode_text(content),
        other => bail!(unsupported_msg(other)),
    };
    finalize_text(text)
}

/// Extraction depuis un fichier déjà spoolé sur disque (feuille d'une archive).
/// Lecture bornée → utilisé pour les gros documents sans tout charger en RAM.
pub fn extract_text_file(path: &Path, ext: &str) -> Result<String> {
    let text = match ext {
        "pdf" => extract_pdf_file(path)?,
        "docx" | "doc" => extract_docx_file(path)?,
        "txt" | "md" | "markdown" | "rst" | "csv" => {
            let raw = read_limited(path, MAX_TEXT_READ_BYTES)?;
            decode_text(&raw)
        }
        other => bail!(unsupported_msg(other)),
    };
    finalize_text(text)
}

/// Lit au plus `cap` octets d'un fichier (RAM bornée pour les très gros textes).
fn read_limited(path: &Path, cap: usize) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut buf = Vec::new();
    file.take(cap as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

// ── PDF extraction ────────────────────────────────────────────────────────────

/// Fallback OCR natif Windows (Windows.Data.Pdf + Windows.Media.Ocr) pour les
/// PDF scannés. Sur non-Windows : aucun OCR → `None` (message d'origine gardé).
#[cfg(target_os = "windows")]
fn ocr_fallback_path(path: &Path) -> Option<String> {
    crate::ocr_windows::ocr_pdf_file(path, crate::config::ocr_max_pages()).ok()
}

#[cfg(not(target_os = "windows"))]
fn ocr_fallback_path(_path: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
fn ocr_fallback_bytes(content: &[u8]) -> Option<String> {
    use std::io::Write;
    let dir = tempfile::Builder::new()
        .prefix("demeter_ocr_")
        .tempdir()
        .ok()?;
    let p = dir.path().join("scan.pdf");
    let mut f = std::fs::File::create(&p).ok()?;
    f.write_all(content).ok()?;
    drop(f);
    crate::ocr_windows::ocr_pdf_file(&p, crate::config::ocr_max_pages()).ok()
}

#[cfg(not(target_os = "windows"))]
fn ocr_fallback_bytes(_content: &[u8]) -> Option<String> {
    None
}

fn extract_pdf(content: &[u8]) -> Result<String> {
    // pdf-extract gère les encodages CID, ToUnicode, Type1, TrueType, etc.
    match pdf_extract::extract_text_from_mem(content) {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        Ok(_) => {
            // PDF scanné → tentative d'OCR natif (Windows), sinon message d'origine.
            if let Some(t) = ocr_fallback_bytes(content) {
                if !t.trim().is_empty() {
                    return Ok(t);
                }
            }
            bail!(
                "Le PDF ne contient pas de texte extractible (PDF scanné ou image). \
                 Utilisez un outil d'OCR avant l'ingestion."
            )
        }
        Err(e) => {
            bail!("Extraction PDF échouée : {}", e)
        }
    }
}

fn extract_pdf_file(path: &Path) -> Result<String> {
    // Variante disque : pdf-extract lit le fichier lui-même.
    match pdf_extract::extract_text(path) {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        Ok(_) => {
            // PDF scanné → tentative d'OCR natif (Windows), sinon message d'origine.
            if let Some(t) = ocr_fallback_path(path) {
                if !t.trim().is_empty() {
                    return Ok(t);
                }
            }
            bail!(
                "Le PDF ne contient pas de texte extractible (PDF scanné ou image). \
                 Utilisez un outil d'OCR avant l'ingestion."
            )
        }
        Err(e) => {
            bail!("Extraction PDF échouée : {}", e)
        }
    }
}

// ── DOCX extraction ───────────────────────────────────────────────────────────

fn read_docx_zip<R: Read + Seek>(source: R) -> Result<String> {
    use zip::ZipArchive;

    let mut archive = ZipArchive::new(source)?;

    // word/document.xml contains the main body text
    let xml = {
        let mut file = match archive.by_name("word/document.xml") {
            Ok(f) => f,
            Err(_) => bail!("word/document.xml introuvable dans le DOCX"),
        };
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        buf
    };

    Ok(docx_xml_to_text(&xml))
}

fn extract_docx(content: &[u8]) -> Result<String> {
    read_docx_zip(std::io::Cursor::new(content))
}

fn extract_docx_file(path: &Path) -> Result<String> {
    read_docx_zip(std::fs::File::open(path)?)
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
                && !text.is_empty()
                && !after_para
            {
                text.push('\n');
                after_para = true;
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
