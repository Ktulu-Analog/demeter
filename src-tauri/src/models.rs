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
    pub collection_id: Option<i64>,
    #[serde(default)]
    pub web_search: bool,
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
    pub source: String,   // résolu depuis Document.name via API
    pub page: String,
    pub document_id: i64, // chunk.document_id depuis Albert API
    #[allow(dead_code)]
    pub score: f64,
    pub rerank_score: Option<f64>,
}

// ── Ingestion ─────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CollectionCreateRequest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_private")]
    pub visibility: String,
}

#[derive(Debug, Deserialize)]
pub struct CollectionRenameRequest {
    pub name: String,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn default_true() -> bool { true }
fn default_icon() -> String { "💬".to_string() }
fn default_private() -> String { "private".to_string() }
