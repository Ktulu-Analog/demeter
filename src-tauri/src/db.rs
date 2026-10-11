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

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use tokio_rusqlite::Connection;
use tracing::{info, warn};

use crate::models::{
    CollectionMeta, CollectionVisibility, Conversation, ConversationSave, Message,
};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS conversations (
    id         TEXT PRIMARY KEY,
    title      TEXT NOT NULL,
    space_id   TEXT,
    messages   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS collection_meta (
    collection_name TEXT PRIMARY KEY,
    owner_id        TEXT NOT NULL,
    visibility      TEXT NOT NULL DEFAULT 'private',
    shared_with     TEXT NOT NULL DEFAULT '[]',
    created_at      TEXT NOT NULL
);";

// ── Chiffrement au repos (D-09) ───────────────────────────────────────────────

/// Vrai si le fichier est une base SQLite NON chiffrée (en-tête en clair).
pub fn is_plaintext_sqlite(path: &Path) -> bool {
    use std::io::Read;
    let mut header = [0u8; 16];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut header))
        .map(|_| &header == b"SQLite format 3\0")
        .unwrap_or(false)
}

/// Littéral SQL de la clé pour `PRAGMA key` / `ATTACH … KEY`.
/// Une clé de 64 caractères hexadécimaux est passée brute (sans dérivation) ;
/// toute autre valeur est une phrase secrète (apostrophes doublées).
fn key_sql_literal(key: &str) -> String {
    if key.len() == 64 && key.chars().all(|c| c.is_ascii_hexdigit()) {
        format!("\"x'{key}'\"")
    } else {
        format!("'{}'", key.replace('\'', "''"))
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    path.with_file_name(name)
}

fn count_rows(conn: &rusqlite::Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .unwrap_or(0)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}
#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

/// Migre une base en clair vers une base SQLCipher, avec vérification.
///
/// 1. export chiffré dans un fichier temporaire (`sqlcipher_export`) ;
/// 2. vérification : le fichier s'ouvre avec la clé et contient le même nombre de
///    conversations et de métadonnées ;
/// 3. l'original est conservé en `*.pre-encryption.bak` (0600), la base chiffrée
///    prend sa place. La sauvegarde en clair est SUPPRIMÉE (écrasée) au lancement
///    suivant par `purge_pre_encryption_backup`, une fois la base chiffrée éprouvée.
fn migrate_plaintext(path: &Path, key: &str) -> Result<()> {
    let tmp = sibling(path, "enc-tmp");
    let bak = sibling(path, "pre-encryption.bak");
    if bak.exists() {
        bail!(
            "Une sauvegarde {} existe déjà : vérifiez-la puis supprimez-la avant de relancer.",
            bak.display()
        );
    }
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", tmp.display()));
    }

    let (conv_before, meta_before) = {
        let plain = rusqlite::Connection::open(path).context("ouverture de la base en clair")?;
        let _ = plain.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
        let counts = (
            count_rows(&plain, "conversations"),
            count_rows(&plain, "collection_meta"),
        );
        let tmp_sql = tmp.to_string_lossy().replace('\'', "''");
        plain.execute_batch(&format!(
            "ATTACH DATABASE '{tmp_sql}' AS encrypted KEY {};",
            key_sql_literal(key)
        ))?;
        plain.execute_batch("SELECT sqlcipher_export('encrypted');")?;
        plain.execute_batch("DETACH DATABASE encrypted;")?;
        counts
    };

    // Vérification de la copie chiffrée
    {
        let enc = rusqlite::Connection::open(&tmp).context("ouverture de la base chiffrée")?;
        enc.execute_batch(&format!("PRAGMA key = {};", key_sql_literal(key)))?;
        enc.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })
        .context("la copie chiffrée est illisible")?;
        let (c, m) = (
            count_rows(&enc, "conversations"),
            count_rows(&enc, "collection_meta"),
        );
        if (c, m) != (conv_before, meta_before) {
            let _ = std::fs::remove_file(&tmp);
            bail!("Vérification échouée : {c}/{m} lignes copiées sur {conv_before}/{meta_before}");
        }
    }

    restrict_permissions(&tmp);
    std::fs::rename(path, &bak).context("mise de côté de la base en clair")?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::rename(&bak, path); // retour arrière
        bail!("installation de la base chiffrée impossible : {e}");
    }
    for suffix in ["-wal", "-shm", "-journal"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
    restrict_permissions(&bak);
    info!(
        "Base de conversations chiffrée ({conv_before} conversation(s)) ; sauvegarde en clair \
         conservée jusqu'au prochain lancement : {}",
        bak.display()
    );
    Ok(())
}

