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
/// Taille décompressée maximale de word/document.xml (64 Mo).
const MAX_DOCX_XML_BYTES: u64 = 64 * 1024 * 1024;

pub fn extract_text(content: &[u8], ext: &str) -> Result<String> {
    let text = match ext {
        "pdf" => extract_pdf(content)?,
        "docx" | "doc" => extract_docx(content)?,
        "txt" | "md" | "markdown" | "rst" | "csv" => {
            // Texte brut — UTF-8 avec fallback latin-1
            match std::str::from_utf8(content) {
                Ok(s) => s.to_string(),
                Err(_) => content.iter().map(|&b| b as char).collect(),
            }
        }
        other => bail!(
            "Format non supporté : .{other}. Formats acceptés : pdf, docx, doc, txt, md, csv."
        ),
    };

    if text.trim().is_empty() {
        bail!("Le fichier ne contient pas de texte extractible.");
    }

    if text.len() > MAX_CHARS {
        // D-18 : on coupe sur une frontière de caractère (couper au milieu d'un
        // caractère UTF-8 multi-octet faisait paniquer le programme).
        let mut cut = MAX_CHARS;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        Ok(format!(
            "{}\n\n[... tronqué à {} caractères ...]",
            &text[..cut],
            MAX_CHARS
        ))
    } else {
        Ok(text)
    }
}

// ── PDF extraction ────────────────────────────────────────────────────────────

fn extract_pdf(content: &[u8]) -> Result<String> {
    // pdf-extract gère les encodages CID, ToUnicode, Type1, TrueType, etc.
    match pdf_extract::extract_text_from_mem(content) {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        Ok(_) => {
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
        // D-18 : lecture plafonnée (bombe ZIP) ; décodage UTF-8 tolérant
        use std::io::Read;
        let mut raw = Vec::new();
        (&mut file)
            .take(MAX_DOCX_XML_BYTES + 1)
            .read_to_end(&mut raw)?;
        if raw.len() as u64 > MAX_DOCX_XML_BYTES {
            bail!("DOCX trop volumineux une fois décompressé");
        }
        String::from_utf8_lossy(&raw).into_owned()
    };

    Ok(docx_xml_to_text(&xml))
}

fn docx_xml_to_text(xml: &str) -> String {
    // Extract text runs from XML, preserving paragraph breaks
    // D-18 : on accumule des OCTETS puis on décode en UTF-8 (l'ancien `octet as char`
    // transformait chaque accent en deux caractères parasites).
    let mut text: Vec<u8> = Vec::new();
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
                text.push(b'\n');
                after_para = true;
            }

            i = tag_end + 1;
        } else {
            if in_w_t {
                // Decode XML entities inline
                text.push(bytes[i]);
                after_para = false;
            }
            i += 1;
        }
    }

    // Clean up XML entities
    String::from_utf8_lossy(&text)
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Version asynchrone : l'extraction (PDF, ZIP) est du calcul lourd et peut
/// paniquer sur un fichier malformé. Elle s'exécute hors du thread asynchrone,
/// avec un délai maximal, et une panique devient une simple erreur (D-18).
pub async fn extract_text_async(content: Vec<u8>, ext: String) -> Result<String> {
    let task = tokio::task::spawn_blocking(move || extract_text(&content, &ext));
    match tokio::time::timeout(std::time::Duration::from_secs(120), task).await {
        Ok(Ok(res)) => res,
        Ok(Err(_)) => bail!("Extraction interrompue : fichier probablement malformé"),
        Err(_) => bail!("Extraction trop longue (délai de 120 s dépassé)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn troncature_sur_frontiere_utf8() {
        // 'é' = 2 octets : le MAX_CHARS-ième octet tombe au milieu d'un caractère
        let big = "é".repeat(MAX_CHARS);
        let out = extract_text(big.as_bytes(), "txt").unwrap();
        assert!(out.contains("tronqué"));
    }

    #[test]
    fn docx_conserve_les_accents() {
        let xml = "<w:p><w:r><w:t>Été à l'hôpital — œuvre</w:t></w:r></w:p>";
        assert_eq!(docx_xml_to_text(xml).trim(), "Été à l'hôpital — œuvre");
    }

    /// PDF minimal valide construit à la main (table xref calculée) : vérifie que
    /// l'extraction fonctionne réellement avec la version de pdf-extract en place.
    fn make_pdf(text: &str) -> Vec<u8> {
        let stream = format!("BT /F1 18 Tf 20 50 Td ({text}) Tj ET");
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 100] /Contents 4 0 R \
             /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_string(),
            format!(
                "<< /Length {} >>\nstream\n{}\nendstream",
                stream.len(),
                stream
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(
            format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes(),
        );
        for off in offsets {
            out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .as_bytes(),
        );
        out
    }

    #[tokio::test]
    async fn extraction_pdf_reelle() {
        let pdf = make_pdf("Bonjour Demeter");
        let text = extract_text_async(pdf, "pdf".into()).await.unwrap();
        assert!(text.contains("Bonjour"), "texte extrait : {text:?}");
    }

    #[tokio::test]
    async fn extraction_async_ok_et_erreur() {
        let ok = extract_text_async(b"bonjour".to_vec(), "txt".into())
            .await
            .unwrap();
        assert_eq!(ok, "bonjour");
        assert!(extract_text_async(b"x".to_vec(), "exe".into())
            .await
            .is_err());
        // PDF invalide : erreur, jamais de panique propagée
        assert!(extract_text_async(b"%PDF-bogus".to_vec(), "pdf".into())
            .await
            .is_err());
    }
}
