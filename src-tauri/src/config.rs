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

/// Configuration centrale de Demeter.
///
/// Toutes les valeurs sont lues depuis les variables d'environnement,
/// elles-mêmes chargées depuis le fichier `.env` au démarrage (via dotenvy).
/// Les valeurs par défaut sont définies ici et documentées dans `.env.example`.
use std::time::Duration;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn env_str(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn env_f64(key: &str, default: f64) -> f64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

// ── Serveur ───────────────────────────────────────────────────────────────────

/// Port d'écoute du serveur HTTP interne (défaut : 45678).
pub fn api_port() -> u16 {
    env_u16("API_PORT", 45678)
}

// ── RAG ───────────────────────────────────────────────────────────────────────

/// Modèle de reranking Albert (défaut : BAAI/bge-reranker-v2-m3).
pub fn rerank_model() -> String {
    env_str("RERANK_MODEL", "BAAI/bge-reranker-v2-m3")
}

/// Nombre de chunks récupérés lors de la recherche initiale (défaut : 20).
pub fn rag_top_k() -> usize {
    env_usize("RAG_TOP_K", 20)
}

/// Nombre de chunks conservés après reranking (défaut : 5).
pub fn rag_top_rerank() -> usize {
    env_usize("RAG_TOP_RERANK", 5)
}

/// Score de similarité minimum pour qu'un chunk soit retenu (défaut : 0.15).
pub fn rag_min_score() -> f64 {
    env_f64("RAG_MIN_SCORE", 0.15)
}

// ── LLM ───────────────────────────────────────────────────────────────────────

/// Nombre maximum de tokens pour les appels LLM principaux (défaut : 100000).
pub fn llm_max_tokens() -> u64 {
    env_u64("LLM_MAX_TOKENS", 100_000)
}

/// Nombre maximum de tokens pour la génération de titre (défaut : 20).
pub fn title_max_tokens() -> u64 {
    env_u64("TITLE_MAX_TOKENS", 20)
}

/// Nombre maximum de tours dans la boucle agentique MCP (défaut : 5).
pub fn mcp_max_turns() -> usize {
    env_usize("MCP_MAX_TURNS", 5)
}

/// Taille maximum en bytes du premier message utilisateur pour le titrage (défaut : 1200).
pub fn title_truncate_user() -> usize {
    env_usize("TITLE_TRUNCATE_USER", 1200)
}

/// Taille maximum en bytes de la première réponse assistant pour le titrage (défaut : 600).
pub fn title_truncate_assistant() -> usize {
    env_usize("TITLE_TRUNCATE_ASSISTANT", 600)
}

// ── Timeouts HTTP ─────────────────────────────────────────────────────────────

/// Timeout pour les appels LLM streamés (défaut : 120s).
pub fn timeout_llm_stream() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_LLM_STREAM_SECS", 120))
}

/// Timeout pour les appels LLM non-streamés / MCP (défaut : 120s).
pub fn timeout_llm_sync() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_LLM_SYNC_SECS", 120))
}

/// Timeout pour la génération de titre (défaut : 30s).
pub fn timeout_title() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_TITLE_SECS", 30))
}

/// Timeout pour les appels RAG / Albert search (défaut : 30s).
pub fn timeout_rag() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_RAG_SECS", 30))
}

/// Timeout pour le reranking (défaut : 30s).
pub fn timeout_rerank() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_RERANK_SECS", 30))
}

/// Timeout pour la résolution des noms de documents (défaut : 10s).
pub fn timeout_doc_resolve() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_DOC_RESOLVE_SECS", 10))
}

/// Timeout pour le proxy image (défaut : 15s).
pub fn timeout_image_proxy() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_IMAGE_PROXY_SECS", 15))
}

/// Timeout pour les appels de liste de modèles et autres appels courts (défaut : 8s).
pub fn timeout_models() -> Duration {
    Duration::from_secs(env_u64("TIMEOUT_MODELS_SECS", 8))
}
