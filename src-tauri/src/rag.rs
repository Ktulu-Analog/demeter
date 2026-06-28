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

//! Pipeline RAG Qdrant.
//!
//! Architecture :
//!   - Embeddings  : Albert API  (`/embeddings`)
//!   - Stockage    : Qdrant local (`QDRANT_URL`, clé optionnelle `QDRANT_API_KEY` ou préférences)
//!   - Reranking   : Albert API  (`/rerank`)
//!
//! Toutes les fonctions Qdrant acceptent `qdrant_key: Option<&str>`.
//! Priorité : valeur des préférences utilisateur > variable d'env QDRANT_API_KEY.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

use crate::config;
use crate::models::Chunk;

// ── JWT structs pour Qdrant granular access ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantCollectionAccess {
    pub collection: String,
    pub access: String, // "r" ou "rw"
}

/// Droits d'accès à une collection individuelle dans le JWT.
/// Qdrant attend : { "r": true } ou { "r": true, "w": true }
#[derive(Serialize, Deserialize)]
struct QdrantCollectionRights {
    r: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    w: bool,
}

/// Payload du JWT pour accès granulaire par collection.
/// Format attendu par Qdrant :
/// { "exp": ..., "access": { "collections": { "col_name": { "r": true, "w": false } } } }
#[derive(Serialize, Deserialize)]
struct QdrantJwtClaims {
    exp: i64,
    access: QdrantAccessClaim,
}

#[derive(Serialize, Deserialize)]
struct QdrantAccessClaim {
    collections: std::collections::HashMap<String, QdrantCollectionRights>,
}

/// JWT read-only global (access = "r" string) — pour les opérations sans collection cible.
#[derive(Serialize, Deserialize)]
struct QdrantJwtClaimsReadOnly {
    exp: i64,
    access: String,
}

// ── Helpers client ────────────────────────────────────────────────────────────

fn albert_client() -> Client {
    Client::builder()
        .timeout(config::timeout_rag())
        .use_rustls_tls()
        .build()
        .expect("Albert client build failed")
}

fn qdrant_client() -> Client {
    Client::builder()
        .timeout(config::timeout_rag())
        .use_rustls_tls()
        .build()
        .expect("Qdrant client build failed")
}

/// Retire le suffixe de chemin spécifique à Albert pour obtenir l'URL de base,
/// puis normalise vers /v1 (requis par la nouvelle API Albert).
pub fn albert_base(endpoint: &str) -> String {
    let mut base = endpoint.trim_end_matches('/').to_string();
    // Strip les suffixes de sous-routes connus (avec ou sans préfixe /v1)
    for suffix in &[
        "/v1/chat/completions",
        "/v1/rerank",
        "/v1/embeddings",
        "/v1/search",
        "/chat/completions",
        "/rerank",
        "/embeddings",
        "/search",
    ] {
        if base.ends_with(suffix) {
            base = base[..base.len() - suffix.len()].to_string();
            break;
        }
    }
    let base = base.trim_end_matches('/').to_string();
    // S'assurer que /v1 est présent — la nouvelle API Albert l'exige sur tous les endpoints
    if base.ends_with("/v1") {
        base
    } else {
        format!("{}/v1", base)
    }
}

/// URL de base Qdrant : priorité à l'URL passée depuis les credentials (préférences),
/// puis QDRANT_URL env, puis défaut localhost.
fn qdrant_base_url(override_url: Option<&str>) -> String {
    override_url
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .unwrap_or_else(config::qdrant_url)
        .trim_end_matches('/')
        .to_string()
}

/// Retourne true si `key` est déjà un JWT (commence par "eyJ").
pub fn is_jwt(key: &str) -> bool {
    key.starts_with("eyJ")
}

