// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
// Année   : 2026
// ============================================================================

// Vérification et installation des mises à jour du pack de MCP officiels
// (dépôt `demeter-mcp-pack` sur GitHub — voir son README pour le cycle de
// release complet). Ce module ne gère QUE la vérification/mise à jour à
// l'exécution : l'installation initiale embarquée dans Demeter passe par
// `scripts/fetch-mcp-pack.sh` au build, et `StdioManager::install_official`
// (mcp_stdio.rs) pour la copie non-destructive vers le dossier utilisateur —
// ce module réutilise cette même fonction, avec pour source un dossier
// temporaire fraîchement téléchargé plutôt que les resources de l'app.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;

/// Dépôt GitHub hébergeant les releases du pack. À adapter à votre
/// organisation (voir aussi `MCP_PACK_REPO` dans scripts/fetch-mcp-pack.sh —
/// idéalement la même valeur, mais celle-ci est indépendante : ce module
/// tourne dans le binaire déjà construit, l'autre au moment du build).
pub const PACK_REPO: &str = "Ktulu-Analog/demeter-mcp-pack";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Résultat d'une vérification de mise à jour.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PackUpdateStatus {
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
}

/// Interroge la dernière release publiée du pack et la compare à la version
/// actuellement installée dans le dossier MCP de l'utilisateur. Ne fait
/// jamais échouer l'appelant : en cas d'erreur réseau, `latest` reste `None`
/// (on ne bloque jamais l'usage de Demeter pour un problème de connectivité).
pub async fn check_update(client: &reqwest::Client, installed: Option<String>) -> PackUpdateStatus {
    let latest = fetch_latest_release(client)
        .await
        .ok()
        .map(|r| r.tag_name.trim_start_matches('v').to_string());

    let update_available = match (&installed, &latest) {
        (Some(i), Some(l)) => i != l,
        (None, Some(_)) => true,
        _ => false,
    };

    PackUpdateStatus {
        installed,
        latest,
        update_available,
    }
}

async fn fetch_latest_release(client: &reqwest::Client) -> Result<GithubRelease, String> {
    let url = format!("https://api.github.com/repos/{PACK_REPO}/releases/latest");
    let resp = client
        .get(&url)
        .header("User-Agent", "Demeter")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("GitHub a répondu {}", resp.status()));
    }

    resp.json::<GithubRelease>()
        .await
        .map_err(|e| e.to_string())
}

/// Télécharge la dernière release du pack, vérifie son checksum SHA-256
/// (fichier `checksums.txt` publié à côté de l'archive), l'extrait dans un
/// dossier temporaire, puis délègue l'installation non-destructive à
/// `StdioManager::install_official` (même logique que pour les MCP
/// embarqués — un serveur déjà configuré n'est jamais écrasé).
pub async fn download_and_install(
    client: &reqwest::Client,
    mcp_stdio: &crate::mcp_stdio::StdioManager,
) -> Result<Vec<String>, String> {
    let release = fetch_latest_release(client).await?;
    let version = release.tag_name.trim_start_matches('v');

    let archive_name = format!("mcp-pack-{version}.zip");
    let archive_asset = release
        .assets
        .iter()
        .find(|a| a.name == archive_name)
        .ok_or_else(|| {
            format!(
                "Archive '{archive_name}' introuvable dans la release {}",
                release.tag_name
            )
        })?;
    let checksums_asset = release
        .assets
        .iter()
        .find(|a| a.name == "checksums.txt")
        .ok_or("Fichier checksums.txt introuvable dans la release")?;

    let archive_bytes = download(client, &archive_asset.browser_download_url).await?;
    let checksums_text =
        String::from_utf8(download(client, &checksums_asset.browser_download_url).await?)
            .map_err(|e| format!("checksums.txt illisible : {e}"))?;

    verify_checksum(&archive_bytes, &archive_name, &checksums_text)?;

    let tmp_dir = std::env::temp_dir().join(format!("demeter-mcp-pack-update-{version}"));
    if tmp_dir.exists() {
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    extract_zip(&archive_bytes, &tmp_dir)?;
    // Marqueur relu par `installed_pack_version()` après copie.
    let _ = std::fs::write(tmp_dir.join("VERSION"), version);

    let installed = mcp_stdio.install_official(&tmp_dir).await?;
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(installed)
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "Demeter")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Téléchargement échoué ({}) : {url}", resp.status()));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

fn verify_checksum(bytes: &[u8], filename: &str, checksums_text: &str) -> Result<(), String> {
    let expected = checksums_text
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts
                .next()?
                .trim_start_matches('*')
                .trim_start_matches("./");
            if name == filename {
                Some(hash.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("Aucune entrée pour '{filename}' dans checksums.txt"))?;

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = hex_encode(&hasher.finalize());

    if actual != expected.to_lowercase() {
        return Err(format!(
            "Checksum invalide pour '{filename}' (attendu {expected}, obtenu {actual}) — fichier corrompu ou altéré, installation annulée."
        ));
    }
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    archive.extract(dest).map_err(|e| e.to_string())?;
    Ok(())
}
