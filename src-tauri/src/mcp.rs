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

// Client MCP — transport HTTP Streamable (spec 2025-03-26)
//
// Compatible avec mcp.data.gouv.fr et tout serveur FastMCP en mode stateless_http.
//
// Une seule requête POST par appel. Headers obligatoires :
//   Content-Type: application/json
//   Accept: application/json, text/event-stream   ← les DEUX ensemble, obligatoire
//   Origin: {scheme}://{host}                     ← obligatoire (anti-DNS-rebinding)

use futures::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::time::Duration;
use tracing::{info, warn};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extrait l'origine (scheme + host) d'une URL pour le header Origin.
/// "https://mcp.data.gouv.fr/mcp" → "https://mcp.data.gouv.fr"
fn origin_from_url(url: &str) -> String {
    if let Some(scheme_end) = url.find("://") {
        let after = &url[scheme_end + 3..];
        let host_end = after.find('/').unwrap_or(after.len());
        return format!("{}://{}", &url[..scheme_end], &after[..host_end]);
    }
    url.to_string()
}

/// Parse un stream SSE et retourne la première réponse JSON-RPC valide.
async fn parse_sse_stream(resp: reqwest::Response) -> Option<Value> {
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                warn!("MCP SSE read error: {}", e);
                break;
            }
        };
        let text = String::from_utf8_lossy(&bytes).to_string();
        buf.push_str(&text);

        // Les blocs SSE sont séparés par "\r\n\r\n" ou "\n\n"
        loop {
            let sep = buf
                .find("\r\n\r\n")
                .map(|i| (i, 4))
                .or_else(|| buf.find("\n\n").map(|i| (i, 2)));
            let (end, sep_len) = match sep {
                Some(s) => s,
                None => break,
            };

            let block = buf[..end].to_string();
            buf = buf[end + sep_len..].to_string();

            let mut data = String::new();
            for line in block.lines() {
                // On ignore "event:", "id:", "retry:" — seul "data:" nous intéresse
                if let Some(r) = line.strip_prefix("data:") {
                    data = r.trim().to_string();
                }
            }

            if data.is_empty() || data == "[DONE]" {
                continue;
            }

            match serde_json::from_str::<Value>(&data) {
                Ok(v) => {
                    if v.get("result").is_some() || v.get("error").is_some() {
                        return Some(v);
                    }
                }
                Err(e) => {
                    warn!("MCP SSE JSON parse error: {}", e);
                }
            }
        }
    }
    warn!(
        "MCP SSE stream ended — no result found (buf remaining: {}b)",
        buf.len()
    );
    None
}

/// Normalise une URL MCP : ajoute https:// si absent.
fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    }
}

// ── Appel JSON-RPC principal ──────────────────────────────────────────────────

/// Envoie un message JSON-RPC au serveur MCP et retourne la réponse.
/// Gère les réponses JSON directes et SSE.
async fn rpc_post(
    client: &Client,
    server_url: &str,
    body: &Value,
    timeout_secs: u64,
) -> Option<Value> {
    let server_url = &normalize_url(server_url);
    let server_url: &str = server_url.as_str();
    let origin = origin_from_url(server_url);

    let resp = match client
        .post(server_url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .header("Origin", &origin)
        .header("MCP-Protocol-Version", "2025-03-26")
        .header("User-Agent", "Mozilla/5.0 (compatible; Demeter/1.0)")
        .json(body)
        .timeout(Duration::from_secs(timeout_secs))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            warn!(
                "MCP: send() failed — is_timeout={} is_connect={} is_builder={} — {}",
                e.is_timeout(),
                e.is_connect(),
                e.is_builder(),
                e
            );
            if let Some(url) = e.url() {
                warn!("MCP: failed URL = {}", url);
            }
            return None;
        }
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let deny_reason = resp
            .headers()
            .get("x-deny-reason")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-")
            .to_string();
        let body_text = resp.text().await.unwrap_or_default();
        warn!(
            "MCP HTTP {} (x-deny-reason: {}) — {}",
            status, deny_reason, body_text
        );
        return None;
    }

    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if content_type.contains("application/json") {
        // Réponse JSON directe (mode json_response activé)
        resp.json::<Value>().await.ok()
    } else if content_type.contains("text/event-stream") {
        // Réponse SSE (mode par défaut de FastMCP)
        parse_sse_stream(resp).await
    } else {
        warn!("MCP: Content-Type inattendu: {}", content_type);
        None
    }
}

// ── API publique ──────────────────────────────────────────────────────────────