/// Ajoute l'en-tête d'API key Qdrant selon le type de clé :
/// - JWT utilisateur (`eyJ...`) → passé tel quel à Qdrant.
/// - Clé admin brute + accessible_collections Some([...]) → JWT granulaire HS256.
/// - Clé admin brute + accessible_collections None → clé brute (opérations admin).
/// - Clé admin brute + accessible_collections Some([]) → JWT read-only global.
fn qdrant_auth(
    rb: reqwest::RequestBuilder,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
) -> reqwest::RequestBuilder {
    let resolved_key: Option<String> = admin_key
        .filter(|k| !k.is_empty())
        .map(|k| k.to_string())
        .or_else(config::qdrant_api_key);

    tracing::debug!(
        "[TRACE qdrant_auth] admin_key={} resolved_key_type={} accessible_collections={:?}",
        admin_key
            .map(|k| if k.starts_with("eyJ") { "JWT" } else { "raw" })
            .unwrap_or("none"),
        resolved_key
            .as_deref()
            .map(|k| if k.starts_with("eyJ") { "JWT" } else { "raw" })
            .unwrap_or("none"),
        accessible_collections.map(|c| c.iter().map(|x| x.collection.as_str()).collect::<Vec<_>>()),
    );

    match resolved_key {
        None => {
            tracing::warn!(
                "[TRACE qdrant_auth] ⚠ Aucune clé Qdrant — requête sans authentification"
            );
            rb
        }
        Some(ref key) if is_jwt(key) => {
            // JWT pré-signé (token utilisateur) → passer directement à Qdrant
            tracing::info!(
                "[TRACE qdrant_auth] JWT utilisateur → header api-key prefix={:?}",
                &key[..key.len().min(20)]
            );
            rb.header("api-key", key.clone())
        }
        Some(ref ak) => match accessible_collections {
            Some(cols) if !cols.is_empty() => match generate_qdrant_jwt(ak, cols) {
                Ok(token) => rb.header("api-key", token),
                Err(e) => {
                    warn!("JWT generation failed ({}), fallback to admin key", e);
                    rb.header("api-key", ak.clone())
                }
            },
            Some(_) => match generate_readonly_jwt(ak) {
                Ok(token) => rb.header("api-key", token),
                Err(_) => rb.header("api-key", ak.clone()),
            },
            None => rb.header("api-key", ak.clone()),
        },
    }
}

pub fn generate_qdrant_jwt(
    admin_key: &str,
    collections: &[QdrantCollectionAccess],
) -> Result<String, jsonwebtoken::errors::Error> {
    generate_qdrant_jwt_with_ttl(admin_key, collections, 365 * 24 * 3600) // 1 an par défaut
}

/// Génère un JWT HS256 Qdrant avec accès granulaire et durée personnalisée.
pub fn generate_qdrant_jwt_with_ttl(
    admin_key: &str,
    collections: &[QdrantCollectionAccess],
    ttl_seconds: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let exp = chrono::Utc::now().timestamp() + ttl_seconds;
    let cols_map: std::collections::HashMap<String, QdrantCollectionRights> = collections
        .iter()
        .map(|c| {
            let rights = QdrantCollectionRights {
                r: true,
                w: c.access == "rw",
            };
            (c.collection.clone(), rights)
        })
        .collect();

    let claims = QdrantJwtClaims {
        exp,
        access: QdrantAccessClaim {
            collections: cols_map,
        },
    };
    // Qdrant exige un header JWT sans champ "typ" : {"alg":"HS256"} uniquement.
    // Header::new() ajoute typ="JWT" par défaut → Qdrant retourne 401.
    let mut header = Header::new(Algorithm::HS256);
    header.typ = None;
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(admin_key.as_bytes()),
    )
}

/// JWT read-only global (utilisé quand la liste de collections accessibles est vide).
fn generate_readonly_jwt(admin_key: &str) -> Result<String, jsonwebtoken::errors::Error> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let exp = chrono::Utc::now().timestamp() + 3600;
    let claims = QdrantJwtClaimsReadOnly {
        exp,
        access: "r".to_string(),
    };
    let mut header = Header::new(Algorithm::HS256);
    header.typ = None;
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(admin_key.as_bytes()),
    )
}

/// JWT admin complet (utilisé pour les opérations de gestion : create/delete/list collections).
pub fn generate_admin_jwt(admin_key: &str) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};

    let exp = chrono::Utc::now().timestamp() + 300; // 5 min suffisent pour une op admin
    let claims = serde_json::json!({ "exp": exp }); // pas de claim "access" = manage
    let mut header = Header::new(Algorithm::HS256);
    header.typ = None;
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(admin_key.as_bytes()),
    )
    .map_err(|e| e.to_string())
}

// ── Embeddings via Albert ─────────────────────────────────────────────────────

