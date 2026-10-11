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
    // D-13 : l'étiquette vient d'une API distante, elle entre dans des noms de
    // fichiers : on n'accepte qu'un numéro de version.
    if !is_valid_version(version) {
        return Err(format!(
            "Étiquette de release invalide : '{}'",
            release.tag_name
        ));
    }

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

    let archive_bytes = download(
        client,
        &archive_asset.browser_download_url,
        MAX_ARCHIVE_BYTES,
    )
    .await?;
    let checksums_text = String::from_utf8(
        download(
            client,
            &checksums_asset.browser_download_url,
            MAX_CHECKSUMS_BYTES,
        )
        .await?,
    )
    .map_err(|e| format!("checksums.txt illisible : {e}"))?;

    // D-13 : authenticité. Le SHA-256 ci-dessous vient de la MÊME release que
    // l'archive : il ne protège que de la corruption. C'est la signature minisign
    // de checksums.txt, vérifiée avec une clé publique embarquée dans
    // l'application, qui protège d'une release compromise.
    ensure_signed(client, &release, version, checksums_text.as_bytes()).await?;

    verify_checksum(&archive_bytes, &archive_name, &checksums_text)?;

    // D-13 : dossier temporaire à nom ALÉATOIRE, créé en exclusivité (échoue s'il
    // existe déjà) et en 0700 : un autre compte local ne peut ni le pré-créer ni
    // y substituer des fichiers entre l'extraction et l'installation.
    let tmp_dir = create_private_tmp_dir()?;

    let result = async {
        extract_zip(&archive_bytes, &tmp_dir)?;
        // Marqueur relu par `installed_pack_version()` après copie.
        let _ = std::fs::write(tmp_dir.join("VERSION"), version);
        mcp_stdio.install_official(&tmp_dir).await
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

async fn download(
    client: &reqwest::Client,
    url: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let resp = client
        .get(url)
        .header("User-Agent", "Demeter")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Téléchargement échoué ({}) : {url}", resp.status()));
    }
    read_capped(resp, max_bytes).await
}