/// Récupère la liste des tools depuis un serveur MCP.
///
/// Le serveur est en mode stateless → pas besoin d'initialize préalable.
/// Un seul POST suffit.
pub async fn list_tools(client: &Client, server_url: &str) -> (Vec<Value>, bool) {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    match rpc_post(client, server_url, &body, 10).await {
        Some(data) => {
            if let Some(tools) = data["result"]["tools"].as_array() {
                info!("MCP: {} tools depuis {}", tools.len(), server_url);
                return (tools.clone(), true);
            }
            if let Some(err) = data.get("error") {
                warn!("MCP tools/list error depuis {}: {}", server_url, err);
            } else {
                warn!("MCP: réponse inattendue de {}: {:?}", server_url, data);
            }
            (vec![], false)
        }
        None => {
            warn!("MCP: serveur injoignable — {}", server_url);
            (vec![], false)
        }
    }
}

/// Convertit un tool MCP au format OpenAI function tool.
pub fn mcp_tool_to_openai(tool: &Value, server_url: &str) -> Value {
    let tool_name = tool["name"].as_str().unwrap_or("unknown");
    let fn_name = format!("mcp__{}__{}", slugify(server_url), tool_name);

    // Filet de sécurité : l'API rejette tout nom de fonction > 64 caractères.
    // Avec SLUG_MAX_LEN=12 ça ne devrait plus arriver, mais si un futur outil
    // MCP a un nom très long, on préfère le signaler bruyamment dans les logs
    // plutôt que de laisser l'appel échouer en silence côté API.
    if fn_name.len() > 64 {
        warn!(
            "MCP: nom de fonction '{}' ({} caractères) dépasse la limite de 64 imposée par l'API — \
             l'outil '{}' du serveur '{}' risque de provoquer une erreur 400. \
             Envisagez de raccourcir SLUG_MAX_LEN ou le nom de l'outil côté serveur MCP.",
            fn_name, fn_name.len(), tool_name, server_url
        );
    }

    json!({
        "type": "function",
        "function": {
            "name": fn_name,
            "description": tool["description"].as_str().unwrap_or(""),
            "parameters": tool.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{}}))
        }
    })
}

/// Résultat d'un appel tool MCP.
/// Retourne le texte, et optionnellement une image (b64 + mime).
pub struct McpToolResult {
    pub text: String,
    pub image_b64: Option<String>,
    pub image_mime: Option<String>,
}

/// Exécute un tool MCP et retourne le résultat structuré.
pub async fn call_tool(
    client: &Client,
    server_url: &str,
    tool_name: &str,
    arguments: &Value,
) -> McpToolResult {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": { "name": tool_name, "arguments": arguments }
    });

    match rpc_post(client, server_url, &body, 30).await {
        Some(data) => {
            if let Some(err) = data.get("error") {
                warn!("MCP tools/call error: {}", err);
                return McpToolResult {
                    text: format!("Erreur MCP : {}", err),
                    image_b64: None,
                    image_mime: None,
                };
            }
            extract_content(&data["result"])
        }
        None => {
            warn!(
                "MCP: échec d'appel du tool {} sur {}",
                tool_name, server_url
            );
            McpToolResult {
                text: format!(
                    "Erreur : impossible d'appeler le tool {} sur {}",
                    tool_name, server_url
                ),
                image_b64: None,
                image_mime: None,
            }
        }
    }
}

/// Extrait le contenu d'une réponse MCP (texte + image optionnelle).
fn extract_content(data: &Value) -> McpToolResult {
    if let Some(content) = data["content"].as_array() {
        let mut texts: Vec<&str> = Vec::new();
        let mut image_b64: Option<String> = None;
        let mut image_mime: Option<String> = None;

        for block in content {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        texts.push(t);
                    }
                }
                Some("image") => {
                    image_b64 = block["data"].as_str().map(|s| s.to_string());
                    image_mime = block["mimeType"].as_str().map(|s| s.to_string());
                }
                _ => {}
            }
        }

        return McpToolResult {
            text: texts.join("\n"),
            image_b64,
            image_mime,
        };
    }
    if let Some(s) = data.as_str() {
        return McpToolResult {
            text: s.to_string(),
            image_b64: None,
            image_mime: None,
        };
    }
    McpToolResult {
        text: data.to_string(),
        image_b64: None,
        image_mime: None,
    }
}

/// Transforme une URL en slug pour les noms de tools.
pub fn server_slug(url: &str) -> String {
    slugify(url)
}

// Longueur max du slug serveur dans les noms de fonction `mcp__{slug}__{outil}`.
//
// L'API impose un nom de fonction total <= 64 caractères. Avec le préfixe
// "mcp__" (5) et le séparateur "__" (2), et le plus long nom d'outil connu
// à ce jour (legifrance_lister_questions_parlementaires, 42 caractères),
// il reste 64 - 5 - 2 - 42 = 15 caractères disponibles pour le slug.
// On se limite à 12 pour garder de la marge si des outils plus longs
// apparaissent, tout en restant suffisamment distinctif entre serveurs.
const SLUG_MAX_LEN: usize = 12;

fn slugify(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
        .chars()
        .take(SLUG_MAX_LEN)
        .collect()
}