/// Génère un vecteur d'embedding pour `text` via Albert.
pub async fn embed(text: &str, albert_endpoint: &str, bearer: &str) -> Option<Vec<f32>> {
    let base = albert_base(albert_endpoint);
    let client = albert_client();
    let model = config::embedding_model();

    let body = json!({
        "model": model,
        "input": [text],
    });

    let resp = client
        .post(format!("{}/embeddings", base))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        warn!("embed: HTTP {} from Albert", resp.status());
        return None;
    }

    let data: Value = resp.json().await.ok()?;
    let vec: Vec<f32> = data["data"][0]["embedding"]
        .as_array()?
        .iter()
        .filter_map(|v| v.as_f64().map(|f| f as f32))
        .collect();

    if vec.is_empty() {
        warn!("embed: empty vector returned");
        None
    } else {
        Some(vec)
    }
}

// ── Qdrant : gestion des collections ─────────────────────────────────────────

/// Vérifie si une collection Qdrant existe.
pub async fn collection_exists(
    name: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> bool {
    let client = qdrant_client();
    let url = format!("{}/collections/{}", qdrant_base_url(qdrant_url), name);
    let req = qdrant_auth(client.get(&url), admin_key, None);
    match req.send().await {
        Ok(r) => r.status().is_success(),
        Err(e) => {
            warn!("collection_exists: {}", e);
            false
        }
    }
}

/// Crée une collection Qdrant avec la bonne dimension vectorielle.
pub async fn create_collection(
    name: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<(), String> {
    let client = qdrant_client();
    let url = format!("{}/collections/{}", qdrant_base_url(qdrant_url), name);
    let dim = config::embedding_dim();

    let body = json!({
        "vectors": {
            "size": dim,
            "distance": "Cosine"
        }
    });

    let req = qdrant_auth(client.put(&url).json(&body), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;

    if resp.status().is_success() {
        info!("Qdrant collection '{}' créée (dim={})", name, dim);
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {} — {}", status, body))
    }
}

/// Supprime une collection Qdrant.
pub async fn delete_collection(
    name: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<(), String> {
    let client = qdrant_client();
    let url = format!("{}/collections/{}", qdrant_base_url(qdrant_url), name);
    let req = qdrant_auth(client.delete(&url), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        info!("Qdrant collection '{}' supprimée", name);
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {} — {}", status, body))
    }
}

/// Renomme une collection Qdrant (scroll + upsert dans la nouvelle + suppression de l'ancienne).
pub async fn rename_collection(
    old_name: &str,
    new_name: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<(), String> {
    let client = qdrant_client();
    let scroll_url = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base_url(qdrant_url),
        old_name
    );
    let body = json!({ "limit": 10000, "with_payload": true, "with_vector": true });
    let req = qdrant_auth(client.post(&scroll_url).json(&body), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("scroll HTTP {}", resp.status()));
    }
    let data: Value = resp.json().await.map_err(|e| e.to_string())?;
    let points = data["result"]["points"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    create_collection(new_name, admin_key, qdrant_url).await?;

    if !points.is_empty() {
        let upsert_url = format!(
            "{}/collections/{}/points",
            qdrant_base_url(qdrant_url),
            new_name
        );
        let body = json!({ "points": points });
        let req = qdrant_auth(client.put(&upsert_url).json(&body), admin_key, None);
        let resp = req.send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("upsert HTTP {}", resp.status()));
        }
    }

    delete_collection(old_name, admin_key, qdrant_url).await?;
    info!("Qdrant collection renommée '{}' → '{}'", old_name, new_name);
    Ok(())
}

/// Liste toutes les collections Qdrant.
pub async fn list_collections(
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<Vec<Value>, String> {
    let client = qdrant_client();
    let url = format!("{}/collections", qdrant_base_url(qdrant_url));
    let req = qdrant_auth(client.get(&url), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let data: Value = resp.json().await.map_err(|e| e.to_string())?;
    let cols: Vec<Value> = data["result"]["collections"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(cols)
}

/// Liste les documents (groupés par `doc_id`) dans une collection Qdrant.
pub async fn list_documents(
    collection_name: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<Vec<Value>, String> {
    let client = qdrant_client();
    let url = format!(
        "{}/collections/{}/points/scroll",
        qdrant_base_url(qdrant_url),
        collection_name
    );
    let body = json!({ "limit": 10000, "with_payload": true, "with_vector": false });
    let req = qdrant_auth(client.post(&url).json(&body), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let data: Value = resp.json().await.map_err(|e| e.to_string())?;
    let points = data["result"]["points"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut docs: std::collections::HashMap<String, (String, usize)> =
        std::collections::HashMap::new();
    for pt in &points {
        let payload = &pt["payload"];
        let doc_id = payload["doc_id"].as_str().unwrap_or("").to_string();
        let source = payload["source"].as_str().unwrap_or("").to_string();
        if doc_id.is_empty() {
            continue;
        }
        let entry = docs.entry(doc_id).or_insert((source, 0));
        entry.1 += 1;
    }

    let result: Vec<Value> = docs
        .into_iter()
        .map(|(id, (name, chunks))| {
            json!({ "id": id, "name": name, "filename": name, "chunks_count": chunks })
        })
        .collect();

    Ok(result)
}

/// Supprime tous les points d'un document (filtrage par payload `doc_id`).
pub async fn delete_document(
    collection_name: &str,
    doc_id: &str,
    admin_key: Option<&str>,
    qdrant_url: Option<&str>,
) -> Result<(), String> {
    let client = qdrant_client();
    let url = format!(
        "{}/collections/{}/points/delete",
        qdrant_base_url(qdrant_url),
        collection_name
    );
    let body = json!({
        "filter": {
            "must": [
                { "key": "doc_id", "match": { "value": doc_id } }
            ]
        }
    });
    let req = qdrant_auth(client.post(&url).json(&body), admin_key, None);
    let resp = req.send().await.map_err(|e| e.to_string())?;
    if resp.status().is_success() {
        info!("Qdrant: doc '{}' supprimé de '{}'", doc_id, collection_name);
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("HTTP {} — {}", status, body))
    }
}

// ── Qdrant : ingestion d'un document ─────────────────────────────────────────

fn chunk_text(text: &str, size: usize, overlap: usize) -> Vec<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let total = chars.len();
    if total == 0 {
        return vec![];
    }
    let step = if size > overlap { size - overlap } else { size };
    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut page = 0usize;

    while start < total {
        let end = (start + size).min(total);
        let s: String = chars[start..end].iter().collect();
        chunks.push((page, s));
        page += 1;
        if end == total {
            break;
        }
        start += step;
    }
    chunks
}

#[allow(clippy::too_many_arguments)]
pub async fn ingest_document(
    collection_name: &str,
    doc_id: &str,
    file_name: &str,
    content: &[u8],
    ext: &str,
    chunk_size: usize,
    chunk_overlap: usize,
    albert_endpoint: &str,
    bearer: &str,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
    qdrant_url: Option<&str>,
) -> Result<usize, String> {
    let text = crate::extract::extract_text(content, ext)
        .map_err(|e| format!("Extraction échouée : {}", e))?;

    let raw_chunks = chunk_text(&text, chunk_size, chunk_overlap);
    if raw_chunks.is_empty() {
        return Err("Aucun contenu extrait.".into());
    }
    info!(
        "ingest_document: '{}' → {} chunks (size={}, overlap={})",
        file_name,
        raw_chunks.len(),
        chunk_size,
        chunk_overlap
    );

    if !collection_exists(collection_name, admin_key, qdrant_url).await {
        create_collection(collection_name, admin_key, qdrant_url)
            .await
            .map_err(|e| format!("Création collection : {}", e))?;
    }

    let batch_size = 16usize;
    let qclient = qdrant_client();
    let upsert_url = format!(
        "{}/collections/{}/points",
        qdrant_base_url(qdrant_url),
        collection_name
    );
    let mut inserted = 0usize;

    for batch in raw_chunks.chunks(batch_size) {
        let mut points: Vec<Value> = Vec::with_capacity(batch.len());
        for (page, chunk_text_val) in batch {
            let vector = match embed(chunk_text_val, albert_endpoint, bearer).await {
                Some(v) => v,
                None => {
                    warn!("ingest_document: embedding failed for a chunk, skipping");
                    continue;
                }
            };
            let point_id = uuid::Uuid::new_v4().to_string();
            points.push(json!({
                "id": point_id,
                "vector": vector,
                "payload": {
                    "text":    chunk_text_val,
                    "source":  file_name,
                    "doc_id":  doc_id,
                    "page":    page.to_string(),
                }
            }));
        }

        if points.is_empty() {
            continue;
        }

        let body = json!({ "points": points });
        let req = qdrant_auth(
            qclient.put(&upsert_url).json(&body),
            admin_key,
            accessible_collections,
        );
        match req.send().await {
            Ok(r) if r.status().is_success() => {
                inserted += points.len();
            }
            Ok(r) => {
                warn!(
                    "ingest_document: upsert HTTP {} — {}",
                    r.status(),
                    r.text().await.unwrap_or_default()
                );
            }
            Err(e) => {
                warn!("ingest_document: upsert error — {}", e);
            }
        }
    }

    info!(
        "ingest_document: '{}' → {} chunks insérés dans '{}'",
        file_name, inserted, collection_name
    );
    Ok(inserted)
}

// ── Search ────────────────────────────────────────────────────────────────────

/// Exécute une recherche vectorielle Qdrant avec un filtre optionnel sur content_type.
async fn qdrant_vector_search(
    client: &reqwest::Client,
    url: &str,
    vector: &[f32],
    limit: usize,
    content_type_filter: Option<&str>,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
) -> Option<Value> {
    let mut body = json!({
        "vector":       vector,
        "limit":        limit,
        "with_payload": true,
        "with_vector":  false,
    });
    if let Some(ct) = content_type_filter {
        body["filter"] = json!({
            "must": [{"key": "content_type", "match": {"value": ct}}]
        });
    }
    let req = qdrant_auth(
        client.post(url).json(&body),
        admin_key,
        accessible_collections,
    );
    let resp = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            warn!("[qdrant_vector_search] réseau: {}", e);
            return None;
        }
    };
    if !resp.status().is_success() {
        warn!("[qdrant_vector_search] HTTP {}", resp.status());
        return None;
    }
    resp.json().await.ok()
}

/// Parse les résultats bruts Qdrant en Vec<Chunk>.
fn parse_qdrant_results(data: &Value) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    if let Some(results) = data["result"].as_array() {
        for r in results {
            let payload = &r["payload"];
            let text = payload["text"].as_str().unwrap_or("").trim().to_string();
            if text.is_empty() {
                continue;
            }
            let source = payload["source"].as_str().unwrap_or("").to_string();
            let page = payload["page"].as_str().unwrap_or("").to_string();
            let doc_id = payload["doc_id"]
                .as_str()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            let score = r["score"].as_f64().unwrap_or(0.0);
            let image_url = payload["image_storage_url"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            let content_type = payload["content_type"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            info!(
                "[search_chunks] chunk content_type={:?} image_url={:?}",
                content_type, image_url
            );
            chunks.push(Chunk {
                text,
                source,
                page,
                document_id: doc_id,
                score,
                rerank_score: None,
                image_url,
                content_type,
            });
        }
    }
    chunks
}

pub async fn search_chunks(
    query: &str,
    collection_name: &str,
    albert_endpoint: &str,
    bearer: &str,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
    qdrant_url: Option<&str>,
) -> Vec<Chunk> {
    let vector = match embed(query, albert_endpoint, bearer).await {
        Some(v) => v,
        None => {
            warn!("search_chunks: embed failed");
            return vec![];
        }
    };

    let client = qdrant_client();
    let url = format!(
        "{}/collections/{}/points/search",
        qdrant_base_url(qdrant_url),
        collection_name
    );
    tracing::info!(
        "[TRACE search_chunks] url={} admin_key={} accessible_collections={:?}",
        url,
        admin_key
            .map(|k| if k.starts_with("eyJ") { "JWT" } else { "raw" })
            .unwrap_or("none"),
        accessible_collections.map(|c| c.iter().map(|x| x.collection.as_str()).collect::<Vec<_>>()),
    );

    // ── Recherche 1 : chunks texte (top-K global) ────────────────────────────
    let data_text = match qdrant_vector_search(
        &client,
        &url,
        &vector,
        config::rag_top_k(),
        None,
        admin_key,
        accessible_collections,
    )
    .await
    {
        Some(d) => d,
        None => return vec![],
    };

    let nb_results = data_text["result"].as_array().map(|a| a.len()).unwrap_or(0);
    tracing::info!(
        "[TRACE search_chunks] Qdrant résultats bruts: {}",
        nb_results
    );
    let mut chunks = parse_qdrant_results(&data_text);

    // ── Recherche 2 : chunks chart_image dédiée ──────────────────────────────
    // Garantit que des images pertinentes remontent indépendamment du top-K texte.
    let image_top_k = config::rag_image_top_k();
    if image_top_k > 0 {
        if let Some(data_img) = qdrant_vector_search(
            &client,
            &url,
            &vector,
            image_top_k,
            Some("chart_image"),
            admin_key,
            accessible_collections,
        )
        .await
        {
            let img_chunks = parse_qdrant_results(&data_img);
            let nb_img = img_chunks.len();
            // Dédoublonner : ne garder que les images pas déjà dans chunks texte
            let existing_texts: std::collections::HashSet<String> =
                chunks.iter().map(|c| c.text.clone()).collect();
            let new_images: Vec<Chunk> = img_chunks
                .into_iter()
                .filter(|c| !existing_texts.contains(&c.text))
                .collect();
            info!(
                "[search_chunks] image search dédiée: {} résultats, {} nouveaux",
                nb_img,
                new_images.len()
            );
            chunks.extend(new_images);
        }
    }

    info!(
        "Qdrant search: {} chunks (collection='{}')",
        chunks.len(),
        collection_name
    );
    chunks
}

// ── Rerank ────────────────────────────────────────────────────────────────────

pub async fn rerank_chunks(
    query: &str,
    chunks: Vec<Chunk>,
    albert_endpoint: &str,
    bearer: &str,
) -> Vec<Chunk> {
    if chunks.is_empty() {
        return chunks;
    }

    let base = albert_base(albert_endpoint);
    let client = match Client::builder().timeout(config::timeout_rerank()).build() {
        Ok(c) => c,
        Err(_) => return chunks[..config::rag_top_rerank().min(chunks.len())].to_vec(),
    };

    // Les chunks graphiques ne sont pas soumis au reranker (texte VLM peu pertinent
    // pour la similarité avec la question). On les conserve à part et on les
    // réinjecte après, limités à 3 pour ne pas surcharger le contexte.
    let (image_chunks, text_chunks): (Vec<Chunk>, Vec<Chunk>) = chunks
        .into_iter()
        .partition(|c| c.content_type.as_deref() == Some("chart_image"));
    let chunks = text_chunks;

    if chunks.is_empty() {
        let kept: Vec<Chunk> = image_chunks.into_iter().take(3).collect();
        info!(
            "Rerank: 0 text chunks, {} image chunks conservés",
            kept.len()
        );
        return kept;
    }

    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let body = json!({
        "model":     config::rerank_model(),
        "query":     query,
        "documents": texts,
        "top_n":     config::rag_top_rerank(),
    });

    // Helper : fallback avec réinjection des images
    macro_rules! fallback_with_images {
        ($text_chunks:expr, $image_chunks:expr) => {{
            let top = config::rag_top_rerank();
            let mut result: Vec<Chunk> = $text_chunks[..top.min($text_chunks.len())].to_vec();
            let img_count = $image_chunks.len().min(3);
            result.extend($image_chunks.into_iter().take(3));
            info!(
                "Rerank fallback: {} text + {} image(s)",
                result.len() - img_count,
                img_count
            );
            return result;
        }};
    }

    let resp = match client
        .post(format!("{}/rerank", base))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Reranking failed ({}), fallback", e);
            fallback_with_images!(chunks, image_chunks);
        }
    };

    if !resp.status().is_success() {
        warn!("Rerank HTTP {}", resp.status());
        fallback_with_images!(chunks, image_chunks);
    }

    let data: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => {
            fallback_with_images!(chunks, image_chunks);
        }
    };

    let min_score = config::rag_min_score();
    let mut reranked: Vec<Chunk> = Vec::new();

    if let Some(results) = data["results"].as_array() {
        for r in results {
            let score = r["relevance_score"].as_f64().unwrap_or(1.0);
            if score >= min_score {
                let idx = r["index"].as_u64().unwrap_or(0) as usize;
                if idx < chunks.len() {
                    let mut c = chunks[idx].clone();
                    c.rerank_score = Some(score);
                    reranked.push(c);
                }
            }
        }
    }

    reranked.sort_by(|a, b| {
        b.rerank_score
            .unwrap_or(0.0)
            .partial_cmp(&a.rerank_score.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Réinjecter les chunks graphiques (max 3) à la fin du contexte
    let image_count = image_chunks.len().min(3);
    reranked.extend(image_chunks.into_iter().take(3));

    info!(
        "Rerank: {} → {} chunks (threshold={}) + {} image(s)",
        chunks.len(),
        reranked.len() - image_count,
        min_score,
        image_count,
    );
    reranked
}

// ── Full RAG pipeline ─────────────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn retrieve_context(
    query: &str,
    space_id: &str,
    endpoint: &str,
    bearer: &str,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
    qdrant_url: Option<&str>,
) -> Option<String> {
    let (ctx, _) = retrieve_context_with_chunks(
        query,
        space_id,
        endpoint,
        bearer,
        admin_key,
        accessible_collections,
        qdrant_url,
    )
    .await?;
    Some(ctx)
}

pub async fn retrieve_context_with_chunks(
    query: &str,
    space_id: &str,
    endpoint: &str,
    bearer: &str,
    admin_key: Option<&str>,
    accessible_collections: Option<&[QdrantCollectionAccess]>,
    qdrant_url: Option<&str>,
) -> Option<(String, Vec<Chunk>)> {
    let chunks = search_chunks(
        query,
        space_id,
        endpoint,
        bearer,
        admin_key,
        accessible_collections,
        qdrant_url,
    )
    .await;
    if chunks.is_empty() {
        return None;
    }
    let chunks = rerank_chunks(query, chunks, endpoint, bearer).await;
    if chunks.is_empty() {
        return None;
    }
    let ctx = chunks_to_context(&chunks);
    Some((ctx, chunks))
}

pub fn chunks_to_context(chunks: &[Chunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut meta = format!("[{}]", i + 1);
            if !c.source.is_empty() {
                meta.push(' ');
                meta.push_str(&c.source);
                if !c.page.is_empty() {
                    meta.push_str(&format!(", p.{}", c.page));
                }
            }
            info!(
                "[chunks_to_context] chunk[{}] content_type={:?} image_url={:?}",
                i, c.content_type, c.image_url
            );
            let image_part = if let Some(ref url) = c.image_url {
                // Extraire le champ TITRE de la description VLM comme texte alternatif.
                // Un alt contenant des newlines ou crochets casse le rendu Markdown inline.
                let alt: String = c
                    .text
                    .lines()
                    .find(|l| l.trim_start().starts_with("TITRE"))
                    .and_then(|l| l.split_once(':').map(|x| x.1))
                    .unwrap_or(&c.text)
                    .trim()
                    .chars()
                    .take(80)
                    .filter(|ch| *ch != '[' && *ch != ']' && *ch != '\n' && *ch != '\r')
                    .collect();
                format!("\n![{}]({})", alt, url)
            } else {
                String::new()
            };
            format!("{}\n{}{}", meta, c.text, image_part)
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub fn build_system_with_rag(base_system: &str, context: Option<&str>) -> String {
    match context {
        None => base_system.to_string(),
        Some(ctx) => {
            // Log des URLs d'images présentes dans le contexte pour debug
            let image_count = ctx.matches("![").count();
            tracing::info!(
                "[build_system_with_rag] contexte: {} images détectées",
                image_count
            );
            let mut idx_img = 0usize;
            for (i, line) in ctx.lines().enumerate() {
                if line.contains("![") {
                    tracing::info!(
                        "[build_system_with_rag] image[{}] ligne {}: {}",
                        idx_img,
                        i,
                        &line[..line.len().min(120)]
                    );
                    idx_img += 1;
                }
            }
            format!(
            "{}\n\n{}\nCONTEXTE DOCUMENTAIRE (extraits de la base de connaissances)\n{}\n{}\n\n\
             INSTRUCTIONS :\n\
             - Appuie-toi prioritairement sur les extraits ci-dessus.\n\
             - Cite les sources entre crochets (ex. [1], [2]) quand tu les utilises.\n\
             - Si le contexte ne suffit pas, complete avec tes connaissances generales en le signalant.\n\
             - Ne fabrique jamais de references ou citations inexistantes.",
            base_system,
            "══════════════════════════════════════════════",
            ctx,
            "══════════════════════════════════════════════"
            )
        }
    }
}
