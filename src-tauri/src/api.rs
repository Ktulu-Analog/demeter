use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

use crate::{
    db::Database,
    extract::extract_text,
    models::*,
    rag::{
        build_system_with_rag, chunks_to_context, rerank_chunks,
        retrieve_context, retrieve_context_with_chunks, search_chunks, albert_base,
    },
    spaces::{load_spaces, save_spaces},
    web_search::fetch_web_search,
};

// ── System prompt constants ───────────────────────────────────────────────────

const LATEX_INSTRUCTION: &str = r#"

──────────────────────────────────────────
RENDU LATEX
Pour toute formule ou expression mathématique, utilise EXCLUSIVEMENT la notation KaTeX :
- Inline : $formule$  →  ex: le taux est $r = \frac{p}{q} \times 100$
- Bloc centré : $$formule$$  →  ex: $$\text{Turn-over} = \frac{D}{EM} \times 100$$
INTERDIT : ne jamais utiliser [ formule ] ou \[ formule \] ou \( formule \).
SEULS les délimiteurs $ et $$ sont reconnus par le moteur de rendu.
──────────────────────────────────────────
"#;

const ECHARTS_INSTRUCTION: &str = r#"

──────────────────────────────────────────
RENDU DE GRAPHIQUES
Quand une réponse bénéficierait d'une visualisation, insère un bloc "echarts" contenant un objet JSON valide.
Règles STRICTES : JSON RFC 8259 pur, toutes les clés entre guillemets doubles, aucun commentaire (ni // ni /* */), pas d'apostrophes comme délimiteurs de chaîne, pas de virgule après le dernier élément, pas de fonctions JS. Les chaînes de caractères utilisent exclusivement les guillemets doubles.
Exemple minimal valide :
```echarts
{"title":{"text":"Exemple"},"xAxis":{"data":["A","B"]},"yAxis":{},"series":[{"type":"bar","data":[1,2]}]}
```
──────────────────────────────────────────
"#;

const MERMAID_INSTRUCTION: &str = r#"

──────────────────────────────────────────
DIAGRAMMES MERMAID
Quand une réponse bénéficierait d'un diagramme, insère un bloc mermaid après l'explication textuelle.
Règles : IDs de nœuds en ASCII sans accent, pas de mots réservés comme IDs.
──────────────────────────────────────────
"#;

const WORD_INSTRUCTION: &str = r#"

──────────────────────────────────────────
GENERATION DE DOCUMENTS WORD
Quand l'utilisateur demande un document Word, un rapport, un modele de lettre, un contrat
ou tout autre document a telecharger, genere le contenu dans UN SEUL bloc ```word.

STRUCTURE OBLIGATOIRE — respecter exactement cette syntaxe :

```word Titre du fichier
# Titre principal du document

## Section avec texte
Paragraphe de contenu...

## Section avec graphique
Texte introductif.
```echarts
{"title":{"text":"Mon graphique"},"xAxis":{"type":"category","data":["A","B","C"]},"yAxis":{"type":"value"},"series":[{"type":"bar","data":[10,20,30]}]}
```
Suite du texte après le graphique.

## Section avec tableau
| Col A | Col B |
|-------|-------|
| v1    | v2    |
```

REGLES ABSOLUES :
1. UN SEUL bloc ```word par reponse. Jamais deux.
2. Chaque graphique = un bloc ```echarts sur UNE SEULE LIGNE de JSON, imbriqué DANS le ```word.
3. INTERDICTION ABSOLUE d'ecrire le JSON d'un graphique comme texte ordinaire — ca doit toujours etre dans ```echarts ... ```.
4. INTERDICTION d'ecrire [voir graphique ci-dessous] ou tout texte de substitution.
5. Fermer le bloc ```word avec trois backticks seuls sur une ligne.
──────────────────────────────────────────
"#;

const IMAGE_INSTRUCTION: &str = r#"

──────────────────────────────────────────
AFFICHAGE D'IMAGES
Quand l'utilisateur demande une image ou quand le contexte de recherche web contient des URLs d'images :
- OBLIGATOIRE : insere l'image directement dans ta reponse avec la syntaxe markdown : ![description courte](url)
- INTERDIT : ne jamais te contenter de donner un lien texte ou une liste de sources sans afficher l'image
- Si tu as plusieurs images disponibles, choisis la plus pertinente et affiche-la en premier
- Apres l'image, tu peux ajouter la source et les metadonnees (auteur, licence) en texte
- Pour Wikimedia Commons, utilise l'URL de la page (ex: https://commons.wikimedia.org/wiki/File:X.jpg)
──────────────────────────────────────────
"#;

fn default_system() -> String {
    format!(
        "Tu es Demeter, un assistant RH expert et bienveillant.\nReponds en francais, de facon claire, structuree et professionnelle.{}{}{}{}{}",
        LATEX_INSTRUCTION, ECHARTS_INSTRUCTION, MERMAID_INSTRUCTION, WORD_INSTRUCTION, IMAGE_INSTRUCTION
    )
}

fn append_instructions(system: &str) -> String {
    format!("{}{}{}{}{}{}",
        system, LATEX_INSTRUCTION, ECHARTS_INSTRUCTION, MERMAID_INSTRUCTION, WORD_INSTRUCTION, IMAGE_INSTRUCTION)
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub prompts_path: PathBuf,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn build_router(db: Database, prompts_path: PathBuf) -> Router {
    let state = Arc::new(AppState { db, prompts_path });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Spaces
        .route("/api/spaces", get(get_spaces))
        .route("/api/spaces", put(put_spaces))
        // Chat
        .route("/api/chat", post(chat))
        .route("/api/generate-title", post(generate_title))
        // RAG status
        .route("/api/rag/status", get(rag_status))
        // Conversations
        .route("/api/conversations", get(list_conversations))
        .route("/api/conversations", post(save_conversation))
        .route("/api/conversations/:id", delete(delete_conversation))
        // User
        .route("/api/users/me", get(get_user_me))
        // Extract
        .route("/api/extract", post(extract_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/extract-multiple", post(extract_multiple_files).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        // Ingestion
        .route("/api/ingestion/collections", get(list_collections))
        .route("/api/ingestion/collections", post(create_collection))
        .route("/api/ingestion/collections/:id", patch(rename_collection))
        .route("/api/ingestion/collections/:id", delete(delete_collection_handler))
        .route("/api/ingestion/collections/:id/documents", get(list_documents))
        .route("/api/ingestion/documents/:id", delete(delete_document))
        .route("/api/ingestion/upload", post(upload_document).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        // MCP
        .route("/api/mcp/tools", post(mcp_list_tools))
        // Health
        .route("/api/image-proxy", get(image_proxy))
        .route("/health", get(health))
        // Proxy for frontend (api-proxy prefix)
        .nest("/api-proxy", build_proxy_router(state.clone()))
        .with_state(state)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(cors)
}

fn build_proxy_router(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    // The frontend uses /api-proxy/api/... so we re-expose the same routes under that prefix
    Router::new()
        .route("/api/spaces", get(get_spaces))
        .route("/api/spaces", put(put_spaces))
        .route("/api/chat", post(chat))
        .route("/api/generate-title", post(generate_title))
        .route("/api/rag/status", get(rag_status))
        .route("/api/conversations", get(list_conversations))
        .route("/api/conversations", post(save_conversation))
        .route("/api/conversations/:id", delete(delete_conversation))
        .route("/api/users/me", get(get_user_me))
        .route("/api/models", get(list_models))
        .route("/api/extract", post(extract_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/extract-multiple", post(extract_multiple_files).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/ingestion/collections", get(list_collections))
        .route("/api/ingestion/collections", post(create_collection))
        .route("/api/ingestion/collections/:id", patch(rename_collection))
        .route("/api/ingestion/collections/:id", delete(delete_collection_handler))
        .route("/api/ingestion/collections/:id/documents", get(list_documents))
        .route("/api/ingestion/documents/:id", delete(delete_document))
        .route("/api/ingestion/upload", post(upload_document).layer(DefaultBodyLimit::max(50 * 1024 * 1024)))
        .route("/api/mcp/tools", post(mcp_list_tools))
        .route("/api/image-proxy", get(image_proxy))
        .route("/health", get(health))
}

// ── Image proxy ───────────────────────────────────────────────────────────────
// Fetche une image distante côté serveur pour contourner les blocages CORS/Referer.
// Le frontend appelle /api-proxy/api/image-proxy?url=<encoded_url>

#[derive(serde::Deserialize)]
struct ImageProxyParams {
    url: String,
}

async fn image_proxy(Query(params): Query<ImageProxyParams>) -> Response {
    let target_url = params.url.trim().to_string();

    // Sécurité minimale : n'accepter que http(s)
    if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
        return err(StatusCode::BAD_REQUEST, "URL invalide");
    }

    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36")
        .build()
    {
        Ok(c) => c,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    let resp = match client
        .get(&target_url)
        .header("Referer", "https://www.google.com/")
        .header("Accept", "image/avif,image/webp,image/apng,image/*,*/*;q=0.8")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("Fetch failed: {}", e)),
    };

    if !resp.status().is_success() {
        return err(
            StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
            format!("Upstream returned {}", resp.status()),
        );
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();

    // Refuser les réponses non-image pour éviter les abus
    if !content_type.starts_with("image/") {
        return err(StatusCode::BAD_GATEWAY, format!("Upstream content-type non-image: {}", content_type));
    }

    let body_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => return err(StatusCode::BAD_GATEWAY, format!("Read failed: {}", e)),
    };

    use axum::http::header;
    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header("Cache-Control", "public, max-age=86400")
        .header("Access-Control-Allow-Origin", "*")
        .body(axum::body::Body::from(body_bytes))
        .unwrap()
        .into_response()
}

// ── Error helper ──────────────────────────────────────────────────────────────

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(json!({"detail": msg.into()}))).into_response()
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "Demeter API", "rag_backend": "albert-api"}))
}

// ── Spaces ────────────────────────────────────────────────────────────────────

async fn get_spaces(State(state): State<Arc<AppState>>) -> Response {
    match load_spaces(&state.prompts_path) {
        Ok(mut spaces) => {
            for s in &mut spaces {
                s.rag_enabled = Some(true);
            }
            Json(json!({"spaces": spaces})).into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn put_spaces(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SpacesPayload>,
) -> Response {
    let count = payload.spaces.len();
    match save_spaces(&state.prompts_path, payload.spaces) {
        Ok(()) => Json(json!({"ok": true, "count": count})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── RAG status ────────────────────────────────────────────────────────────────

async fn rag_status() -> Json<Value> {
    Json(json!({
        "backend":        "albert-api",
        "rerank_model":   std::env::var("RERANK_MODEL").unwrap_or_else(|_| "bge-reranker-v2-m3".to_string()),
        "rag_top_k":      std::env::var("RAG_TOP_K").unwrap_or_else(|_| "20".to_string()),
        "rag_top_rerank": std::env::var("RAG_TOP_RERANK").unwrap_or_else(|_| "5".to_string()),
        "rag_min_score":  std::env::var("RAG_MIN_SCORE").unwrap_or_else(|_| "0.15".to_string()),
        "info": "Les collections sont gérées directement par Albert.",
    }))
}

// ── Chat ──────────────────────────────────────────────────────────────────────

async fn chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> Response {
    // Resolve system prompt
    let mut system_prompt = default_system();
    if let Some(ref space_id) = req.space_id {
        if let Ok(spaces) = load_spaces(&state.prompts_path) {
            if let Some(space) = spaces.iter().find(|s| &s.id == space_id) {
                system_prompt = append_instructions(&space.system);
            }
        }
    }

    // Last user message (texte uniquement pour RAG/web search)
    let last_user: Option<String> = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| match &m.content {
            crate::models::MessageContent::Text(s) => s.clone(),
            crate::models::MessageContent::Parts(parts) => {
                // Extraire les parts "text" pour RAG/web search
                parts.iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        });

    // RAG context
    let mut rag_chunks_used: Vec<crate::models::Chunk> = Vec::new();
    let rag_context = if let Some(ref query) = last_user {
        if let Some(cid) = req.collection_id {
            let chunks = search_chunks(query, cid, &req.endpoint, &req.bearer).await;
            let chunks = rerank_chunks(query, chunks, &req.endpoint, &req.bearer).await;
            if !chunks.is_empty() {
                let ctx = chunks_to_context(&chunks);
                rag_chunks_used = chunks;
                Some(ctx)
            } else {
                None
            }
        } else if let Some(ref sid) = req.space_id {
            match retrieve_context_with_chunks(query, sid, &req.endpoint, &req.bearer).await {
                Some((ctx, chunks)) => {
                    rag_chunks_used = chunks;
                    Some(ctx)
                }
                None => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut final_system =
        build_system_with_rag(&system_prompt, rag_context.as_deref());

    // Web search
    if req.web_search {
        if let Some(ref query) = last_user {
            info!("Web search triggered for: {:?}", query);
            let web_ctx = fetch_web_search(query, 6, &req.tavily_key).await;
            final_system.push_str("\n\n");
            final_system.push_str(&web_ctx);
        }
    }

    // Build LLM endpoint
    let mut endpoint = req.endpoint.trim_end_matches('/').to_string();
    if !endpoint.ends_with("/chat/completions") {
        endpoint.push_str("/chat/completions");
    }

    let mut messages_payload: Vec<Value> = vec![json!({"role": "system", "content": final_system})];
    for m in &req.messages {
        messages_payload.push(json!({"role": m.role, "content": m.content}));
    }

    let client = match Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
    {
        Ok(c) => c,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    // ── Boucle MCP (tool calling) ─────────────────────────────────────────────
    // Si des serveurs MCP sont configurés ET que ce n'est pas un appel streaming,
    // on fait une boucle synchrone : LLM → tool_calls → résultats → LLM → ...
    // Le streaming est réservé à la réponse finale (sans tool_calls).
    let mcp_tools: Vec<Value> = if !req.mcp_servers.is_empty() {
        let all = crate::mcp::collect_all_tools(&client, &req.mcp_servers).await;
        all.iter()
            .map(|(server, tool)| crate::mcp::mcp_tool_to_openai(tool, server))
            .collect()
    } else {
        vec![]
    };

    // Si on a des tools MCP, on fait d'abord un appel non-streamé pour gérer les tool_calls
    if !mcp_tools.is_empty() {
        let mut loop_messages = messages_payload.clone();
        const MAX_TURNS: usize = 5;

        for turn in 0..MAX_TURNS {
            let loop_payload = json!({
                "model":      req.model,
                "messages":   loop_messages,
                "stream":     false,
                "max_tokens": 100000,
                "tools":      mcp_tools,
                "tool_choice": "auto",
            });

            let resp = match client.post(&endpoint)
                .bearer_auth(&req.bearer)
                .json(&loop_payload)
                .send().await
            {
                Ok(r) => r,
                Err(e) => return err(StatusCode::BAD_GATEWAY, e.to_string()),
            };

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return err(StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY), body);
            }

            let data: Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => return err(StatusCode::BAD_GATEWAY, e.to_string()),
            };

            let choice = &data["choices"][0];
            let finish_reason = choice["finish_reason"].as_str().unwrap_or("");
            let msg = &choice["message"];

            // Pas de tool_calls → réponse finale
            // On émet le texte chunk par chunk via un vrai stream SSE (pas en un seul bloc)
            if finish_reason != "tool_calls" || msg["tool_calls"].as_array().map(|a| a.is_empty()).unwrap_or(true) {
                info!("MCP: réponse finale après {} tour(s)", turn + 1);
                let content = msg["content"].as_str().unwrap_or("").to_string();

                // Construire la liste ordonnée de chunks SSE à émettre
                let mut sse_chunks: Vec<Bytes> = Vec::new();

                // Injecter les sources RAG en premier si disponibles
                if !rag_chunks_used.is_empty() {
                    let sources: Vec<serde_json::Value> = rag_chunks_used.iter().enumerate().map(|(i, c)| {
                        json!({
                            "index":        i + 1,
                            "source":       c.source,
                            "page":         c.page,
                            "rerank_score": c.rerank_score,
                        })
                    }).collect();
                    let ev = format!(
                        "event: rag_sources\ndata: {}\n\n",
                        serde_json::to_string(&json!({ "sources": sources })).unwrap_or_default()
                    );
                    sse_chunks.push(Bytes::from(ev));
                }

                // Découper le contenu en petits morceaux de ~20 chars pour un rendu fluide
                for chunk in content.chars().collect::<Vec<_>>().chunks(20) {
                    let piece: String = chunk.iter().collect();
                    let delta = json!({
                        "choices": [{"delta": {"content": piece}, "finish_reason": null}]
                    });
                    sse_chunks.push(Bytes::from(format!("data: {}\n\n", delta)));
                }

                // Usage si présent
                if let Some(usage) = data.get("usage") {
                    let usage_event = json!({"choices": [{"delta": {}, "finish_reason": "stop"}], "usage": usage});
                    sse_chunks.push(Bytes::from(format!("data: {}\n\n", usage_event)));
                }
                sse_chunks.push(Bytes::from("data: [DONE]\n\n"));

                // Émettre les chunks via un stream async avec micro-délai entre chaque
                // pour que le frontend reçoive et affiche les tokens progressivement.
                let stream = futures::stream::iter(sse_chunks.into_iter().enumerate())
                    .then(|(i, chunk)| async move {
                        if i > 0 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(8)).await;
                        }
                        Ok::<Bytes, std::io::Error>(chunk)
                    });

                use axum::http::header;
                return axum::response::Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, "text/event-stream")
                    .header("Cache-Control", "no-cache")
                    .header("X-Accel-Buffering", "no")
                    .body(axum::body::Body::from_stream(stream))
                    .unwrap()
                    .into_response();
            }

            // Ajouter le message assistant avec ses tool_calls dans la conversation
            loop_messages.push(msg.clone());

            // Exécuter chaque tool_call
            if let Some(tool_calls) = msg["tool_calls"].as_array() {
                for tc in tool_calls {
                    let tc_id   = tc["id"].as_str().unwrap_or("").to_string();
                    let fn_name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let fn_args: Value = serde_json::from_str(
                        tc["function"]["arguments"].as_str().unwrap_or("{}")
                    ).unwrap_or(json!({}));

                    // Retrouver le serveur MCP à partir du préfixe du nom du tool
                    // Format : mcp__{slug}__{tool_name}
                    let parts: Vec<&str> = fn_name.splitn(3, "__").collect();
                    let real_tool_name = if parts.len() == 3 { parts[2] } else { &fn_name };
                    let server = req.mcp_servers.iter()
                        .find(|s| fn_name.starts_with(&format!("mcp__{}", crate::mcp::server_slug(s))))
                        .cloned()
                        .unwrap_or_else(|| req.mcp_servers[0].clone());

                    info!("MCP: appel tool {} sur {}", real_tool_name, server);
                    let result = crate::mcp::call_tool(&client, &server, real_tool_name, &fn_args).await;

                    loop_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc_id,
                        "content": result,
                    }));
                }
            }
        }

        // Max turns atteint — renvoyer un message d'erreur
        return (StatusCode::OK, axum::Json(json!({
            "choices": [{ "message": { "role": "assistant", "content": "⚠️ Nombre maximum de tours MCP atteint." }, "finish_reason": "stop" }]
        }))).into_response();
    }

    // ── Sans MCP : chemin normal ──────────────────────────────────────────────
    let payload = json!({
        "model":      req.model,
        "messages":   messages_payload,
        "stream":     req.stream,
        "max_tokens": 100000,
    });

    if req.stream {
        // Streaming SSE passthrough
        let resp = match client
            .post(&endpoint)
            .bearer_auth(&req.bearer)
            .json(&payload)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => return err(StatusCode::BAD_GATEWAY, e.to_string()),
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return err(
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            );
        }

        // Préparer l'event rag_sources à injecter en tête du stream
        let rag_prefix: bytes::Bytes = if !rag_chunks_used.is_empty() {
            let sources: Vec<serde_json::Value> = rag_chunks_used.iter().enumerate().map(|(i, c)| {
                json!({
                    "index":        i + 1,
                    "source":       c.source,
                    "page":         c.page,
                    "rerank_score": c.rerank_score,
                })
            }).collect();
            let event = format!(
                "event: rag_sources\ndata: {}\n\n",
                serde_json::to_string(&json!({ "sources": sources })).unwrap_or_default()
            );
            bytes::Bytes::from(event)
        } else {
            bytes::Bytes::new()
        };

        let llm_stream = resp.bytes_stream().map(|result| {
            result.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
        });

        use axum::body::Body;
        use axum::http::header;
        use futures::stream;

        let prefix_stream = if !rag_prefix.is_empty() {
            stream::once(async move { Ok::<bytes::Bytes, std::io::Error>(rag_prefix) })
                .left_stream()
        } else {
            stream::empty().right_stream()
        };
        let combined = prefix_stream.chain(llm_stream);

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header("Cache-Control", "no-cache")
            .header("X-Accel-Buffering", "no")
            .body(Body::from_stream(combined))
            .unwrap()
            .into_response()
    } else {
        // Non-streaming
        match client
            .post(&endpoint)
            .bearer_auth(&req.bearer)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let body: Value = resp.json().await.unwrap_or(json!({}));
                (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                    .into_response()
            }
            Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
        }
    }
}

// ── MCP ───────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct McpListRequest {
    servers: Vec<String>,
}

async fn mcp_list_tools(Json(req): Json<McpListRequest>) -> Response {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    let mut result: Vec<Value> = Vec::new();
    for server in &req.servers {
        let tools = crate::mcp::list_tools(&client, server).await;
        result.push(json!({
            "server": server,
            "status": if tools.is_empty() { "error" } else { "ok" },
            "tools": tools,
            "tool_count": tools.len(),
        }));
    }
    Json(json!({ "servers": result })).into_response()
}

// ── Generate title ────────────────────────────────────────────────────────────

async fn generate_title(Json(req): Json<TitleRequest>) -> Response {
    let mut endpoint = req.endpoint.trim_end_matches('/').to_string();
    if !endpoint.ends_with("/chat/completions") {
        endpoint.push_str("/chat/completions");
    }

    let payload = json!({
        "model":  req.model,
        "stream": false,
        "max_tokens": 20,
        "messages": [
            {
                "role": "system",
                "content": "Tu génères des titres de conversation ultra-courts (4 mots max, sans ponctuation finale, sans guillemets). Réponds uniquement avec le titre, rien d'autre.",
            },
            {
                "role": "user",
                "content": format!(
                    "Question : {}\nRéponse : {}\n\nGénère un titre court.",
                    &req.first_user[..req.first_user.len().min(400)],
                    &req.first_assistant[..req.first_assistant.len().min(400)],
                ),
            },
        ],
    });

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .post(&endpoint)
        .bearer_auth(&req.bearer)
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let data: Value = resp.json().await.unwrap_or(json!({}));
            let title = data["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .trim()
                .trim_matches(|c: char| "\"'«»".contains(c))
                .to_string();
            Json(json!({"title": if title.is_empty() { "Conversation".to_string() } else { title }}))
                .into_response()
        }
        Err(e) => {
            warn!("generate-title failed: {}", e);
            Json(json!({"title": "Conversation"})).into_response()
        }
    }
}

// ── Conversations ─────────────────────────────────────────────────────────────

async fn list_conversations(State(state): State<Arc<AppState>>) -> Response {
    match state.db.list_conversations().await {
        Ok(convs) => Json(json!({"conversations": convs})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn save_conversation(
    State(state): State<Arc<AppState>>,
    Json(conv): Json<ConversationSave>,
) -> Response {
    let id = conv.id.clone();
    match state.db.save_conversation(conv).await {
        Ok(()) => Json(json!({"ok": true, "id": id})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn delete_conversation(
    State(state): State<Arc<AppState>>,
    Path(conv_id): Path<String>,
) -> Response {
    match state.db.delete_conversation(conv_id).await {
        Ok(true) => Json(json!({"ok": true})).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "Conversation introuvable."),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── User info ─────────────────────────────────────────────────────────────────

async fn get_user_me(Query(params): Query<EndpointBearerParams>) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/me/info", base))
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

// ── Models ────────────────────────────────────────────────────────────────────

async fn list_models(Query(params): Query<EndpointBearerParams>) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/models", base))
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({"data": []}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

// ── Extract ───────────────────────────────────────────────────────────────────

async fn extract_file(mut multipart: Multipart) -> Response {
    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = field.file_name().unwrap_or("file").to_string();
        let ext = filename
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string()),
        };

        return match extract_text(&bytes, &ext) {
            Ok(text) => {
                let chars = text.chars().count();
                Json(json!({"filename": filename, "text": text, "chars": chars})).into_response()
            }
            Err(e) => err(StatusCode::UNPROCESSABLE_ENTITY, e.to_string()),
        };
    }
    err(StatusCode::BAD_REQUEST, "Aucun fichier reçu.")
}

async fn extract_multiple_files(mut multipart: Multipart) -> Response {
    let mut results: Vec<Value> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();
    let mut count = 0;

    while let Ok(Some(field)) = multipart.next_field().await {
        if count >= 10 {
            break;
        }
        count += 1;

        let filename = field.file_name().unwrap_or("file").to_string();
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                errors.push(json!({"filename": filename, "error": e.to_string()}));
                continue;
            }
        };

        match extract_text(&bytes, &ext) {
            Ok(text) => {
                let chars = text.chars().count();
                results.push(json!({"filename": filename, "text": text, "chars": chars, "ext": ext}));
            }
            Err(e) => {
                errors.push(json!({"filename": filename, "error": e.to_string()}));
            }
        }
    }

    if results.is_empty() && !errors.is_empty() {
        return err(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Aucun fichier extrait. {:?}", errors),
        );
    }

    let total = results.len();
    Json(json!({"files": results, "errors": errors, "total": total})).into_response()
}

// ── Ingestion ─────────────────────────────────────────────────────────────────

async fn list_collections(Query(params): Query<EndpointBearerParams>) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/collections", base))
        .query(&[("limit", "100")])
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn create_collection(Json(req): Json<CollectionCreateRequest>) -> Response {
    let base = albert_base(&req.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    let body = json!({
        "name":        req.name,
        "description": req.description,
        "model":       "BAAI/bge-m3",
        "visibility":  req.visibility,
    });

    match client
        .post(format!("{}/collections", base))
        .bearer_auth(&req.bearer)
        .json(&body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn rename_collection(
    Path(collection_id): Path<i64>,
    Json(req): Json<CollectionRenameRequest>,
) -> Response {
    let base = albert_base(&req.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .patch(format!("{}/collections/{}", base, collection_id))
        .bearer_auth(&req.bearer)
        .json(&json!({"name": req.name}))
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            if resp.content_length() == Some(0) || status == StatusCode::NO_CONTENT {
                return Json(json!({"ok": true})).into_response();
            }
            let body: Value = resp.json().await.unwrap_or(json!({"ok": true}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn delete_collection_handler(
    Path(collection_id): Path<i64>,
    Query(params): Query<EndpointBearerParams>,
) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .delete(format!("{}/collections/{}", base, collection_id))
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Json(json!({"ok": true})).into_response(),
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            err(
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            )
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn list_documents(
    Path(collection_id): Path<i64>,
    Query(params): Query<EndpointBearerParams>,
) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/documents", base))
        .query(&[
            ("collection_id", collection_id.to_string()),
            ("limit", "100".to_string()),
        ])
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn delete_document(
    Path(document_id): Path<i64>,
    Query(params): Query<EndpointBearerParams>,
) -> Response {
    let base = albert_base(&params.endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .delete(format!("{}/documents/{}", base, document_id))
        .bearer_auth(&params.bearer)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => Json(json!({"ok": true})).into_response(),
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            err(
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY),
                body,
            )
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

async fn upload_document(mut multipart: Multipart) -> Response {
    // Extract fields from multipart
    let mut file_name = String::new();
    let mut file_bytes: Option<Bytes> = None;
    let mut file_content_type = String::from("application/octet-stream");
    let mut collection_id: Option<i64> = None;
    let mut endpoint = String::new();
    let mut bearer = String::new();
    let mut chunk_size = 1024i64;
    let mut chunk_overlap = 100i64;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("file").to_string();
                if let Some(ct) = field.content_type() {
                    file_content_type = ct.to_string();
                }
                file_bytes = field.bytes().await.ok();
            }
            "collection_id" => {
                if let Ok(text) = field.text().await {
                    collection_id = text.parse().ok();
                }
            }
            "endpoint" => endpoint = field.text().await.unwrap_or_default(),
            "bearer" => bearer = field.text().await.unwrap_or_default(),
            "chunk_size" => {
                if let Ok(text) = field.text().await {
                    chunk_size = text.parse().unwrap_or(1024);
                }
            }
            "chunk_overlap" => {
                if let Ok(text) = field.text().await {
                    chunk_overlap = text.parse().unwrap_or(100);
                }
            }
            _ => {}
        }
    }

    let cid = match collection_id {
        Some(id) => id,
        None => return err(StatusCode::BAD_REQUEST, "collection_id manquant"),
    };
    let bytes = match file_bytes {
        Some(b) => b,
        None => return err(StatusCode::BAD_REQUEST, "Fichier manquant"),
    };

    let base = albert_base(&endpoint);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    let part = reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(file_name.clone())
        .mime_str(&file_content_type)
        .unwrap_or_else(|_| reqwest::multipart::Part::bytes(bytes.to_vec()));

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("collection_id", cid.to_string())
        .text("chunk_size", chunk_size.to_string())
        .text("chunk_overlap", chunk_overlap.to_string())
        .text("preset_separators", "markdown");

    match client
        .post(format!("{}/documents", base))
        .bearer_auth(&bearer)
        .multipart(form)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), Json(body))
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}
