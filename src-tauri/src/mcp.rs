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

// ── Garde réseau (D-10) ───────────────────────────────────────────────────────
// Les serveurs MCP locaux ou de réseau privé restent autorisés (choix produit :
// MCP maison, ComfyUI…). Sont refusées les cibles qui n'ont aucune raison d'être
// un serveur MCP : adresses link-local / métadonnées cloud (169.254.0.0/16,
// fe80::/10, 100.100.100.200, metadata.google.internal), non spécifiées.

/// Taille maximale d'une réponse MCP (les images base64 sont volumineuses).
const MAX_RESPONSE_BYTES: usize = 25 * 1024 * 1024;

fn is_forbidden_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_link_local() || v4.is_unspecified() || v4.octets() == [100, 100, 100, 200]
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_forbidden_ip(std::net::IpAddr::V4(v4));
            }
            v6.is_unspecified() || (v6.segments()[0] & 0xFFC0) == 0xFE80
        }
    }
}

fn is_forbidden_literal_host(host: &str) -> bool {
    let h = host.trim_start_matches('[').trim_end_matches(']');
    if h.eq_ignore_ascii_case("metadata.google.internal") {
        return true;
    }
    h.parse::<std::net::IpAddr>()
        .map(is_forbidden_ip)
        .unwrap_or(false)
}

/// Politique de redirection : 5 max, jamais de rétrogradation https → http,
/// jamais vers une adresse interdite.
pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("trop de redirections");
        }
        let was_https = attempt
            .previous()
            .last()
            .map(|u| u.scheme() == "https")
            .unwrap_or(false);
        if was_https && attempt.url().scheme() != "https" {
            return attempt.error("redirection https vers http refusée");
        }
        if attempt
            .url()
            .host_str()
            .map(is_forbidden_literal_host)
            .unwrap_or(false)
        {
            return attempt.error("redirection vers une adresse interdite");
        }
        attempt.follow()
    })
}

/// Valide la cible d'un appel MCP (schéma, identifiants, adresse, résolution DNS).
async fn check_target(url: &str) -> Result<(), String> {
    let u = reqwest::Url::parse(url).map_err(|_| "URL invalide".to_string())?;
    if !matches!(u.scheme(), "http" | "https") {
        return Err("schéma non autorisé (http/https uniquement)".into());
    }
    if !u.username().is_empty() || u.password().is_some() {
        return Err("identifiants dans l'URL refusés".into());
    }
    let host = u.host_str().ok_or_else(|| "URL sans hôte".to_string())?;
    if is_forbidden_literal_host(host) {
        return Err("adresse interdite (link-local / métadonnées)".into());
    }
    if host
        .trim_start_matches('[')
        .parse::<std::net::IpAddr>()
        .is_err()
    {
        let port = u.port_or_known_default().unwrap_or(443);
        if let Ok(addrs) = tokio::net::lookup_host((host, port)).await {
            if addrs.into_iter().any(|a| is_forbidden_ip(a.ip())) {
                return Err("le nom résout vers une adresse interdite".into());
            }
        }
    }
    Ok(())
}

/// Lit le corps d'une réponse en plafonnant sa taille.
async fn read_capped(mut resp: reqwest::Response) -> Option<Vec<u8>> {
    if resp
        .content_length()
        .map(|l| l as usize > MAX_RESPONSE_BYTES)
        .unwrap_or(false)
    {
        warn!("MCP: réponse trop volumineuse (Content-Length)");
        return None;
    }
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = resp.chunk().await {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            warn!(
                "MCP: réponse trop volumineuse (> {} octets)",
                MAX_RESPONSE_BYTES
            );
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

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
    let mut total = 0usize;

    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => {
                warn!("MCP SSE read error: {}", e);
                break;
            }
        };
        total += bytes.len();
        if total > MAX_RESPONSE_BYTES || buf.len() > MAX_RESPONSE_BYTES {
            warn!("MCP SSE: flux trop volumineux, abandon");
            return None;
        }
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

    if let Err(reason) = check_target(server_url).await {
        warn!("MCP: cible refusée — {}", reason);
        return None;
    }

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
        let body_text: String = resp
            .text()
            .await
            .unwrap_or_default()
            .chars()
            .take(300)
            .collect();
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
        let bytes = read_capped(resp).await?;
        serde_json::from_slice::<Value>(&bytes).ok()
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

// Slug serveur dans les noms de fonction `mcp__{slug}__{outil}`.
const SLUG_READABLE_LEN: usize = 6;

fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in bytes {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn slugify(url: &str) -> String {
    let id = url.trim();
    let readable: String = id
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .take(SLUG_READABLE_LEN)
        .collect();
    let readable = if readable.is_empty() {
        "x".to_string()
    } else {
        readable
    };
    format!("{}_{:05x}", readable, fnv1a(id.as_bytes()) & 0xF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_distincts_pour_urls_au_meme_debut() {
        let a = slugify("https://mcp.exemple.fr/a");
        let b = slugify("https://mcp.exemple.fr/b");
        assert_ne!(a, b);
        assert!(a.len() <= 12 && b.len() <= 12);
        assert!(!a.contains("__") && !b.contains("__"));
    }

    #[test]
    fn slug_stdio_et_vide_sans_double_underscore() {
        assert!(!slugify("stdio:mcp-grist").contains("__"));
        assert!(!slugify("::").contains("__"));
    }

    #[test]
    fn adresses_interdites() {
        assert!(is_forbidden_literal_host("169.254.169.254"));
        assert!(is_forbidden_literal_host("[fe80::1]"));
        assert!(is_forbidden_literal_host("metadata.google.internal"));
        assert!(is_forbidden_literal_host("100.100.100.200"));
        assert!(!is_forbidden_literal_host("127.0.0.1"));
        assert!(!is_forbidden_literal_host("192.168.1.10"));
        assert!(!is_forbidden_literal_host("mcp.data.gouv.fr"));
    }
}
