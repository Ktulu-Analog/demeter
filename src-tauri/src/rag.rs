use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::config;
use crate::models::Chunk;

// ── URL helper ────────────────────────────────────────────────────────────────

pub fn albert_base(endpoint: &str) -> String {
    let mut base = endpoint.trim_end_matches('/').to_string();
    for suffix in &[
        "/chat/completions",
        "/rerank",
        "/embeddings",
        "/search",
    ] {
        if base.ends_with(suffix) {
            base = base[..base.len() - suffix.len()].to_string();
        }
    }
    base.trim_end_matches('/').to_string()
}

// ── Collection resolution ─────────────────────────────────────────────────────

pub async fn get_collection_id(
    space_id: &str,
    endpoint: &str,
    bearer: &str,
) -> Option<i64> {
    let base = albert_base(endpoint);
    let client = Client::builder()
        .timeout(config::timeout_doc_resolve())
        .build()
        .ok()?;

    let resp = client
        .get(format!("{}/collections", base))
        .query(&[("name", space_id), ("limit", "1")])
        .bearer_auth(bearer)
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        warn!("get_collection_id: HTTP {}", resp.status());
        return None;
    }

    let data: Value = resp.json().await.ok()?;
    let id = data["data"].as_array()?.first()?.get("id")?.as_i64()?;
    info!("Collection '{}' → id={}", space_id, id);
    Some(id)
}

// ── Document name resolution ──────────────────────────────────────────────────
// Albert API ne retourne pas le nom du document dans les chunks de recherche.
// Le nom vit dans Document.name, accessible via GET /v1/documents/{document_id}.
// On résout tous les document_id uniques en parallèle avec futures::join_all.

async fn resolve_document_names(
    document_ids: &[i64],
    base: &str,
    bearer: &str,
    client: &Client,
) -> HashMap<i64, String> {
    use futures::future::join_all;

    let futures: Vec<_> = document_ids
        .iter()
        .map(|&doc_id| {
            let url = format!("{}/documents/{}", base, doc_id);
            let bearer = bearer.to_string();
            let client = client.clone();
            async move {
                let resp = client
                    .get(&url)
                    .bearer_auth(&bearer)
                    .send()
                    .await;
                match resp {
                    Ok(r) if r.status().is_success() => {
                        match r.json::<Value>().await {
                            Ok(data) => {
                                let name = data["name"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                (doc_id, name)
                            }
                            Err(e) => {
                                warn!("resolve_document_names: parse error for doc {}: {}", doc_id, e);
                                (doc_id, String::new())
                            }
                        }
                    }
                    Ok(r) => {
                        warn!("resolve_document_names: HTTP {} for doc {}", r.status(), doc_id);
                        (doc_id, String::new())
                    }
                    Err(e) => {
                        warn!("resolve_document_names: request error for doc {}: {}", doc_id, e);
                        (doc_id, String::new())
                    }
                }
            }
        })
        .collect();

    join_all(futures).await.into_iter().collect()
}

// ── Search ────────────────────────────────────────────────────────────────────

pub async fn search_chunks(
    query: &str,
    collection_id: i64,
    endpoint: &str,
    bearer: &str,
) -> Vec<Chunk> {
    let base = albert_base(endpoint);
    let client = match Client::builder()
        .timeout(config::timeout_rag())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("search_chunks: client error: {}", e);
            return vec![];
        }
    };

    let body = serde_json::json!({
        "collection_ids": [collection_id],
        "query":          query,
        "method":         "hybrid",
        "limit":          config::rag_top_k(),
        "rff_k":          60,
    });

    let resp = match client
        .post(format!("{}/search", base))
        .bearer_auth(bearer)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!("Albert search failed: {}", e);
            return vec![];
        }
    };

    if !resp.status().is_success() {
        warn!("Albert search HTTP {}", resp.status());
        return vec![];
    }

    let data: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            warn!("Albert search parse error: {}", e);
            return vec![];
        }
    };

    // Première passe : extraire les chunks bruts avec leur document_id
    let mut chunks: Vec<Chunk> = Vec::new();
    if let Some(results) = data["data"].as_array() {
        for r in results {
            let chunk_obj = &r["chunk"];
            let text = chunk_obj["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            if text.is_empty() {
                continue;
            }

            let document_id = chunk_obj["document_id"].as_i64().unwrap_or(0);

            // Page : dans metadata si renseignée à l'ingestion
            let meta = &chunk_obj["metadata"];
            let page = meta
                .as_object()
                .and_then(|m| {
                    m.get("page")
                        .or_else(|| m.get("page_number"))
                        .or_else(|| m.get("page_num"))
                })
                .map(|v| {
                    if let Some(s) = v.as_str() {
                        s.to_string()
                    } else {
                        v.to_string()
                    }
                })
                .unwrap_or_default();

            let score = r["score"].as_f64().unwrap_or(0.0);

            chunks.push(Chunk {
                text,
                source: String::new(), // rempli après résolution
                page,
                document_id,
                score,
                rerank_score: None,
            });
        }
    }

    // Deuxième passe : résoudre les noms via GET /v1/documents/{id}
    let doc_ids: Vec<i64> = {
        let mut seen = std::collections::HashSet::new();
        chunks
            .iter()
            .filter_map(|c| {
                if c.document_id > 0 && seen.insert(c.document_id) {
                    Some(c.document_id)
                } else {
                    None
                }
            })
            .collect()
    };

    if !doc_ids.is_empty() {
        let name_map = resolve_document_names(&doc_ids, &base, bearer, &client).await;
        info!("Document names resolved: {:?}", name_map);
        for chunk in &mut chunks {
            if let Some(name) = name_map.get(&chunk.document_id) {
                chunk.source = name.clone();
            }
        }
    }

    info!(
        "Albert search: {} chunks (collection_id={})",
        chunks.len(),
        collection_id
    );
    chunks
}

