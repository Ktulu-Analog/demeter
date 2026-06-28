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

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Chat ──────────────────────────────────────────────────────────────────────

/// `content` peut être une string (texte simple) ou un tableau de parts (multimodal).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: MessageContent,
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<Message>,
    pub model: String,
    #[serde(default = "default_true")]
    pub stream: bool,
    pub space_id: Option<String>,
    pub collection_id: Option<String>, // nom de collection Qdrant (anciennement i64 Albert)
    #[serde(default)]
    pub mcp_servers: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TitleRequest {
    pub first_user: String,
    pub first_assistant: String,
    pub model: String,
}

// ── Conversations ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub space_id: Option<String>,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ConversationSave {
    pub id: String,
    pub title: String,
    pub space_id: Option<String>,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Spaces ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptItem {
    #[serde(default = "default_icon")]
    pub icon: String,
    pub label: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceItem {
    pub id: String,
    #[serde(default = "default_icon")]
    pub icon: String,
    pub label: String,
    #[serde(default)]
    pub dot: String,
    #[serde(default)]
    pub system: String,
    #[serde(default)]
    pub prompts: Vec<PromptItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rag_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SpacesPayload {
    pub spaces: Vec<SpaceItem>,
}

// ── RAG ───────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Chunk {
    pub text: String,
    pub source: String, // résolu depuis Document.name via API
    pub page: String,
    #[allow(dead_code)]
    pub document_id: i64, // chunk.document_id depuis Albert API
    #[allow(dead_code)]
    pub score: f64,
    pub rerank_score: Option<f64>,
    pub image_url: Option<String>, // URL S3/Garage pour les chunks graphiques
    pub content_type: Option<String>, // "chart_image" pour les graphiques, None pour le texte
}

// ── Ingestion ─────────────────────────────────────────────────────────────────

// ── Collection visibility ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionVisibility {
    Private,
    Public,
    Shared,
}

impl CollectionVisibility {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Private => "private",
            Self::Public => "public",
            Self::Shared => "shared",
        }
    }
    pub fn from_str(s: &str) -> Self {
        match s {
            "public" => Self::Public,
            "shared" => Self::Shared,
            _ => Self::Private,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionMeta {
    pub collection_name: String,
    pub owner_id: String, // email Albert du créateur
    pub visibility: CollectionVisibility,
    pub shared_with: Vec<String>, // emails (pour visibility = shared)
    pub created_at: String,
}

impl CollectionMeta {
    /// Retourne true si `user_email` peut accéder à cette collection.
    pub fn is_accessible_by(&self, user_email: &str) -> bool {
        match self.visibility {
            CollectionVisibility::Public => true,
            CollectionVisibility::Private => self.owner_id == user_email,
            CollectionVisibility::Shared => {
                self.owner_id == user_email || self.shared_with.iter().any(|e| e == user_email)
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct CollectionMetaUpdateRequest {
    pub visibility: CollectionVisibility,
    pub shared_with: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct TokenGenerateRequest {
    /// Niveau d'accès : "r" (lecture) ou "rw" (lecture-écriture)
    pub access: String,
    /// Durée de validité en secondes (défaut : 365 jours)
    pub ttl_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct CollectionCreateRequest {
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub description: String,
    #[serde(default = "default_private")]
    pub visibility: String,
}

#[derive(Debug, Deserialize)]
pub struct CollectionRenameRequest {
    pub name: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}
fn default_icon() -> String {
    "💬".to_string()
}
fn default_private() -> String {
    "private".to_string()
}
