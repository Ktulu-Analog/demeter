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

use anyhow::Result;
use std::path::Path;
use tokio_rusqlite::Connection;
use tracing::info;

use crate::models::{
    CollectionMeta, CollectionVisibility, Conversation, ConversationSave, Message,
};

#[derive(Clone)]
pub struct Database {
    conn: Connection,
}

impl Database {
    pub async fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).await?;

        conn.call(|db| {
            db.execute_batch(
                "CREATE TABLE IF NOT EXISTS conversations (
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
                );",
            )?;
            Ok(())
        })
        .await?;

        info!("SQLite initialised at {}", path.display());
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