/// Prépare le fichier de base avant ouverture : migre une base en clair si la
/// clé est fournie. Retourne `true` si une migration vient d'être effectuée.
pub fn prepare_file(path: &Path, key: Option<&str>) -> Result<bool> {
    let Some(key) = key else { return Ok(false) };
    let usable = path.exists()
        && std::fs::metadata(path)
            .map(|m| m.len() > 0)
            .unwrap_or(false);
    if usable && is_plaintext_sqlite(path) {
        migrate_plaintext(path, key)?;
        return Ok(true);
    }
    Ok(false)
}

/// Supprime la sauvegarde en clair laissée par la migration : le contenu est
/// écrasé par des zéros avant suppression (au mieux : sur SSD, le remplacement
/// physique des blocs n'est pas garanti).
pub fn purge_pre_encryption_backup(path: &Path) -> bool {
    use std::io::Write;
    let bak = sibling(path, "pre-encryption.bak");
    let Ok(meta) = std::fs::metadata(&bak) else {
        return false;
    };
    let overwritten = (|| -> std::io::Result<()> {
        let mut f = std::fs::OpenOptions::new().write(true).open(&bak)?;
        let block = vec![0u8; 1 << 20];
        let mut left = meta.len();
        while left > 0 {
            let n = left.min(block.len() as u64) as usize;
            f.write_all(&block[..n])?;
            left -= n as u64;
        }
        f.sync_all()
    })();
    if let Err(e) = overwritten {
        warn!("Écrasement de la sauvegarde en clair impossible : {e}");
    }
    match std::fs::remove_file(&bak) {
        Ok(()) => {
            info!("Sauvegarde en clair de l'ancienne base supprimée");
            true
        }
        Err(e) => {
            warn!("Suppression de la sauvegarde en clair impossible : {e}");
            false
        }
    }
}

