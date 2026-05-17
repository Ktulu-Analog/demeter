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


use reqwest::Client;
use tracing::{info, warn};

use crate::config;

/// Extrait une requête de recherche concise depuis le message utilisateur.
/// Ex : "quelles sont les informations du jour sur le site Le Figaro ?"
///   → "Le Figaro actualités du jour"
fn extract_search_query(user_msg: &str) -> String {
    // Supprime les tournures interrogatives françaises courantes
    let msg = user_msg.trim();
    let patterns = [
        "quelles sont les ", "quels sont les ", "quelle est ", "quel est ",
        "qu'est-ce que ", "qu'est-ce qui ", "est-ce que ", "est-ce qu'",
        "pouvez-vous me dire ", "peux-tu me dire ", "dis-moi ",
        "donne-moi ", "donnez-moi ", "explique-moi ", "expliquez-moi ",
        "comment ", "pourquoi ", "où ", "quand ",
        "récupère ", "récupère sur internet ", "cherche ",
        "trouve ", "recherche ",
    ];
    let mut cleaned = msg.to_lowercase();
    for p in &patterns {
        if cleaned.starts_with(p) {
            cleaned = cleaned[p.len()..].to_string();
            break;
        }
    }
    // Supprime les suffixes parasites
    let suffixes = [" ?", "?", " !", "!", " s'il te plaît", " svp", " stp"];
    for s in &suffixes {
        if cleaned.ends_with(s) {
            cleaned = cleaned[..cleaned.len() - s.len()].to_string();
        }
    }
    // Si la cleaning a produit quelque chose de raisonnable, l'utiliser
    let result = cleaned.trim().to_string();
    if result.len() >= 5 && result.len() < msg.len() {
        // Recapitalise le premier caractère
        let mut chars = result.chars();
        match chars.next() {
            None => msg.to_string(),
            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        }
    } else {
        msg.to_string()
    }
}

/// Appelle l'API Tavily et retourne un bloc de contexte prêt à injecter dans le system prompt.
/// Utilise search_depth=advanced et include_raw_content=true pour obtenir le contenu réel des pages.
pub async fn fetch_web_search(query: &str, max_results: usize, tavily_key: &str) -> String {
    if tavily_key.is_empty() {
        return format!(
            "## Recherche web pour : «{}»\n\n\
             ⚠️ Aucune clé Tavily configurée. \
             Renseigne ta clé API dans les paramètres (⚙️) pour activer la recherche web.\n",
            query
        );
    }

    // Optimise la query pour la recherche
    let search_query = extract_search_query(query);
    info!("Tavily: query originale={:?} → query optimisée={:?}", query, search_query);

    let client = match Client::builder()
        .timeout(config::timeout_web_search())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("Tavily: impossible de créer le client HTTP: {}", e);
            return format!("## Recherche web\n\nErreur client HTTP : {}\n", e);
        }
    };

    let body = serde_json::json!({
        "api_key": tavily_key,
        "query": search_query,
        "search_depth": "advanced",       // récupère le contenu réel des pages
        "include_answer": true,            // synthèse Tavily en bonus
        "include_raw_content": true,       // contenu texte complet de chaque page
        "max_results": max_results,
        "include_domains": [],
        "exclude_domains": []
    });

    match client
        .post("https://api.tavily.com/search")
        .json(&body)
        .send()
        .await
    {
        Err(e) => {
            warn!("Tavily request failed: {}", e);
            format!(
                "## Recherche web pour : «{}»\n\n\
                 Impossible de contacter Tavily ({}). \
                 Réponds avec tes connaissances en précisant leur date limite.\n",
                query, e
            )
        }
        Ok(resp) if !resp.status().is_success() => {
            let status = resp.status();
            let body_text = resp.text().await.unwrap_or_default();
            warn!("Tavily HTTP {}: {}", status, body_text);
            format!(
                "## Recherche web pour : «{}»\n\n\
                 Erreur Tavily (HTTP {}) : {}. \
                 Réponds avec tes connaissances en précisant leur date limite.\n",
                query, status, body_text
            )
        }
        Ok(resp) => {
            match resp.json::<serde_json::Value>().await {
                Err(e) => {
                    warn!("Tavily JSON parse error: {}", e);
                    format!("## Recherche web\n\nErreur de parsing Tavily : {}\n", e)
                }
                Ok(data) => {
                    let mut parts: Vec<String> = Vec::new();

                    // Réponse synthétique de Tavily (optionnelle)
                    if let Some(answer) = data["answer"].as_str() {
                        if !answer.is_empty() {
                            parts.push(format!("**Synthèse Tavily :** {}", answer));
                        }
                    }

                    // Résultats individuels — priorité au raw_content, fallback sur content
                    if let Some(results) = data["results"].as_array() {
                        for r in results.iter().take(max_results) {
                            let title = r["title"].as_str().unwrap_or("Sans titre");
                            let url   = r["url"].as_str().unwrap_or("");

                            // raw_content = texte complet extrait de la page (jusqu'à ~10 000 chars)
                            // content     = snippet court (200-400 chars) — fallback
                            let raw = r["raw_content"].as_str().unwrap_or("").trim();
                            let snippet = r["content"].as_str().unwrap_or("").trim();

                            let body_text = if !raw.is_empty() {
                                // Tronquer à 4 000 chars pour ne pas saturer le contexte
                                let truncated = if raw.len() > 4000 {
                                    let cutoff = raw.char_indices()
                                        .map(|(i, _)| i)
                                        .nth(4000)
                                        .unwrap_or(raw.len());
                                    &raw[..cutoff]
                                } else {
                                    raw
                                };
                                truncated.to_string()
                            } else if !snippet.is_empty() {
                                snippet.to_string()
                            } else {
                                continue;
                            };

                            parts.push(format!(
                                "### {}\nSource : {}\n\n{}",
                                title, url, body_text
                            ));
                        }
                    }

                    info!("Tavily: {} résultats pour {:?}", parts.len(), search_query);

                    if parts.is_empty() {
                        return format!(
                            "## Recherche web pour : «{}»\n\n\
                             Aucun résultat Tavily. \
                             Réponds avec tes connaissances en précisant leur date limite.\n",
                            query
                        );
                    }

                    // Extraire les images retournées par Tavily (champ "images")
                    let mut image_lines: Vec<String> = Vec::new();
                    if let Some(images) = data["images"].as_array() {
                        for img in images.iter().take(4) {
                            let url = img.as_str()
                                .map(|s| s.to_string())
                                .or_else(|| img["url"].as_str().map(|s| s.to_string()));
                            let desc = img["description"].as_str().unwrap_or("").to_string();
                            if let Some(u) = url {
                                if !u.is_empty() {
                                    let label = if desc.is_empty() { "Image".to_string() } else { desc };
                                    image_lines.push(format!("![{}]({})", label, u));
                                }
                            }
                        }
                    }

                    let images_section = if !image_lines.is_empty() {
                        format!(
                            "\n\n## Images disponibles\nSi demandé, affiche avec ![desc](url) :\n{}",
                            image_lines.join("\n")
                        )
                    } else {
                        String::new()
                    };

                    format!(
                        "## Résultats de recherche web pour : «{}»\n                         (requête optimisée : «{}»)\n\n{}{}\n\n---\n                         Utilise ces informations pour répondre. Cite les sources quand pertinent.\n                         Si l'utilisateur demande une image, insère-la avec ![description](url).\n",
                        query,
                        search_query,
                        parts.join("\n\n---\n\n"),
                        images_section
                    )
                }
            }
        }
    }
}