/// Lit le corps d'une réponse en refusant de dépasser `max_bytes` (D-13).
async fn read_capped(mut resp: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>, String> {
    if resp
        .content_length()
        .map(|l| l as usize > max_bytes)
        .unwrap_or(false)
    {
        return Err(format!(
            "Fichier trop volumineux (max {} Mo)",
            max_bytes / 1024 / 1024
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > max_bytes {
            return Err(format!(
                "Fichier trop volumineux (max {} Mo)",
                max_bytes / 1024 / 1024
            ));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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

// ── Authenticité du pack : signature minisign (D-13) ──────────────────────────

/// Clé publique minisign embarquée (fichier `src-tauri/mcp-pack.pub`).
const EMBEDDED_PUBLIC_KEY_FILE: &str = include_str!("../mcp-pack.pub");

/// Extrait la clé publique (base64) d'un fichier `.pub` minisign : on ignore les
/// lignes vides, les commentaires `#` et la ligne `untrusted comment:`.
fn parse_public_key_file(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("untrusted comment:"))
        .map(str::to_string)
}

/// Vérifie la signature de `data` : clé valide, signature valide ET commentaire
/// signé (« trusted comment ») portant `version:<version>`. Ce dernier point
/// empêche de rejouer, sous une nouvelle étiquette, une ancienne release
/// légitimement signée mais vulnérable (rétrogradation).
fn verify_pack_signature(
    public_key_b64: &str,
    data: &[u8],
    signature_text: &str,
    version: &str,
) -> Result<(), String> {
    let key = minisign_verify::PublicKey::from_base64(public_key_b64)
        .map_err(|e| format!("Clé publique du pack invalide : {e}"))?;
    let sig = minisign_verify::Signature::decode(signature_text)
        .map_err(|e| format!("Signature illisible : {e}"))?;
    key.verify(data, &sig, false)
        .map_err(|_| "Signature invalide : la release n'est pas authentique".to_string())?;
    let expected = format!("version:{version}");
    if !sig
        .trusted_comment()
        .split_whitespace()
        .any(|w| w == expected)
    {
        return Err(format!(
            "Signature valide mais pour une autre version (attendu « {expected} ») : \
             rétrogradation refusée"
        ));
    }
    Ok(())
}

/// Exige que `checksums.txt` soit signé, sauf dérogation explicite.
async fn ensure_signed(
    client: &reqwest::Client,
    release: &GithubRelease,
    version: &str,
    checksums: &[u8],
) -> Result<(), String> {
    let allow_unsigned = std::env::var("DEMETER_ALLOW_UNSIGNED_PACK")
        .map(|v| v == "1")
        .unwrap_or(false);
    let Some(key) = parse_public_key_file(EMBEDDED_PUBLIC_KEY_FILE) else {
        if allow_unsigned {
            tracing::warn!(
                "Pack MCP installé SANS vérification de signature (DEMETER_ALLOW_UNSIGNED_PACK=1)"
            );
            return Ok(());
        }
        return Err(
            "Mise à jour en ligne refusée : aucune clé publique de signature n'est configurée \
             dans cette version de Demeter (src-tauri/mcp-pack.pub). Le pack embarqué reste \
             utilisable. Voir docs/SECURITE_PACK_MCP.md."
                .to_string(),
        );
    };
    let sig_asset = release
        .assets
        .iter()
        .find(|a| a.name == "checksums.txt.minisig")
        .ok_or("Release non signée : checksums.txt.minisig introuvable")?;
    let sig_bytes = download(client, &sig_asset.browser_download_url, MAX_CHECKSUMS_BYTES).await?;
    let sig_text =
        String::from_utf8(sig_bytes).map_err(|e| format!("Signature illisible : {e}"))?;
    verify_pack_signature(&key, checksums, &sig_text, version)
}

/// Plafonds de l'archive (D-13).
const MAX_ARCHIVE_BYTES: usize = 300 * 1024 * 1024;
const MAX_CHECKSUMS_BYTES: usize = 1024 * 1024;
const MAX_UNCOMPRESSED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 10_000;

/// Un numéro de version : « 1.2.3 », éventuellement suivi de « -rc.1 » / « +build ».
fn is_valid_version(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or("");
    let parts: Vec<&str> = core.split('.').collect();
    v.len() <= 40
        && parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

/// Crée un dossier temporaire privé (nom aléatoire, 0700 sous Unix).
fn create_private_tmp_dir() -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join(format!(
        "demeter-mcp-pack-{}",
        uuid::Uuid::new_v4().simple()
    ));
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(&dir)
        .map_err(|e| format!("Dossier temporaire impossible : {e}"))?;
    Ok(dir)
}

/// Extraction manuelle et durcie (D-13) : noms « zip slip » refusés, liens
/// symboliques refusés, nombre d'entrées et taille décompressée plafonnés.
fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if archive.len() > MAX_ENTRIES {
        return Err("Archive invalide : trop d'entrées".into());
    }
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| format!("Archive invalide : chemin dangereux '{}'", entry.name()))?;
        if entry
            .unix_mode()
            .map(|m| m & 0o170000 == 0o120000)
            .unwrap_or(false)
        {
            return Err(format!(
                "Archive invalide : lien symbolique '{}'",
                entry.name()
            ));
        }
        let out = dest.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err("Archive invalide : taille décompressée excessive".into());
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        // `take` : on ne croit pas la taille annoncée par l'entrée
        let mut limited = std::io::Read::take(&mut entry, MAX_UNCOMPRESSED_BYTES + 1);
        let written = std::io::copy(&mut limited, &mut f).map_err(|e| e.to_string())?;
        if written > MAX_UNCOMPRESSED_BYTES {
            return Err("Archive invalide : taille décompressée excessive".into());
        }
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode & 0o755));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            for (name, data) in entries {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    // Vecteurs de test : clés jetables générées avec `minisign -G -W` (aucun lien avec
    // la clé de production). Données signées : « abc123  mcp-pack-1.2.3.zip\n ».
    const T_PUB: &str = "RWT/0cFDCithUuW1hKg/D3v7FBwYBH4nx5hZfco5NgdJcGLLIcC39Usq";
    const T_OTHER_PUB: &str = "RWRCnL2OMi8Sdl47EWlv8E+TDpS03ARwG4XKij9kmeJbYsEyHfxnoZ4N";
    const T_DATA: &[u8] = b"abc123  mcp-pack-1.2.3.zip\n";
    const T_SIG: &str = "untrusted comment: signature from minisign secret key\n\
RUT/0cFDCithUp+whX3DcWZcCMilO6TtFVHI98s5dPKgv7Mw0bXCtcfdwUxnJuvD69Bb+j1uK4UjEg3M/YVPN8NvM2u70X+oyAc=\n\
trusted comment: demeter-mcp-pack version:1.2.3\n\
nqUc9kIQ+2CyxbM48tSJhXloJEOqUVLQTISqsHpc+Y9icjE0PO1s1Xgn1O+fQPalRxb/xMenmLlxoTyw5qjSCQ==\n";

    #[test]
    fn signature_valide_acceptee() {
        assert!(verify_pack_signature(T_PUB, T_DATA, T_SIG, "1.2.3").is_ok());
    }

    #[test]
    fn signature_refuse_donnees_modifiees() {
        assert!(
            verify_pack_signature(T_PUB, b"abc123  mcp-pack-1.2.4.zip\n", T_SIG, "1.2.3").is_err()
        );
    }

    #[test]
    fn signature_refuse_autre_cle() {
        assert!(verify_pack_signature(T_OTHER_PUB, T_DATA, T_SIG, "1.2.3").is_err());
    }

    #[test]
    fn signature_refuse_retrogradation() {
        // même release signée, présentée sous une autre étiquette
        let e = verify_pack_signature(T_PUB, T_DATA, T_SIG, "1.2.4").unwrap_err();
        assert!(e.contains("rétrogradation"), "{e}");
    }

    #[test]
    fn signature_refuse_commentaire_modifie() {
        let forged = T_SIG.replace("version:1.2.3", "version:9.9.9");
        assert!(verify_pack_signature(T_PUB, T_DATA, &forged, "9.9.9").is_err());
    }

    #[test]
    fn cle_publique_du_depot_est_un_modele_vide_ou_valide() {
        // Le fichier livré est un modèle (aucune clé) ou contient une clé valide.
        match parse_public_key_file(EMBEDDED_PUBLIC_KEY_FILE) {
            None => {}
            Some(k) => assert!(minisign_verify::PublicKey::from_base64(&k).is_ok()),
        }
        assert_eq!(
            parse_public_key_file("untrusted comment: x\nRWABC\n").as_deref(),
            Some("RWABC")
        );
        assert_eq!(parse_public_key_file("# seul un commentaire\n\n"), None);
    }

    #[test]
    fn versions_valides_et_invalides() {
        for ok in ["1.2.3", "10.0.1", "1.2.3-rc.1", "2.0.0+build5"] {
            assert!(is_valid_version(ok), "{ok}");
        }
        for bad in [
            "",
            "1.2",
            "v1.2.3",
            "../../x",
            "1.2.3/../..",
            "1.2.3 ",
            "a.b.c",
            "1.2.3;rm",
        ] {
            assert!(!is_valid_version(bad), "{bad:?}");
        }
    }

    #[test]
    fn extraction_refuse_zip_slip() {
        let dir = create_private_tmp_dir().unwrap();
        let z = make_zip(&[("../evil.txt", b"x")]);
        assert!(extract_zip(&z, &dir).is_err());
        assert!(!dir.parent().unwrap().join("evil.txt").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extraction_normale() {
        let dir = create_private_tmp_dir().unwrap();
        let z = make_zip(&[
            ("mcp-a/manifest.json", b"{}"),
            ("mcp-a/bin/run", b"#!/bin/sh"),
        ]);
        extract_zip(&z, &dir).unwrap();
        assert!(dir.join("mcp-a/manifest.json").exists());
        assert!(dir.join("mcp-a/bin/run").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dossiers_temporaires_distincts() {
        let a = create_private_tmp_dir().unwrap();
        let b = create_private_tmp_dir().unwrap();
        assert_ne!(a, b);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }
}