#[derive(Clone)]
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Ouvre (ou crée) la base. `key` = clé SQLCipher ; `None` = base non chiffrée
    /// (dérogation explicite, voir `secrets.rs`).
    pub async fn new(path: &Path, key: Option<&str>) -> Result<Self> {
        let conn = Connection::open(path).await?;
        let pragma = key.map(|k| format!("PRAGMA key = {};", key_sql_literal(k)));

        conn.call(move |db| {
            if let Some(p) = &pragma {
                db.execute_batch(p)?;
            }
            // Échoue ici si la clé est mauvaise (« file is not a database »).
            db.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })?;
            db.execute_batch(SCHEMA)?;
            Ok(())
        })
        .await
        .context("ouverture de la base (clé incorrecte ou fichier corrompu ?)")?;

        restrict_permissions(path);
        info!(
            "SQLite initialised at {} ({})",
            path.display(),
            if key.is_some() {
                "chiffrée"
            } else {
                "NON chiffrée"
            }
        );
        Ok(Self { conn })
    }

    /// Base en mémoire (mode dégradé : rien n'est écrit sur le disque).
    pub async fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().await?;
        conn.call(|db| {
            db.execute_batch(SCHEMA)?;
            Ok(())
        })
        .await?;
        Ok(Self { conn })
    }

    // ── collection_meta ───────────────────────────────────────────────────────

    pub async fn upsert_collection_meta(&self, meta: CollectionMeta) -> Result<()> {
        let shared = serde_json::to_string(&meta.shared_with).unwrap_or_else(|_| "[]".into());
        let vis = meta.visibility.as_str().to_string();
        self.conn
            .call(move |db| {
                db.execute(
                    "INSERT INTO collection_meta (collection_name, owner_id, visibility, shared_with, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(collection_name) DO UPDATE SET
                         visibility  = excluded.visibility,
                         shared_with = excluded.shared_with",
                    rusqlite::params![meta.collection_name, meta.owner_id, vis, shared, meta.created_at],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn get_collection_meta(&self, name: String) -> Result<Option<CollectionMeta>> {
        let row = self
            .conn
            .call(move |db| {
                let mut stmt = db.prepare(
                    "SELECT collection_name, owner_id, visibility, shared_with, created_at
                     FROM collection_meta WHERE collection_name = ?1",
                )?;
                let mut rows = stmt.query_map(rusqlite::params![name], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                    ))
                })?;
                Ok(rows.next().transpose()?)
            })
            .await?;

        Ok(
            row.map(|(n, owner, vis, shared, created_at)| CollectionMeta {
                collection_name: n,
                owner_id: owner,
                visibility: CollectionVisibility::from_str(&vis),
                shared_with: serde_json::from_str(&shared).unwrap_or_default(),
                created_at,
            }),
        )
    }

    pub async fn list_collection_meta(&self) -> Result<Vec<CollectionMeta>> {
        let rows = self
            .conn
            .call(|db| {
                let mut stmt = db.prepare(
                    "SELECT collection_name, owner_id, visibility, shared_with, created_at
                     FROM collection_meta",
                )?;
                let rows = stmt
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, String>(3)?,
                            r.get::<_, String>(4)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await?;

        Ok(rows
            .into_iter()
            .map(|(n, owner, vis, shared, created_at)| CollectionMeta {
                collection_name: n,
                owner_id: owner,
                visibility: CollectionVisibility::from_str(&vis),
                shared_with: serde_json::from_str(&shared).unwrap_or_default(),
                created_at,
            })
            .collect())
    }

    pub async fn delete_collection_meta(&self, name: String) -> Result<()> {
        self.conn
            .call(move |db| {
                db.execute(
                    "DELETE FROM collection_meta WHERE collection_name = ?1",
                    rusqlite::params![name],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn list_conversations(&self) -> Result<Vec<Conversation>> {
        let rows = self
            .conn
            .call(|db| {
                let mut stmt = db.prepare(
                    "SELECT id, title, space_id, messages, created_at, updated_at
                     FROM conversations ORDER BY updated_at DESC",
                )?;
                let rows = stmt
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                            row.get::<_, String>(5)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(rows)
            })
            .await?;

        let mut conversations = Vec::new();
        for (id, title, space_id, messages_json, created_at, updated_at) in rows {
            let messages: Vec<Message> = serde_json::from_str(&messages_json).unwrap_or_default();
            conversations.push(Conversation {
                id,
                title,
                space_id,
                messages,
                created_at,
                updated_at,
            });
        }
        Ok(conversations)
    }

    pub async fn save_conversation(&self, conv: ConversationSave) -> Result<()> {
        let messages_json =
            serde_json::to_string(&conv.messages).unwrap_or_else(|_| "[]".to_string());
        self.conn
            .call(move |db| {
                db.execute(
                    "INSERT INTO conversations (id, title, space_id, messages, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                     ON CONFLICT(id) DO UPDATE SET
                         title      = excluded.title,
                         space_id   = excluded.space_id,
                         messages   = excluded.messages,
                         updated_at = excluded.updated_at",
                    rusqlite::params![
                        conv.id,
                        conv.title,
                        conv.space_id,
                        messages_json,
                        conv.created_at,
                        conv.updated_at,
                    ],
                )?;
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub async fn delete_conversation(&self, id: String) -> Result<bool> {
        let count = self
            .conn
            .call(move |db| {
                let n = db.execute(
                    "DELETE FROM conversations WHERE id = ?1",
                    rusqlite::params![id],
                )?;
                Ok(n)
            })
            .await?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn tmp_dir() -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("demeter-db-test-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_plain_db(path: &Path) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch(SCHEMA).unwrap();
        c.execute_batch(
            "INSERT INTO conversations VALUES ('c1','Titre secret','s','[]','2026-01-01','2026-01-01');
             INSERT INTO conversations VALUES ('c2','Autre','s','[]','2026-01-01','2026-01-01');
             INSERT INTO collection_meta VALUES ('rh','alice@x.fr','private','[]','2026-01-01');",
        )
        .unwrap();
    }

    #[tokio::test]
    async fn migration_chiffre_et_conserve_les_donnees() {
        let dir = tmp_dir();
        let path = dir.join("conversations.db");
        make_plain_db(&path);
        assert!(is_plaintext_sqlite(&path));

        let migrated = prepare_file(&path, Some(KEY)).unwrap();
        assert!(migrated);
        assert!(
            !is_plaintext_sqlite(&path),
            "l'en-tête ne doit plus être en clair"
        );
        // le contenu en clair n'apparaît plus dans le fichier
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(12).any(|w| w == b"Titre secret"));

        // lisible avec la clé
        let db = Database::new(&path, Some(KEY)).await.unwrap();
        let list = db.list_conversations().await.unwrap();
        assert_eq!(list.len(), 2);

        // illisible sans clé ou avec une mauvaise clé
        assert!(Database::new(&path, None).await.is_err());
        assert!(
            Database::new(&path, Some("une-autre-cle-de-plus-de-16-caracteres"))
                .await
                .is_err()
        );

        // la sauvegarde en clair existe puis est purgée
        let bak = sibling(&path, "pre-encryption.bak");
        assert!(bak.exists());
        assert!(purge_pre_encryption_backup(&path));
        assert!(!bak.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn nouvelle_base_est_chiffree_et_sans_migration() {
        let dir = tmp_dir();
        let path = dir.join("conversations.db");
        assert!(!prepare_file(&path, Some(KEY)).unwrap());
        let db = Database::new(&path, Some(KEY)).await.unwrap();
        drop(db);
        assert!(!is_plaintext_sqlite(&path));
        // une base déjà chiffrée n'est pas « migrée » une seconde fois
        assert!(!prepare_file(&path, Some(KEY)).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn phrase_secrete_avec_apostrophe() {
        let dir = tmp_dir();
        let path = dir.join("conversations.db");
        let k = "phrase d'essai assez longue";
        drop(Database::new(&path, Some(k)).await.unwrap());
        assert!(Database::new(&path, Some(k)).await.is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn base_en_memoire() {
        let db = Database::in_memory().await.unwrap();
        assert!(db.list_conversations().await.unwrap().is_empty());
    }
}