// ── Rerank ────────────────────────────────────────────────────────────────────

pub async fn rerank_chunks(
    query: &str,
    chunks: Vec<Chunk>,
    endpoint: &str,
    bearer: &str,
) -> Vec<Chunk> {
    if chunks.is_empty() {
        return chunks;
    }

    let base = albert_base(endpoint);
    let client = match Client::builder()
        .timeout(config::timeout_rerank())
        .build()
    {
        Ok(c) => c,
        Err(_) => return chunks[..config::rag_top_rerank().min(chunks.len())].to_vec(),
    };

    let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
    let body = serde_json::json!({
        "model":     config::rerank_model(),
        "query":     query,
        "documents": texts,
        "top_n":     config::rag_top_rerank(),
    });

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
            return chunks[..config::rag_top_rerank().min(chunks.len())].to_vec();
        }
    };

    if !resp.status().is_success() {
        warn!("Rerank HTTP {}", resp.status());
        return chunks[..config::rag_top_rerank().min(chunks.len())].to_vec();
    }

    let data: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return chunks[..config::rag_top_rerank().min(chunks.len())].to_vec(),
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

    info!(
        "Rerank: {} → {} chunks (threshold={})",
        chunks.len(),
        reranked.len(),
        min_score
    );
    reranked
}

// ── Full RAG pipeline ─────────────────────────────────────────────────────────

/// Raccourci conservé pour compatibilité éventuelle.
#[allow(dead_code)]
pub async fn retrieve_context(
    query: &str,
    space_id: &str,
    endpoint: &str,
    bearer: &str,
) -> Option<String> {
    let (ctx, _) = retrieve_context_with_chunks(query, space_id, endpoint, bearer).await?;
    Some(ctx)
}

/// Retourne (contexte_texte, chunks_utilisés) pour permettre le feedback RAG côté client.
pub async fn retrieve_context_with_chunks(
    query: &str,
    space_id: &str,
    endpoint: &str,
    bearer: &str,
) -> Option<(String, Vec<Chunk>)> {
    let collection_id = get_collection_id(space_id, endpoint, bearer).await?;
    let chunks = search_chunks(query, collection_id, endpoint, bearer).await;
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
            format!("{}\n{}", meta, c.text)
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub fn build_system_with_rag(base_system: &str, context: Option<&str>) -> String {
    match context {
        None => base_system.to_string(),
        Some(ctx) => format!(
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
        ),
    }
}
