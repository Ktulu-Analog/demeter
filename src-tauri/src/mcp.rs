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
            Err(e) => { warn!("MCP SSE read error: {}", e); break; }
        };
        let text = String::from_utf8_lossy(&bytes).to_string();
        buf.push_str(&text);

        // Les blocs SSE sont séparés par "\r\n\r\n" ou "\n\n"
        loop {
            let sep = if let Some(i) = buf.find("\r\n\r\n") {
                Some((i, 4))
            } else if let Some(i) = buf.find("\n\n") {
                Some((i, 2))
            } else {
                None
            };
            let (end, sep_len) = match sep { Some(s) => s, None => break };

            let block = buf[..end].to_string();
            buf = buf[end + sep_len..].to_string();

            let mut data = String::new();
            for line in block.lines() {
                // On ignore "event:", "id:", "retry:" — seul "data:" nous intéresse
                if let Some(r) = line.strip_prefix("data:") {
                    data = r.trim().to_string();
                }
            }

            if data.is_empty() || data == "[DONE]" { continue; }

            match serde_json::from_str::<Value>(&data) {
                Ok(v) => {
                    if v.get("result").is_some() || v.get("error").is_some() {
                        return Some(v);
                    }
                }
                Err(e) => { warn!("MCP SSE JSON parse error: {}", e); }
            }
        }
    }
    warn!("MCP SSE stream ended — no result found (buf remaining: {}b)", buf.len());
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
            warn!("MCP: send() failed — is_timeout={} is_connect={} is_builder={} — {}",
                e.is_timeout(), e.is_connect(), e.is_builder(), e);
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
        warn!("MCP HTTP {} (x-deny-reason: {}) — {}", status, deny_reason, body_text);
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
pub async fn list_tools(client: &Client, server_url: &str) -> Vec<Value> {
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
                return tools.clone();
            }
            if let Some(err) = data.get("error") {
                warn!("MCP tools/list error depuis {}: {}", server_url, err);
            } else {
                warn!("MCP: réponse inattendue de {}: {:?}", server_url, data);
            }
            vec![]
        }
        None => {
            warn!("MCP: serveur injoignable — {}", server_url);
            vec![]
        }
    }
}

/// Convertit un tool MCP au format OpenAI function tool.
pub fn mcp_tool_to_openai(tool: &Value, server_url: &str) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": format!("mcp__{}__{}", slugify(server_url), tool["name"].as_str().unwrap_or("unknown")),
            "description": tool["description"].as_str().unwrap_or(""),
            "parameters": tool.get("inputSchema").cloned().unwrap_or(json!({"type":"object","properties":{}}))
        }
    })
}

/// Exécute un tool MCP et retourne le résultat sous forme de string.
pub async fn call_tool(client: &Client, server_url: &str, tool_name: &str, arguments: &Value) -> String {
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
                return format!("Erreur MCP : {}", err);
            }
            extract_content(&data["result"])
        }
        None => {
            warn!("MCP: échec d'appel du tool {} sur {}", tool_name, server_url);
            format!("Erreur : impossible d'appeler le tool {} sur {}", tool_name, server_url)
        }
    }
}

/// Collecte tous les tools de tous les serveurs MCP configurés.
pub async fn collect_all_tools(client: &Client, servers: &[String]) -> Vec<(String, Value)> {
    let mut result = Vec::new();
    for server in servers {
        let tools = list_tools(client, server).await;
        info!("MCP: {} tools depuis {}", tools.len(), server);
        for tool in tools {
            result.push((server.clone(), tool));
        }
    }
    result
}

/// Extrait le contenu textuel d'une réponse MCP.
fn extract_content(data: &Value) -> String {
    // Format MCP standard : { content: [{type:"text", text:"..."}] }
    if let Some(content) = data["content"].as_array() {
        return content
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(s) = data.as_str() { return s.to_string(); }
    data.to_string()
}

/// Transforme une URL en slug pour les noms de tools.
pub fn server_slug(url: &str) -> String { slugify(url) }

fn slugify(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
        .chars()
        .take(20)
        .collect()
}
