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
//
// ── OCR natif Windows (fallback « PDF scanné ») ──────────────────────────────
//
// 100 % API Windows (aucune DLL tierce) :
//  • `Windows.Data.Pdf`  → rend chaque page du PDF en image (moteur Windows,
//    le même que Edge/Reader).
//  • `Windows.Media.Ocr` → OCR de l'image (hors-ligne, gratuit). La langue
//    dépend des langues OCR installées (fr-FR si le paquet est présent) —
//    `OcrEngine::TryCreateFromUserProfileLanguages`.
//
// Module compilé UNIQUEMENT sur Windows (`cfg`). Tout échec est retourné en
// `Err` : l'appelant (extract.rs) retombe alors sur le message « PDF scanné »
// existant — repli gracieux garanti.
//
// NB : les opérations WinRT asynchrones sont attendues en *blocage* via
// `futures::executor::block_on` (type windows-future) — à utiliser depuis un
// thread de type `spawn_blocking`, jamais sur un worker async.
// ============================================================================

#![cfg(target_os = "windows")]

use std::future::Future;
use std::path::Path;

use windows::core::{Interface, Result as WinResult};
use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Graphics::Imaging::{BitmapDecoder, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Storage::StorageFile;
use windows::Storage::Streams::{IRandomAccessStream, InMemoryRandomAccessStream};

/// Bloque sur une opération asynchrone WinRT (rendue Future par windows-future).
fn block_on<T>(fut: impl Future<Output = WinResult<T>>) -> WinResult<T> {
    futures::executor::block_on(fut)
}

fn ctx_err(context: &str, e: windows::core::Error) -> String {
    format!("OCR Windows - {context} : {e}")
}

/// OCRise un fichier PDF (scanné) : rend jusqu'à `max_pages` pages puis OCR.
pub fn ocr_pdf_file(path: &Path, max_pages: usize) -> Result<String, String> {
    let hpath = windows::core::HSTRING::from(path.to_string_lossy().as_ref());
    let op = StorageFile::GetFileFromPathAsync(&hpath)
        .map_err(|e| ctx_err("ouverture du fichier", e))?;
    let file = block_on(async move { op.await })
        .map_err(|e| ctx_err("ouverture du fichier (async)", e))?;

    let op = PdfDocument::LoadFromFileAsync(&file)
        .map_err(|e| ctx_err("chargement du PDF", e))?;
    let doc = block_on(async move { op.await })
        .map_err(|e| ctx_err("chargement du PDF (async)", e))?;

    let count = doc.PageCount().map_err(|e| ctx_err("nb de pages", e))?;
    if count == 0 {
        return Err("OCR Windows - PDF vide.".into());
    }
    let pages = count.min(max_pages as u32);

    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| ctx_err("moteur OCR indisponible (langue OCR installee ?)", e))?;

    let mut all = String::new();
    for idx in 0..pages {
        let page = doc.GetPage(idx).map_err(|e| ctx_err("lecture page", e))?;
        let text = match ocr_one_page(&engine, &page) {
            Ok(t) => t,
            Err(_) => String::new(), // page illisible : non bloquante
        };
        let _ = page.Close();
        if !text.trim().is_empty() {
            if !all.is_empty() {
                all.push('\n');
            }
            all.push_str(text.trim());
        }
    }
    if all.trim().is_empty() {
        Err("OCR Windows - aucun texte detecte sur les pages.".into())
    } else {
        Ok(all)
    }
}

/// Rend une page PDF dans un flux mémoire, décode l'image puis OCRise.
fn ocr_one_page(engine: &OcrEngine, page: &windows::Data::Pdf::PdfPage) -> Result<String, String> {
    let mem =
        InMemoryRandomAccessStream::new().map_err(|e| ctx_err("flux memoire", e))?;
    let stream: IRandomAccessStream = mem.cast().map_err(|e| ctx_err("conversion flux", e))?;

    // Options : largeur cible ~2200 px (bonne densite pour l'OCR).
    let opts = PdfPageRenderOptions::new().map_err(|e| ctx_err("options rendu", e))?;
    opts.SetDestinationWidth(2200)
        .map_err(|e| ctx_err("largeur rendu", e))?;

    let op = page
        .RenderWithOptionsToStreamAsync(&stream, &opts)
        .map_err(|e| ctx_err("rendu page", e))?;
    block_on(async move { op.await }).map_err(|e| ctx_err("rendu page (async)", e))?;

    stream.Seek(0).map_err(|e| ctx_err("rewind flux", e))?;

    let op = BitmapDecoder::CreateAsync(&stream).map_err(|e| ctx_err("decodage image", e))?;
    let decoder = block_on(async move { op.await })
        .map_err(|e| ctx_err("decodage image (async)", e))?;

    let op = decoder
        .GetSoftwareBitmapAsync()
        .map_err(|e| ctx_err("bitmap", e))?;
    let bmp: SoftwareBitmap =
        block_on(async move { op.await }).map_err(|e| ctx_err("bitmap (async)", e))?;

    let op = engine
        .RecognizeAsync(&bmp)
        .map_err(|e| ctx_err("reconnaissance", e))?;
    let result: OcrResult =
        block_on(async move { op.await }).map_err(|e| ctx_err("reconnaissance (async)", e))?;

    let text = result.Text().map_err(|e| ctx_err("texte", e))?.to_string();
    Ok(text)
}

/// OCRise une image déjà décodée en bitmap (préparé par l'appelant).
#[allow(dead_code)]
pub fn ocr_software_bitmap(bmp: &SoftwareBitmap) -> Result<String, String> {
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()
        .map_err(|e| ctx_err("moteur OCR indisponible (langue OCR installee ?)", e))?;
    let op = engine
        .RecognizeAsync(bmp)
        .map_err(|e| ctx_err("reconnaissance", e))?;
    let result: OcrResult =
        block_on(async move { op.await }).map_err(|e| ctx_err("reconnaissance (async)", e))?;
    Ok(result
        .Text()
        .map_err(|e| ctx_err("texte", e))?
        .to_string())
}
