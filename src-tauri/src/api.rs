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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config;

/// Tronque une string UTF-8 à `max_bytes` octets sans couper un caractère multi-octet.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut boundary = max_bytes;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    &s[..boundary]
}

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, HeaderName, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post, put},
    Json, Router,
};
use bytes::Bytes;
use futures::stream::StreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::{error, info, warn};

use crate::{
    db::Database,
    extract::extract_text_async,
    models::*,
    rag::{
        self as rag, albert_base, build_system_with_rag, chunks_to_context,
        collection_exists as qdrant_collection_exists, generate_qdrant_jwt_with_ttl, is_jwt,
        rerank_chunks, retrieve_context_with_chunks, search_chunks, QdrantCollectionAccess,
    },
    spaces::{load_spaces, save_spaces},
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

const ECHARTS_INSTRUCTION: &str = r##"

──────────────────────────────────────────
RENDU DE GRAPHIQUES
Quand une réponse bénéficierait d'une visualisation, insère un bloc "echarts" contenant un objet JSON valide.

RÈGLES JSON STRICTES : JSON RFC 8259 pur, toutes les clés entre guillemets doubles, aucun commentaire (ni // ni /* */), pas d'apostrophes comme délimiteurs de chaîne, pas de virgule après le dernier élément, pas de fonctions JS.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
MISE EN PAGE INTELLIGENTE — règles obligatoires
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

TITRE :
- Toujours présent. "left":"center", "top":"2%".
- Si un sous-titre est utile (total, date…) : "subtext":"…", "subtextStyle":{"color":"#888","fontSize":11}.

LÉGENDE :
- CAMEMBERT ≤ 6 séries : "legend":{"orient":"vertical","right":"5%","top":"middle"}.
  Le centre du pie doit être décalé à gauche : "center":["38%","54%"].
- CAMEMBERT ≥ 7 séries : supprimer la légende ("legend":{"show":false}).
  Utiliser des labels extérieurs : "label":{"show":true,"position":"outside","formatter":"{b}: {d}%","overflow":"truncate","width":110}
  et "labelLine":{"length":8,"length2":6} et "labelLayout":{"hideOverlap":true}.
- BARRES / LIGNES : "legend":{"bottom":"2%","type":"scroll"}.

CAMEMBERT (pie) — règles supplémentaires :
- "avoidLabelOverlap":true TOUJOURS présent.
- Pie plein : "radius":["0%","55%"]. Donut : "radius":["35%","60%"].
- "center":["50%","54%"] par défaut ; ["38%","54%"] quand la légende est verticale à droite.
- Si ≥ 7 séries, réduire : "radius":["0%","47%"], "center":["42%","54%"].
- Ne jamais utiliser "label":{"position":"inner"} avec plus de 5 séries.

BARRES HORIZONTALES (bar + yAxis category) :
- "grid":{"left":"28%","right":"12%","top":"14%","bottom":"12%"}.
- "yAxis":{"axisLabel":{"overflow":"truncate","width":160}}.

BARRES VERTICALES / LIGNES :
- "grid":{"left":"8%","right":"6%","top":"14%","bottom":"18%"}.
- Si > 6 catégories : "xAxis":{"axisLabel":{"rotate":30,"overflow":"truncate","width":80}}.

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Exemple camembert 8 séries (correct) :
```echarts
{"title":{"text":"Répartition","left":"center","top":"2%","subtext":"Total : 31","subtextStyle":{"color":"#888","fontSize":11}},"tooltip":{"trigger":"item","formatter":"{b}: {c} ({d}%)"},"legend":{"show":false},"series":[{"type":"pie","radius":["0%","47%"],"center":["42%","54%"],"avoidLabelOverlap":true,"label":{"show":true,"position":"outside","formatter":"{b}: {d}%","overflow":"truncate","width":110},"labelLine":{"length":8,"length2":6},"labelLayout":{"hideOverlap":true},"data":[{"value":8,"name":"Intérieur"},{"value":3,"name":"Justice"}]}]}
```

Exemple camembert 4 séries (avec légende) :
```echarts
{"title":{"text":"Répartition","left":"center","top":"2%"},"tooltip":{"trigger":"item","formatter":"{b}: {c} ({d}%)"},"legend":{"orient":"vertical","right":"5%","top":"middle"},"series":[{"type":"pie","radius":["0%","55%"],"center":["38%","54%"],"avoidLabelOverlap":true,"label":{"show":false},"data":[{"value":40,"name":"A"},{"value":30,"name":"B"},{"value":20,"name":"C"},{"value":10,"name":"D"}]}]}
```
──────────────────────────────────────────
"##;

const MERMAID_INSTRUCTION: &str = r#"

──────────────────────────────────────────
DIAGRAMMES MERMAID
Quand une réponse bénéficierait d'un diagramme de séquence, d'état ou d'un Gantt, insère un bloc mermaid après l'explication textuelle.
NE PAS utiliser Mermaid pour des schémas d'architecture ou des dessins libres — utiliser Excalidraw à la place.
Règles : IDs de nœuds en ASCII sans accent, pas de mots réservés comme IDs.
──────────────────────────────────────────
"#;

const SVG_INSTRUCTION: &str = r##"

──────────────────────────────────────────
SCHÉMAS SVG
Pour tout schéma d'architecture, organigramme, schéma technique ou dessin libre, utilise OBLIGATOIREMENT un bloc ```svg contenant du SVG valide.
C'est le format PAR DÉFAUT dès que l'utilisateur demande un "schéma", un "dessin" ou une "architecture".
NE PAS utiliser Mermaid pour des schémas libres — utiliser SVG à la place.

RÈGLES ABSOLUES :
- Le bloc SVG doit commencer par <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 500" width="100%" ...>
- Utiliser uniquement des éléments SVG standards : <rect>, <circle>, <ellipse>, <line>, <path>, <polyline>, <polygon>, <text>, <g>, <defs>, <marker>, <arrow>
- Pas de <script>, pas de handlers on*
- Textes centrés dans leurs formes avec <text dominant-baseline="middle" text-anchor="middle">
- Les flèches utilisent un <marker id="arrow"> avec refX/refY et orient="auto"
- Espacer les éléments au minimum 40px

PALETTE :
- Fond des boîtes : #e8f4fd (bleu clair), #e8fdf0 (vert clair), #fdf3e8 (orange clair), #fde8e8 (rouge clair)
- Contours et texte : #1a1a2e
- Flèches et lignes : #555577

EXEMPLE (schéma client-serveur) :
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 200" width="100%" style="font-family:sans-serif;font-size:14px">
  <defs>
    <marker id="arrow" markerWidth="10" markerHeight="7" refX="10" refY="3.5" orient="auto">
      <polygon points="0 0, 10 3.5, 0 7" fill="#555577"/>
    </marker>
  </defs>
  <rect x="40" y="70" width="140" height="60" rx="8" fill="#e8f4fd" stroke="#1a1a2e" stroke-width="1.5"/>
  <text x="110" y="100" dominant-baseline="middle" text-anchor="middle" fill="#1a1a2e">Client</text>
  <line x1="180" y1="100" x2="280" y2="100" stroke="#555577" stroke-width="1.5" marker-end="url(#arrow)"/>
  <text x="230" y="90" text-anchor="middle" fill="#555577" font-size="11">HTTP</text>
  <rect x="280" y="70" width="140" height="60" rx="8" fill="#e8fdf0" stroke="#1a1a2e" stroke-width="1.5"/>
  <text x="350" y="100" dominant-baseline="middle" text-anchor="middle" fill="#1a1a2e">Serveur</text>
  <line x1="420" y1="100" x2="520" y2="100" stroke="#555577" stroke-width="1.5" marker-end="url(#arrow)"/>
  <rect x="420" y="70" width="140" height="60" rx="8" fill="#fdf3e8" stroke="#1a1a2e" stroke-width="1.5"/>
  <text x="490" y="100" dominant-baseline="middle" text-anchor="middle" fill="#1a1a2e">Base de données</text>
</svg>
```

Adapte les formes, couleurs et labels à la demande. Vise un résultat clair et lisible.
──────────────────────────────────────────
"##;

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

const MCP_IMAGE_INSTRUCTION: &str = r#"

──────────────────────────────────────────
OUTILS MCP — IMAGES
Quand un outil MCP retourne une image (screenshot, capture, etc.), l'image est DEJA affichee directement dans le chat par le systeme.
- NE PAS mentionner "chargement", "affichage", "ci-dessus", "ci-dessous" ni aucune reference a l'image
- NE PAS ecrire de syntaxe markdown image ![...](...)
- Commenter UNIQUEMENT le contenu visible : ce que montre la page, les informations pertinentes
- Etre direct et factuel sur ce que l'image contient
──────────────────────────────────────────
"#;

fn default_system() -> String {
    format!(
        "Tu es Demeter, un assistant RH expert et bienveillant.\nReponds en francais, de facon claire, structuree et professionnelle.{}{}{}{}{}",
        LATEX_INSTRUCTION, ECHARTS_INSTRUCTION, MERMAID_INSTRUCTION, SVG_INSTRUCTION, WORD_INSTRUCTION
    )
}

fn append_instructions(system: &str) -> String {
    format!(
        "{}{}{}{}{}{}",
        system,
        LATEX_INSTRUCTION,
        ECHARTS_INSTRUCTION,
        MERMAID_INSTRUCTION,
        SVG_INSTRUCTION,
        WORD_INSTRUCTION
    )
}

// ── App state ─────────────────────────────────────────────────────────────────

/// Entrée de token JWT par collection (pour les utilisateurs non-admin).
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionToken {
    pub collection: String,
    pub token: String, // JWT signé par l'admin, scopé à cette collection
}

#[derive(Debug, Default, Clone)]
pub struct Credentials {
    pub endpoint: String,
    pub bearer: String,
    /// Clé Qdrant principale : clé admin brute OU token JWT utilisateur global.
    pub qdrant_api_key: Option<String>,
    pub qdrant_url: Option<String>,
    pub user_email: Option<String>,
    /// Tokens JWT additionnels par collection (pour les utilisateurs non-admin
    /// ayant accès à plusieurs collections via des tokens distincts).
    /// Chaque entrée est scopée à une collection précise.
    pub qdrant_collection_tokens: Vec<CollectionToken>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub prompts_path: PathBuf,
    pub credentials: Arc<RwLock<Credentials>>,
    pub mcp_stdio: crate::mcp_stdio::StdioManager,
    /// Valeurs de configuration saisies par l'utilisateur dans le panel MCP,
    /// par serveur puis par clé (ex : mcp_env["mcp-grist"]["GRIST_API_KEY"]).
    /// Synchronisées depuis le frontend via la commande Tauri `init_credentials`.
    pub mcp_env: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Dossier des MCP officiels embarqués avec l'application (resources
    /// Tauri), utilisé par le bouton "Installer les MCP officiels" du panel.
    /// `None` si l'app ne bundle aucun MCP (ex : build de dev sans resources).
    pub official_mcp_dir: Option<PathBuf>,
    /// Jeton secret de l'API locale (D-01). Généré à chaque lancement, transmis
    /// au frontend uniquement par la commande IPC `get_api_token`.
    pub api_token: String,
    /// Serveurs MCP autorisés : exactement ceux de la configuration enregistrée
    /// par `init_credentials` (D-03). Toute autre valeur reçue en HTTP est ignorée.
    pub mcp_allowed: Arc<RwLock<std::collections::HashSet<String>>>,
    /// État de sécurité (base chiffrée, trousseau) exposé par `get_security_status`.
    pub security: crate::secrets::SecurityStatus,
}

// ── Router ────────────────────────────────────────────────────────────────────

pub fn build_router(
    db: Database,
    prompts_path: PathBuf,
    official_mcp_dir: Option<PathBuf>,
    api_token: String,
    port: u16,
    security: crate::secrets::SecurityStatus,
) -> (Router, Arc<AppState>) {
    let credentials = Arc::new(RwLock::new(Credentials::default()));
    let state = Arc::new(AppState {
        db,
        prompts_path,
        credentials,
        mcp_stdio: crate::mcp_stdio::StdioManager::new(),
        mcp_env: Arc::new(RwLock::new(HashMap::new())),
        official_mcp_dir,
        api_token: api_token.clone(),
        mcp_allowed: Arc::new(RwLock::new(std::collections::HashSet::new())),
        security,
    });

    // D-01 : CORS restreint aux origines de l'application ; le garde (Host,
    // Origin, jeton) est appliqué à toutes les routes, routeur imbriqué compris.
    let guard = crate::security::ApiGuard::new(api_token, port);
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(crate::security::allowed_origins()))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            HeaderName::from_static(crate::security::TOKEN_HEADER),
        ]);

    let router = Router::new()
        // Spaces
        .route("/api/spaces", get(get_spaces))
        .route("/api/spaces", put(put_spaces))
        .route("/api/spaces/reset", post(reset_spaces))
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
        .route(
            "/api/extract",
            post(extract_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/extract-multiple",
            post(extract_multiple_files).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        // Ingestion
        .route("/api/ingestion/collections", get(list_collections))
        .route("/api/ingestion/collections", post(create_collection))
        .route("/api/ingestion/collections/:id", patch(rename_collection))
        .route(
            "/api/ingestion/collections/:id",
            delete(delete_collection_handler),
        )
        .route(
            "/api/ingestion/collections/:id/documents",
            get(list_documents),
        )
        .route("/api/ingestion/documents/:id", delete(delete_document))
        .route(
            "/api/ingestion/collections/:id/meta",
            get(get_collection_meta_handler),
        )
        .route(
            "/api/ingestion/collections/:id/meta",
            put(put_collection_meta_handler),
        )
        .route(
            "/api/ingestion/collections/:id/token",
            post(generate_collection_token),
        )
        .route(
            "/api/ingestion/upload",
            post(upload_document).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        // Audio (transcription + diarization)
        .route(
            "/api/audio/transcriptions",
            post(transcribe_audio).layer(DefaultBodyLimit::max(300 * 1024 * 1024)),
        )
        // MCP
        .route("/api/mcp/tools", post(mcp_list_tools))
        .route("/api/mcp/stdio/servers", get(mcp_stdio_list_servers))
        .route("/api/mcp/stdio/stop", post(mcp_stdio_stop))
        .route(
            "/api/mcp/stdio/install-official",
            post(mcp_stdio_install_official),
        )
        .route("/api/mcp/stdio/pack-update", get(mcp_pack_check_update))
        .route("/api/mcp/stdio/pack-update", post(mcp_pack_apply_update))
        // Health
        .route("/api/image-fetch", get(image_fetch))
        .route("/health", get(health))
        // Proxy for frontend (api-proxy prefix)
        .nest("/api-proxy", build_proxy_router(state.clone()))
        .with_state(state.clone())
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            guard,
            crate::security::guard,
        ))
        .layer(cors);
    (router, state)
}

fn build_proxy_router(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    // The frontend uses /api-proxy/api/... so we re-expose the same routes under that prefix
    Router::new()
        .route("/api/spaces", get(get_spaces))
        .route("/api/spaces", put(put_spaces))
        .route("/api/spaces/reset", post(reset_spaces))
        .route("/api/chat", post(chat))
        .route("/api/generate-title", post(generate_title))
        .route("/api/rag/status", get(rag_status))
        .route("/api/conversations", get(list_conversations))
        .route("/api/conversations", post(save_conversation))
        .route("/api/conversations/:id", delete(delete_conversation))
        .route("/api/users/me", get(get_user_me))
        .route("/api/models", get(list_models))
        .route(
            "/api/extract",
            post(extract_file).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/extract-multiple",
            post(extract_multiple_files).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route("/api/ingestion/collections", get(list_collections))
        .route("/api/ingestion/collections", post(create_collection))
        .route("/api/ingestion/collections/:id", patch(rename_collection))
        .route(
            "/api/ingestion/collections/:id",
            delete(delete_collection_handler),
        )
        .route(
            "/api/ingestion/collections/:id/documents",
            get(list_documents),
        )
        .route("/api/ingestion/documents/:id", delete(delete_document))
        .route(
            "/api/ingestion/collections/:id/meta",
            get(get_collection_meta_handler),
        )
        .route(
            "/api/ingestion/collections/:id/meta",
            put(put_collection_meta_handler),
        )
        .route(
            "/api/ingestion/collections/:id/token",
            post(generate_collection_token),
        )
        .route(
            "/api/ingestion/upload",
            post(upload_document).layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/api/audio/transcriptions",
            post(transcribe_audio).layer(DefaultBodyLimit::max(300 * 1024 * 1024)),
        )
        .route("/api/mcp/tools", post(mcp_list_tools))
        .route("/api/mcp/stdio/servers", get(mcp_stdio_list_servers))
        .route("/api/mcp/stdio/stop", post(mcp_stdio_stop))
        .route(
            "/api/mcp/stdio/install-official",
            post(mcp_stdio_install_official),
        )
        .route("/api/mcp/stdio/pack-update", get(mcp_pack_check_update))
        .route("/api/mcp/stdio/pack-update", post(mcp_pack_apply_update))
        .route("/api/image-fetch", get(image_fetch))
        .route("/health", get(health))
}

// ── Image distante, à la demande ──────────────────────────────────────────
// Appelée uniquement après un clic explicite de l'utilisateur (D-05/D-06) ;
// toutes les protections SSRF sont dans `image_fetch`.

#[derive(serde::Deserialize)]
struct ImageFetchParams {
    url: String,
}

async fn image_fetch(Query(params): Query<ImageFetchParams>) -> Response {
    match crate::image_fetch::fetch_image(&params.url).await {
        Ok((bytes, mime)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "no-store"),
                (HeaderName::from_static("x-content-type-options"), "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        Err(msg) => err(StatusCode::BAD_GATEWAY, msg),
    }
}

// ── Error helper ──────────────────────────────────────────────────────────────

// ── Contrôle d'accès aux collections (D-15) ──────────────────────────────────
//
// IMPORTANT — ce que ce contrôle est, et n'est pas :
//   La visibilité (privée / partagée / publique) est une table SQLite LOCALE
//   filtrée par l'e-mail de l'utilisateur. C'est un confort d'organisation, PAS
//   une frontière de sécurité : celle-ci est l'authentification Qdrant (clé
//   d'administration, ou JWT scopés par collection — voir
//   docs/SECURITE_COLLECTIONS.md). Quiconque détient la clé d'administration
//   peut tout lire, quelles que soient les métadonnées.
//
// Règles (mode « fermé » pour les écritures) :
//   - lecture : collection avec métadonnées → `is_accessible_by` ; collection
//     SANS métadonnées (créée directement dans Qdrant, dite « externe ») →
//     lisible, Qdrant filtrant via le JWT de l'utilisateur ;
//   - écriture (ajout/suppression de documents, renommage, suppression,
//     changement de visibilité) : administrateur, ou propriétaire identifié
//     (e-mail connu) d'une collection qui a des métadonnées. Une collection
//     externe ou « orpheline » (propriétaire vide ou « unknown ») n'est donc
//     gérable que par un administrateur ;
//   - toute erreur de lecture des métadonnées REFUSE l'accès (jamais d'ouverture
//     par défaut).

/// Adresse e-mail exploitable de l'utilisateur courant (jamais « unknown »).
fn known_email(creds: &Credentials) -> Option<String> {
    creds
        .user_email
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty() && *e != "unknown")
        .map(str::to_string)
}

/// Droit de lecture sur une collection.
async fn can_read_collection(state: &AppState, user_email: &str, name: &str) -> bool {
    match state.db.get_collection_meta(name.to_string()).await {
        Ok(Some(m)) => m.is_accessible_by(user_email),
        Ok(None) => true, // collection externe : Qdrant filtre via le JWT
        Err(e) => {
            warn!("[acl] lecture des métadonnées impossible, accès refusé : {e}");
            false
        }
    }
}

/// Droit d'écriture sur une collection (voir les règles ci-dessus).
async fn can_write_collection(state: &AppState, creds: &Credentials, name: &str) -> bool {
    if is_admin(creds) {
        return true;
    }
    let Some(email) = known_email(creds) else {
        return false;
    };
    match state.db.get_collection_meta(name.to_string()).await {
        Ok(Some(m)) => !m.owner_id.is_empty() && m.owner_id != "unknown" && m.owner_id == email,
        Ok(None) => false, // collection externe : administrateur uniquement
        Err(e) => {
            warn!("[acl] lecture des métadonnées impossible, écriture refusée : {e}");
            false
        }
    }
}

/// Refuse (400) un nom de collection invalide (D-16).
fn reject_bad_collection(name: &str) -> Option<Response> {
    if crate::rag::is_valid_collection_name(name) {
        None
    } else {
        Some(err(
            StatusCode::BAD_REQUEST,
            "Nom de collection invalide (1 à 128 caractères, sans / \\ ? # %).",
        ))
    }
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    let msg = msg.into();
    // D-20 : pour une erreur serveur / amont, le détail complet reste dans les
    // journaux ; le client reçoit un message assaini (URL masquées, tronqué).
    let detail = if status.is_server_error() {
        warn!("[api] {} : {}", status.as_u16(), msg);
        public_message(&msg)
    } else {
        msg
    };
    (status, Json(json!({"detail": detail}))).into_response()
}

/// Message d'erreur présentable à l'utilisateur : première ligne, URL remplacées
/// par « [url] » (elles peuvent contenir un hôte interne ou un jeton), 240
/// caractères au plus.
fn public_message(msg: &str) -> String {
    let first = msg.lines().next().unwrap_or("").trim();
    let cleaned: Vec<String> = first
        .split_whitespace()
        .map(|w| {
            let t = w.trim_start_matches(['(', '"', '\'', '<', '[']);
            if t.starts_with("http://") || t.starts_with("https://") {
                "[url]".to_string()
            } else {
                w.to_string()
            }
        })
        .collect();
    let joined = cleaned.join(" ");
    let mut out: String = joined.chars().take(240).collect();
    if joined.chars().count() > 240 {
        out.push('…');
    }
    if out.is_empty() {
        "Erreur du service distant (voir les journaux).".to_string()
    } else {
        out
    }
}

/// Une clé d'administration utilisable pour SIGNER des jetons partagés (D-19) :
/// le jeton HS256 donne à son détenteur de quoi tenter hors ligne une attaque
/// par dictionnaire sur la clé ; elle doit donc être longue et aléatoire.
fn admin_key_is_strong(key: &str) -> bool {
    let distinct: std::collections::HashSet<char> = key.chars().collect();
    key.chars().count() >= 32 && distinct.len() >= 10
}

// ── Helpers JWT Qdrant ────────────────────────────────────────────────────────

/// Calcule la liste des collections Qdrant accessibles pour `user_email`.
/// Si l'utilisateur a un JWT (pas une clé admin), retourne None →
/// Qdrant filtre lui-même via son RBAC natif.
#[allow(dead_code)]
async fn build_accessible_collections(
    state: &Arc<AppState>,
    user_email: &str,
    creds: &Credentials,
) -> Option<Vec<QdrantCollectionAccess>> {
    // Utilisateur non-admin : son JWT est déjà scopé → Qdrant filtre nativement
    // Toutefois, si l'utilisateur a des tokens multiples, on construit la liste
    // combinée pour que le bon token soit utilisé par collection.
    if !is_admin(creds) {
        // Construire la liste depuis les tokens JWT disponibles
        let jwt_map = collect_jwt_collections(creds);
        if jwt_map.is_empty() {
            return None; // JWT global ou pas de token → laisser Qdrant filtrer
        }
        // Filtrer par les méta locales pour n'exposer que les collections accessibles
        let all_meta = match state.db.list_collection_meta().await {
            Ok(m) => m,
            Err(e) => {
                warn!("[acl] métadonnées illisibles, aucune collection exposée : {e}");
                return Some(Vec::new());
            }
        };
        let cols: Vec<QdrantCollectionAccess> = jwt_map
            .into_iter()
            .filter(|(col_name, _)| {
                // Vérifier l'accès dans les méta locales
                all_meta
                    .iter()
                    .find(|m| &m.collection_name == col_name)
                    .map(|m| m.is_accessible_by(user_email))
                    .unwrap_or(true)
            })
            .map(|(collection, _token)| {
                let access = all_meta
                    .iter()
                    .find(|m| m.collection_name == collection)
                    .map(|m| if m.owner_id == user_email { "rw" } else { "r" })
                    .unwrap_or("r");
                QdrantCollectionAccess {
                    collection,
                    access: access.to_string(),
                }
            })
            .collect();
        return if cols.is_empty() { None } else { Some(cols) };
    }

    // ── Mode admin ───────────────────────────────────────────────────────────
    let all_meta = state.db.list_collection_meta().await.unwrap_or_default();
    if all_meta.is_empty() {
        return None; // Pas encore de métadonnées → pas de filtrage
    }
    // Collections gérées : filtrer selon les droits de l'utilisateur
    let managed_cols: Vec<QdrantCollectionAccess> = all_meta
        .iter()
        .filter(|m| m.is_accessible_by(user_email))
        .map(|m| QdrantCollectionAccess {
            collection: m.collection_name.clone(),
            access: if m.owner_id == user_email {
                "rw".to_string()
            } else {
                "r".to_string()
            },
        })
        .collect();

    // Collections Qdrant sans méta → traitées comme publiques (accès lecture)
    // On les récupère pour les inclure dans le JWT granulaire
    let managed_names: std::collections::HashSet<&str> = all_meta
        .iter()
        .map(|m| m.collection_name.as_str())
        .collect();

    // Pour les collections sans méta on ne peut pas les lister ici sans appeler Qdrant,
    // donc on retourne None (clé admin brute) si toutes les collections existantes
    // ont au moins une méta → cas normal.
    // Si certaines n'ont pas de méta, on passe la clé brute pour ne pas bloquer l'accès
    // aux collections externes/importées.
    // Heuristique : si l'utilisateur est propriétaire d'au moins une collection gérée,
    // on construit le JWT ; sinon on retourne None pour ne pas restreindre.
    let has_managed = !managed_names.is_empty();
    if !has_managed {
        return None;
    }

    Some(managed_cols)
}

/// Résout la clé admin Qdrant : préférences > env.
/// Retourne None si la clé est un JWT utilisateur (commence par "eyJ") — seule
/// une clé admin brute peut signer de nouveaux JWT.
fn resolve_admin_key(creds: &Credentials) -> Option<String> {
    let key = creds
        .qdrant_api_key
        .clone()
        .filter(|k| !k.is_empty())
        .or_else(config::qdrant_api_key);
    // Un JWT utilisateur ne peut pas servir de clé de signature
    key.filter(|k| !is_jwt(k))
}

/// Résout la clé Qdrant (admin OU JWT utilisateur) pour les appels de lecture.
#[allow(dead_code)]
fn resolve_qdrant_key(creds: &Credentials) -> Option<String> {
    creds
        .qdrant_api_key
        .clone()
        .filter(|k| !k.is_empty())
        .or_else(config::qdrant_api_key)
}

/// Retourne true si l'utilisateur courant est admin Qdrant (clé brute, pas un JWT).
fn is_admin(creds: &Credentials) -> bool {
    resolve_admin_key(creds).is_some()
}

/// Construit une map { collection_name → jwt_token } à partir de tous les tokens
/// disponibles dans les credentials (qdrant_api_key global + qdrant_collection_tokens).
/// Utilisé pour savoir quelles collections un utilisateur JWT peut réellement voir.
fn collect_jwt_collections(creds: &Credentials) -> std::collections::HashMap<String, String> {
    let mut map: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    // Token global (peut être un JWT scopé à une ou plusieurs collections)
    if let Some(ref key) = creds.qdrant_api_key {
        if !key.is_empty() && is_jwt(key) {
            if let Some(cols) = decode_jwt_collections_owned(key) {
                for (col, _access) in cols {
                    map.entry(col).or_insert_with(|| key.clone());
                }
            }
            // Si access est global ("r") → pas de restriction par collection
            // On laisse la map vide → le filtre acceptera tout dans ce cas
        }
    }

    // Tokens additionnels par collection
    for ct in &creds.qdrant_collection_tokens {
        if !ct.token.is_empty() && !ct.collection.is_empty() {
            // Décoder le JWT pour vérifier qu'il couvre bien la collection déclarée
            let covers = decode_jwt_collections_owned(&ct.token)
                .map(|cols| cols.iter().any(|(c, _)| c == &ct.collection))
                .unwrap_or(true); // si non décodable, faire confiance à la déclaration
            if covers {
                map.entry(ct.collection.clone())
                    .or_insert_with(|| ct.token.clone());
            }
        }
    }

    map
}

/// Résout le meilleur token Qdrant disponible pour accéder à une collection précise.
/// Priorité : token spécifique pour cette collection > token global.
pub fn resolve_key_for_collection<'a>(creds: &'a Credentials, collection: &str) -> Option<&'a str> {
    // 1. Token dédié à cette collection
    if let Some(ct) = creds
        .qdrant_collection_tokens
        .iter()
        .find(|ct| ct.collection == collection)
    {
        if !ct.token.is_empty() {
            return Some(&ct.token);
        }
    }
    // 2. Clé globale (admin ou JWT)
    creds.qdrant_api_key.as_deref().filter(|k| !k.is_empty())
}

/// Décode les collections couvertes par un JWT Qdrant (sans vérification de signature).
/// Supporte le format natif Qdrant :
///   { "access": { "collections": { "col": { "r": true, "w": false } } } }
fn decode_jwt_collections_owned(token: &str) -> Option<Vec<(String, String)>> {
    let parts: Vec<&str> = token.splitn(3, '.').collect();
    if parts.len() < 2 {
        return None;
    }
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let decoded = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    let payload: serde_json::Value = serde_json::from_slice(&decoded).ok()?;

    let access = payload.get("access")?;

    // Format Qdrant natif : { "collections": { "col": { "r": true, "w": false } } }
    if let Some(cols_obj) = access.get("collections").and_then(|c| c.as_object()) {
        let cols: Vec<(String, String)> = cols_obj
            .iter()
            .map(|(col, rights)| {
                let w = rights.get("w").and_then(|v| v.as_bool()).unwrap_or(false);
                let access_str = if w { "rw" } else { "r" };
                (col.clone(), access_str.to_string())
            })
            .collect();
        return if cols.is_empty() { None } else { Some(cols) };
    }

    None
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "Demeter API", "rag_backend": "qdrant"}))
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

async fn reset_spaces(State(state): State<Arc<AppState>>) -> Response {
    let default_yml = include_str!("../prompts.yml");
    match std::fs::write(&state.prompts_path, default_yml) {
        Ok(()) => Json(json!({"ok": true})).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── RAG status ────────────────────────────────────────────────────────────────

async fn rag_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    let creds = state.credentials.read().await.clone();
    let resolved_url = creds
        .qdrant_url
        .clone()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(config::qdrant_url);
    let admin_key = resolve_admin_key(&creds);
    let key_set = admin_key.is_some();
    let qdrant_ok = {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        let url = format!("{}/healthz", resolved_url.trim_end_matches('/'));
        let mut req = client.get(&url);
        // Pour healthz, un JWT admin sans restriction de collection
        if let Some(ref ak) = admin_key {
            match crate::rag::generate_admin_jwt(ak) {
                Ok(token) => req = req.header("api-key", token),
                Err(_) => req = req.header("api-key", ak.clone()),
            }
        }
        req.send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    };
    Json(json!({
        "backend":         "qdrant",
        "qdrant_url":      resolved_url,
        "qdrant_api_key":  key_set,
        "qdrant_ok":       qdrant_ok,
        "embedding_model": config::embedding_model(),
        "embedding_dim":   config::embedding_dim(),
        "rerank_model":    config::rerank_model(),
        "rag_top_k":       config::rag_top_k().to_string(),
        "rag_top_rerank":  config::rag_top_rerank().to_string(),
        "rag_min_score":   config::rag_min_score().to_string(),
        "info": "Les collections sont des collections Qdrant locales.",
    }))
}

// ── Chat ──────────────────────────────────────────────────────────────────────

async fn chat(State(state): State<Arc<AppState>>, Json(mut req): Json<ChatRequest>) -> Response {
    // D-03 : seuls les serveurs MCP de la configuration enregistrée sont utilisables.
    req.mcp_servers = filter_allowed_mcp(&state, std::mem::take(&mut req.mcp_servers)).await;
    let creds = state.credentials.read().await.clone();
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
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        });

    // RAG context — résolution de clé Qdrant selon le type d'utilisateur
    let mut rag_chunks_used: Vec<crate::models::Chunk> = Vec::new();
    let user_email_for_rag = creds.user_email.clone().unwrap_or_default();
    let admin_key_for_rag = resolve_admin_key(&creds);

    // Avertissement si utilisateur JWT sans URL Qdrant configurée
    if !is_admin(&creds)
        && creds
            .qdrant_url
            .as_deref()
            .filter(|u| !u.is_empty())
            .is_none()
    {
        warn!(
            "[RAG] Utilisateur JWT sans qdrant_url configurée —              les requêtes RAG vont vers localhost:6333 (fallback) et échoueront probablement.              Configurez l'URL Qdrant dans les paramètres."
        );
    }

    let rag_context = if let Some(ref query) = last_user {
        if let Some(ref col_name) = req.collection_id {
            // ── Recherche dans une collection explicite ───────────────────────
            // Pour un admin : vérification SQLite.
            // Pour un utilisateur JWT : si un token couvre cette collection,
            // on lui fait confiance directement — c'est Qdrant qui arbitre.
            // La vérification SQLite est un double-filet, pas la source de vérité.
            let jwt_key_for_col = if !is_admin(&creds) {
                resolve_key_for_collection(&creds, col_name)
            } else {
                None
            };
            let has_jwt_access = jwt_key_for_col.is_some();

            let accessible = has_jwt_access || {
                can_read_collection(&state, &user_email_for_rag, col_name).await
            };

            if !accessible {
                warn!(
                    "RAG: accès refusé à la collection '{}' pour '{}'",
                    col_name, user_email_for_rag
                );
                None
            } else if is_admin(&creds) {
                // Admin : clé brute passée directement.
                let chunks = search_chunks(
                    query,
                    col_name,
                    &creds.endpoint,
                    &creds.bearer,
                    admin_key_for_rag.as_deref(),
                    None,
                    creds.qdrant_url.as_deref(),
                )
                .await;
                let chunks = rerank_chunks(query, chunks, &creds.endpoint, &creds.bearer).await;
                if !chunks.is_empty() {
                    let ctx = chunks_to_context(&chunks);
                    rag_chunks_used = chunks;
                    Some(ctx)
                } else {
                    None
                }
            } else {
                // Utilisateur JWT : token scopé à la collection.
                let chunks = search_chunks(
                    query,
                    col_name,
                    &creds.endpoint,
                    &creds.bearer,
                    jwt_key_for_col,
                    None,
                    creds.qdrant_url.as_deref(),
                )
                .await;
                let chunks = rerank_chunks(query, chunks, &creds.endpoint, &creds.bearer).await;
                if !chunks.is_empty() {
                    let ctx = chunks_to_context(&chunks);
                    rag_chunks_used = chunks;
                    Some(ctx)
                } else {
                    None
                }
            }
        } else if let Some(ref sid) = req.space_id {
            // ── Recherche dans la collection d'un espace ──────────────────────
            let jwt_key_for_col = if !is_admin(&creds) {
                resolve_key_for_collection(&creds, sid)
            } else {
                None
            };
            let has_jwt_access = jwt_key_for_col.is_some();

            let accessible =
                has_jwt_access || { can_read_collection(&state, &user_email_for_rag, sid).await };

            if !accessible {
                warn!(
                    "RAG: accès refusé à la collection d'espace '{}' pour '{}'",
                    sid, user_email_for_rag
                );
                None
            } else if is_admin(&creds) {
                match retrieve_context_with_chunks(
                    query,
                    sid,
                    &creds.endpoint,
                    &creds.bearer,
                    admin_key_for_rag.as_deref(),
                    None,
                    creds.qdrant_url.as_deref(),
                )
                .await
                {
                    Some((ctx, chunks)) => {
                        rag_chunks_used = chunks;
                        Some(ctx)
                    }
                    None => None,
                }
            } else {
                let chunks = search_chunks(
                    query,
                    sid,
                    &creds.endpoint,
                    &creds.bearer,
                    jwt_key_for_col,
                    None,
                    creds.qdrant_url.as_deref(),
                )
                .await;
                let chunks = rerank_chunks(query, chunks, &creds.endpoint, &creds.bearer).await;
                if !chunks.is_empty() {
                    let ctx = chunks_to_context(&chunks);
                    rag_chunks_used = chunks;
                    Some(ctx)
                } else {
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let mut final_system = build_system_with_rag(&system_prompt, rag_context.as_deref());

    // Build LLM endpoint
    let mut endpoint = creds.endpoint.trim_end_matches('/').to_string();
    if !endpoint.ends_with("/chat/completions") {
        endpoint.push_str("/chat/completions");
    }

    let mut messages_payload: Vec<Value> = vec![json!({"role": "system", "content": final_system})];
    for m in &req.messages {
        messages_payload.push(json!({"role": m.role, "content": m.content}));
    }

    let client = match Client::builder()
        .timeout(config::timeout_llm_stream())
        .redirect(crate::mcp::redirect_policy())
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
        let all = collect_all_tools_unified(&client, &state, &req.mcp_servers).await;
        all.iter()
            .map(|(server, tool)| crate::mcp::mcp_tool_to_openai(tool, server))
            .collect()
    } else {
        vec![]
    };

    // Ajouter l'instruction MCP image si des serveurs MCP sont configurés
    if !req.mcp_servers.is_empty() {
        final_system.push_str(MCP_IMAGE_INSTRUCTION);
    }

    // Si on a des tools MCP, on fait d'abord un appel non-streamé pour gérer les tool_calls
    if !mcp_tools.is_empty() {
        let mut loop_messages = messages_payload.clone();
        let max_turns = config::mcp_max_turns();
        let sync_client = Client::builder()
            .timeout(config::timeout_llm_sync())
            .use_rustls_tls()
            .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
            .build()
            .unwrap_or_default();

        let mut pending_sse_events: Vec<bytes::Bytes> = Vec::new();
        for turn in 0..max_turns {
            let loop_payload = json!({
                "model":      req.model,
                "messages":   loop_messages,
                "stream":     false,
                "max_tokens": config::llm_max_tokens(),
                "tools":      mcp_tools,
                "tool_choice": "auto",
            });

            let resp = match sync_client
                .post(&endpoint)
                .bearer_auth(&creds.bearer)
                .json(&loop_payload)
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

            let data: Value = match resp.json().await {
                Ok(v) => v,
                Err(e) => return err(StatusCode::BAD_GATEWAY, e.to_string()),
            };

            let choice = &data["choices"][0];
            let finish_reason = choice["finish_reason"].as_str().unwrap_or("");
            let msg = &choice["message"];

            // Pas de tool_calls → réponse finale
            // On émet le texte chunk par chunk via un vrai stream SSE (pas en un seul bloc)
            if finish_reason != "tool_calls"
                || msg["tool_calls"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true)
            {
                info!("MCP: réponse finale après {} tour(s)", turn + 1);
                let content = msg["content"].as_str().unwrap_or("").to_string();

                // Construire la liste ordonnée de chunks SSE à émettre
                let mut sse_chunks: Vec<Bytes> = Vec::new();

                // Injecter les sources RAG en premier si disponibles
                if !rag_chunks_used.is_empty() {
                    let sources: Vec<serde_json::Value> = rag_chunks_used
                        .iter()
                        .enumerate()
                        .map(|(i, c)| {
                            json!({
                                "index":        i + 1,
                                "source":       c.source,
                                "page":         c.page,
                                "score":        c.score,
                                "rerank_score": c.rerank_score,
                            })
                        })
                        .collect();
                    let ev = format!(
                        "event: rag_sources\ndata: {}\n\n",
                        serde_json::to_string(&json!({ "sources": sources })).unwrap_or_default()
                    );
                    sse_chunks.push(Bytes::from(ev));
                }

                // Émettre d'abord les images MCP interceptées pendant les tours précédents
                for ev in pending_sse_events.drain(..) {
                    sse_chunks.push(ev);
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
                let stream = futures::stream::iter(sse_chunks.into_iter().enumerate()).then(
                    |(i, chunk)| async move {
                        if i > 0 {
                            tokio::time::sleep(tokio::time::Duration::from_millis(8)).await;
                        }
                        Ok::<Bytes, std::io::Error>(chunk)
                    },
                );

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
                    let tc_id = tc["id"].as_str().unwrap_or("").to_string();
                    let fn_name = tc["function"]["name"].as_str().unwrap_or("").to_string();
                    let fn_args: Value =
                        serde_json::from_str(tc["function"]["arguments"].as_str().unwrap_or("{}"))
                            .unwrap_or(json!({}));

                    // Retrouver le serveur MCP à partir du préfixe du nom du tool
                    // Format : mcp__{slug}__{tool_name}
                    let parts: Vec<&str> = fn_name.splitn(3, "__").collect();
                    let real_tool_name = if parts.len() == 3 { parts[2] } else { &fn_name };
                    // D-14 : routage EXACT (préfixe « mcp__{slug}__ »). Un nom d'outil qui
                    // ne correspond à aucun serveur n'est plus envoyé au premier serveur.
                    let server = match req.mcp_servers.iter().find(|s| {
                        fn_name.starts_with(&format!("mcp__{}__", crate::mcp::server_slug(s)))
                    }) {
                        Some(s) => s.clone(),
                        None => {
                            warn!(
                                "MCP: outil '{}' sans serveur correspondant, appel refusé",
                                fn_name
                            );
                            loop_messages.push(json!({
                                "role": "tool",
                                "tool_call_id": tc_id,
                                "content": "Erreur : outil inconnu, aucun serveur MCP correspondant.",
                            }));
                            continue;
                        }
                    };

                    info!("MCP: appel tool {} sur {}", real_tool_name, server);
                    let mcp_result = if let Some(id) = stdio_id(&server) {
                        let env = mcp_env_for(&state, id).await;
                        state
                            .mcp_stdio
                            .call_tool(id, real_tool_name, &fn_args, &env)
                            .await
                    } else {
                        crate::mcp::call_tool(&client, &server, real_tool_name, &fn_args).await
                    };

                    // Si le tool a retourné une image, l'émettre immédiatement via SSE
                    // avant de repasser au LLM (qui ne reçoit que le texte).
                    if let (Some(b64), Some(mime)) = (&mcp_result.image_b64, &mcp_result.image_mime)
                    {
                        let img_event = format!(
                            "event: mcp_image\ndata: {}\n\n",
                            serde_json::to_string(&serde_json::json!({
                                "b64":  b64,
                                "mime": mime,
                                "alt":  &mcp_result.text,
                            }))
                            .unwrap_or_default()
                        );
                        pending_sse_events.push(bytes::Bytes::from(img_event));
                    }

                    loop_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tc_id,
                        // On ne passe au LLM que le texte (pas le base64)
                        "content": &mcp_result.text,
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
    let mut payload = json!({
        "model":      req.model,
        "messages":   messages_payload,
        "stream":     req.stream,
        "max_tokens": config::llm_max_tokens(),
    });
    // Demander à Albert d'inclure les stats d'usage (tokens + impacts CO₂) dans
    // le dernier chunk SSE. Sans stream_options, l'API Albert ne renvoie pas de
    // bloc usage en mode streaming → le CO₂ n'était plus affiché.
    if req.stream {
        payload["stream_options"] = json!({ "include_usage": true });
    }

    if req.stream {
        // Streaming SSE passthrough
        let resp = match client
            .post(&endpoint)
            .bearer_auth(&creds.bearer)
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
            let sources: Vec<serde_json::Value> = rag_chunks_used
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    json!({
                        "index":        i + 1,
                        "source":       c.source,
                        "page":         c.page,
                        "score":        c.score,
                        "rerank_score": c.rerank_score,
                    })
                })
                .collect();
            let event = format!(
                "event: rag_sources\ndata: {}\n\n",
                serde_json::to_string(&json!({ "sources": sources })).unwrap_or_default()
            );
            bytes::Bytes::from(event)
        } else {
            bytes::Bytes::new()
        };

        let llm_stream = resp
            .bytes_stream()
            .map(|result| result.map_err(std::io::Error::other));

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
            .bearer_auth(&creds.bearer)
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                let body: Value = resp.json().await.unwrap_or(json!({}));
                (
                    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                    Json(body),
                )
                    .into_response()
            }
            Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
        }
    }
}

// ── MCP ───────────────────────────────────────────────────────────────────────
//
// Un serveur MCP est référencé par une simple chaîne dans les préférences :
//   - une URL ("https://...", "http://...")           → transport HTTP
//   - "stdio:<id>" où <id> est l'identifiant du        → transport stdio
//     manifest.json trouvé dans le répertoire dédié      (process local)
//
// Ce préfixe est la seule différence de traitement ; le reste de la boucle
// agentique (formatage OpenAI des tools, exécution, extraction du résultat)
// est identique pour les deux transports.

const STDIO_PREFIX: &str = "stdio:";

fn stdio_id(server: &str) -> Option<&str> {
    server.strip_prefix(STDIO_PREFIX)
}

/// Récupère les variables d'environnement configurées par l'utilisateur pour
/// un serveur MCP stdio donné (panel MCP → `init_credentials`).
async fn mcp_env_for(state: &Arc<AppState>, id: &str) -> HashMap<String, String> {
    state
        .mcp_env
        .read()
        .await
        .get(id)
        .cloned()
        .unwrap_or_default()
}

/// Collecte les tools de tous les serveurs MCP configurés, HTTP et stdio
/// confondus, en parallèle.
async fn collect_all_tools_unified(
    client: &Client,
    state: &Arc<AppState>,
    servers: &[String],
) -> Vec<(String, Value)> {
    let futures: Vec<_> = servers
        .iter()
        .map(|server| {
            let client = client.clone();
            let server = server.clone();
            let state = state.clone();
            async move {
                let tools = if let Some(id) = stdio_id(&server) {
                    let env = mcp_env_for(&state, id).await;
                    state.mcp_stdio.list_tools(id, &env).await.0
                } else {
                    crate::mcp::list_tools(&client, &server).await.0
                };
                tools
                    .into_iter()
                    .map(move |t| (server.clone(), t))
                    .collect::<Vec<_>>()
            }
        })
        .collect();
    futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}

#[derive(serde::Deserialize)]
struct McpListRequest {
    servers: Vec<String>,
}

async fn mcp_list_tools(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpListRequest>,
) -> Response {
    let servers = filter_allowed_mcp(&state, req.servers).await;
    let client = Client::builder()
        .timeout(config::timeout_models())
        .redirect(crate::mcp::redirect_policy())
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    // Appels parallèles — plus d'attente séquentielle
    let futures: Vec<_> = servers
        .iter()
        .map(|server| {
            let client = client.clone();
            let server = server.clone();
            let state = state.clone();
            async move {
                let (tools, ok) = if let Some(id) = stdio_id(&server) {
                    let env = mcp_env_for(&state, id).await;
                    state.mcp_stdio.list_tools(id, &env).await
                } else {
                    crate::mcp::list_tools(&client, &server).await
                };
                let count = tools.len();
                json!({
                    "server": server,
                    "status": if ok { "ok" } else { "error" },
                    "tools": tools,
                    "tool_count": count,
                })
            }
        })
        .collect();

    let result = futures::future::join_all(futures).await;
    Json(json!({ "servers": result })).into_response()
}

/// Liste les serveurs MCP stdio disponibles dans le répertoire actuellement
/// configuré (officiels + ajoutés manuellement), avec leur disponibilité sur
/// la plateforme courante et leurs paramètres de configuration attendus.
/// Utilisé par le panel MCP dédié pour construire la liste et les
/// formulaires de configuration.
async fn mcp_stdio_list_servers(State(state): State<Arc<AppState>>) -> Response {
    let servers = state.mcp_stdio.discover().await;
    let dir = state.mcp_stdio.dir().await;
    Json(json!({
        "dir": dir,
        "servers": servers,
    }))
    .into_response()
}

#[derive(serde::Deserialize)]
struct McpStdioStopRequest {
    id: String,
}

/// Arrête un serveur MCP stdio en cours d'exécution (ex : désactivé ou
/// reconfiguré dans le panel). Sans effet s'il n'était pas démarré.
async fn mcp_stdio_stop(
    State(state): State<Arc<AppState>>,
    Json(req): Json<McpStdioStopRequest>,
) -> Response {
    state.mcp_stdio.stop(&req.id).await;
    Json(json!({ "stopped": req.id })).into_response()
}

/// Copie les MCP officiels de Demeter dans le dossier actuellement configuré
/// par l'utilisateur (sans écraser un MCP déjà présent). Échoue si aucun
/// dossier n'est configuré.
async fn mcp_stdio_install_official(State(state): State<Arc<AppState>>) -> Response {
    let Some(resource_dir) = state.official_mcp_dir.clone() else {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(json!({ "error": "Aucun MCP officiel embarqué dans cette build de Demeter." })),
        )
            .into_response();
    };

    match state.mcp_stdio.install_official(&resource_dir).await {
        Ok(installed) => Json(json!({ "installed": installed })).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

/// Compare la version du pack MCP installée à la dernière release publiée
/// sur GitHub. N'échoue jamais côté réseau : `latest` reste simplement à
/// `null` si l'appel GitHub échoue (pas de connexion, dépôt privé, etc.).
async fn mcp_pack_check_update(State(state): State<Arc<AppState>>) -> Response {
    let installed = state.mcp_stdio.installed_pack_version().await;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();
    let status = crate::mcp_pack::check_update(&client, installed).await;
    Json(status).into_response()
}

/// Télécharge, vérifie (SHA-256) et installe la dernière release du pack MCP
/// dans le dossier actuellement configuré — sans jamais écraser la
/// configuration déjà saisie par l'utilisateur pour un serveur existant.
async fn mcp_pack_apply_update(State(state): State<Arc<AppState>>) -> Response {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match crate::mcp_pack::download_and_install(&client, &state.mcp_stdio).await {
        Ok(installed) => Json(json!({ "installed": installed })).into_response(),
        Err(e) => (
            axum::http::StatusCode::BAD_REQUEST,
            Json(json!({ "error": e })),
        )
            .into_response(),
    }
}

// ── Generate title ────────────────────────────────────────────────────────────

async fn generate_title(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TitleRequest>,
) -> Response {
    let creds = state.credentials.read().await.clone();
    let mut endpoint = creds.endpoint.trim_end_matches('/').to_string();
    if !endpoint.ends_with("/chat/completions") {
        endpoint.push_str("/chat/completions");
    }

    let payload = json!({
        "model":  req.model,
        "stream": false,
        "max_tokens": config::title_max_tokens(),
        "messages": [
            {
                "role": "system",
                "content": "Tu génères des titres de conversation ultra-courts (4 mots max, sans ponctuation finale, sans guillemets). Réponds uniquement avec le titre, rien d'autre.",
            },
            {
                "role": "user",
                "content": format!(
                    "Question : {}\nRéponse : {}\n\nGénère un titre court.",
                    truncate_utf8(&req.first_user, config::title_truncate_user()),
                    truncate_utf8(&req.first_assistant, config::title_truncate_assistant()),
                ),
            },
        ],
    });

    let client = Client::builder()
        .timeout(config::timeout_title())
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .post(&endpoint)
        .bearer_auth(&creds.bearer)
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
            Json(
                json!({"title": if title.is_empty() { "Conversation".to_string() } else { title }}),
            )
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

// ── Query params sans bearer ─────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
struct EndpointParams {
    pub endpoint: String,
}

/// Retourne (base Albert, bearer) à partir des credentials ENREGISTRÉS par la
/// commande IPC `init_credentials` (D-02). Le paramètre `endpoint` de la
/// requête HTTP n'est volontairement plus utilisé pour choisir la destination :
/// il permettait d'envoyer le jeton Albert à une URL arbitraire.
fn albert_target(creds: &Credentials) -> Result<(String, String), &'static str> {
    if creds.endpoint.trim().is_empty() || creds.bearer.trim().is_empty() {
        return Err("Endpoint ou jeton Albert non configuré.");
    }
    Ok((albert_base(&creds.endpoint), creds.bearer.clone()))
}

/// Ne conserve que les serveurs MCP présents dans la configuration enregistrée
/// (D-03). Les serveurs inconnus reçus en HTTP sont ignorés.
async fn filter_allowed_mcp(state: &AppState, requested: Vec<String>) -> Vec<String> {
    let allowed = state.mcp_allowed.read().await;
    let total = requested.len();
    let kept: Vec<String> = requested
        .into_iter()
        .filter(|s| allowed.contains(s.trim()))
        .collect();
    if kept.len() != total {
        warn!(
            "[mcp] {} serveur(s) refusé(s) : absents de la configuration enregistrée",
            total - kept.len()
        );
    }
    kept
}

// ── User info ─────────────────────────────────────────────────────────────────

async fn get_user_me(State(state): State<Arc<AppState>>) -> Response {
    let creds = state.credentials.read().await.clone();
    let (base, bearer) = match albert_target(&creds) {
        Ok(t) => t,
        Err(m) => return err(StatusCode::BAD_REQUEST, m),
    };
    let client = Client::builder()
        .timeout(config::timeout_doc_resolve())
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/me/info", base))
        .bearer_auth(&bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({}));
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                Json(body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

// ── Models ────────────────────────────────────────────────────────────────────

async fn list_models(State(state): State<Arc<AppState>>) -> Response {
    let creds = state.credentials.read().await.clone();
    let (base, bearer) = match albert_target(&creds) {
        Ok(t) => t,
        Err(m) => return err(StatusCode::BAD_REQUEST, m),
    };
    let client = Client::builder()
        .timeout(config::timeout_doc_resolve())
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    match client
        .get(format!("{}/models", base))
        .bearer_auth(&bearer)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            let body: Value = resp.json().await.unwrap_or(json!({"data": []}));
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                Json(body),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e.to_string()),
    }
}

// ── Extract ───────────────────────────────────────────────────────────────────

async fn extract_file(mut multipart: Multipart) -> Response {
    if let Ok(Some(field)) = multipart.next_field().await {
        let filename = field.file_name().unwrap_or("file").to_string();
        let ext = filename.rsplit('.').next().unwrap_or("").to_lowercase();
        let bytes = match field.bytes().await {
            Ok(b) => b,
            Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string()),
        };

        return match extract_text_async(bytes.to_vec(), ext.clone()).await {
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

        match extract_text_async(bytes.to_vec(), ext.clone()).await {
            Ok(text) => {
                let chars = text.chars().count();
                results
                    .push(json!({"filename": filename, "text": text, "chars": chars, "ext": ext}));
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

// ── Ingestion : collections Qdrant ────────────────────────────────────────────

async fn list_collections(
    State(state): State<Arc<AppState>>,
    Query(_params): Query<EndpointParams>,
) -> Response {
    let creds_snap = state.credentials.read().await.clone();
    let user_email = creds_snap.user_email.clone().unwrap_or_default();
    let url = creds_snap.qdrant_url.clone();

    // ── Stratégie selon le type de clé ───────────────────────────────────────
    // Admin (clé brute) : interroger Qdrant pour la liste exhaustive, puis filtrer
    //   par les métadonnées de visibilité locales.
    // JWT utilisateur   : Qdrant refuse GET /collections sans claim global "r".
    //   On reconstruit la liste uniquement depuis les méta SQLite pour les collections
    //   auxquelles cet utilisateur a accès, sans aucun appel réseau Qdrant.

    let all_meta = match state.db.list_collection_meta().await {
        Ok(m) => m,
        Err(e) => {
            warn!("[acl] métadonnées illisibles : {e}");
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Métadonnées de collection indisponibles.",
            );
        }
    };

    if is_admin(&creds_snap) {
        // ── Mode admin : on interroge Qdrant ────────────────────────────────
        let admin_key = resolve_admin_key(&creds_snap);
        let qdrant_cols = match rag::list_collections(admin_key.as_deref(), url.as_deref()).await {
            Ok(c) => c,
            Err(e) => return err(StatusCode::BAD_GATEWAY, e),
        };
        let meta_map: std::collections::HashMap<String, &crate::models::CollectionMeta> = all_meta
            .iter()
            .map(|m| (m.collection_name.clone(), m))
            .collect();

        let data: Vec<Value> = qdrant_cols
            .iter()
            .filter_map(|c| {
                let name = c["name"].as_str().unwrap_or("").to_string();
                let meta = meta_map.get(&name);
                // Filtre d'accès : si pas de méta → public (collection externe)
                let accessible = meta
                    .map(|m| m.is_accessible_by(&user_email))
                    .unwrap_or(true);
                if !accessible {
                    return None;
                }
                let visibility = meta.map(|m| m.visibility.as_str()).unwrap_or("public");
                let owner = meta.map(|m| m.owner_id.as_str()).unwrap_or("");
                let is_owner = owner == user_email || owner.is_empty();
                let shared_with: Vec<&str> = meta
                    .map(|m| m.shared_with.iter().map(String::as_str).collect())
                    .unwrap_or_default();
                Some(json!({
                    "id":          name.clone(),
                    "name":        name,
                    "visibility":  visibility,
                    "owner":       owner,
                    "is_owner":    is_owner,
                    "shared_with": shared_with,
                }))
            })
            .collect();

        Json(json!({ "data": data })).into_response()
    } else {
        // ── Mode JWT utilisateur : liste depuis les méta SQLite uniquement ──
        // Un JWT scopé à une ou plusieurs collections ne peut pas appeler
        // GET /collections sur Qdrant (401). On reconstruit la liste à partir
        // des métadonnées locales filtrées par les droits de l'utilisateur,
        // CROISÉES avec les collections présentes dans le(s) token(s) JWT.

        // Extraire les collections autorisées par les tokens JWT de l'utilisateur
        let jwt_collections = collect_jwt_collections(&creds_snap);

        let data: Vec<Value> = all_meta
            .iter()
            .filter(|m| m.is_accessible_by(&user_email))
            .filter(|m| {
                // Si on a des JWT scopés, n'afficher que les collections couvertes
                if jwt_collections.is_empty() {
                    true
                } else {
                    jwt_collections.contains_key(m.collection_name.as_str())
                }
            })
            .map(|m| {
                let is_owner = m.owner_id == user_email;
                json!({
                    "id":          m.collection_name.clone(),
                    "name":        m.collection_name.clone(),
                    "visibility":  m.visibility.as_str(),
                    "owner":       m.owner_id.as_str(),
                    "is_owner":    is_owner,
                    "shared_with": m.shared_with,
                })
            })
            .collect();

        Json(json!({ "data": data })).into_response()
    }
}

async fn create_collection(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CollectionCreateRequest>,
) -> Response {
    if let Some(r) = reject_bad_collection(&req.name) {
        return r;
    }
    let creds_snap = state.credentials.read().await.clone();
    let admin_key = resolve_admin_key(&creds_snap);
    let url = creds_snap.qdrant_url.clone();
    // D-15 : plus de propriétaire « unknown ». Sans e-mail connu, seul un
    // administrateur peut créer une collection (propriétaire « admin »).
    let owner = match known_email(&creds_snap) {
        Some(e) => e,
        None if is_admin(&creds_snap) => "admin".to_string(),
        None => {
            return err(
                StatusCode::BAD_REQUEST,
                "Identité utilisateur inconnue : impossible de créer une collection.",
            )
        }
    };
    info!(
        "[create_collection] Création Qdrant collection '{}'",
        req.name
    );
    if qdrant_collection_exists(&req.name, admin_key.as_deref(), url.as_deref()).await {
        return err(
            StatusCode::CONFLICT,
            format!("La collection '{}' existe déjà.", req.name),
        );
    }
    match rag::create_collection(&req.name, admin_key.as_deref(), url.as_deref()).await {
        Ok(()) => {
            // Enregistrer les métadonnées de visibilité
            let vis = match req.visibility.as_str() {
                "public" => crate::models::CollectionVisibility::Public,
                "shared" => crate::models::CollectionVisibility::Shared,
                _ => crate::models::CollectionVisibility::Private,
            };
            let meta = crate::models::CollectionMeta {
                collection_name: req.name.clone(),
                owner_id: owner,
                visibility: vis,
                shared_with: vec![],
                created_at: chrono::Utc::now().to_rfc3339(),
            };
            if let Err(e) = state.db.upsert_collection_meta(meta).await {
                warn!("[create_collection] meta save failed: {}", e);
            }
            Json(json!({ "id": req.name.clone(), "name": req.name })).into_response()
        }
        Err(e) => {
            error!("[create_collection] Erreur : {}", e);
            err(StatusCode::BAD_GATEWAY, e)
        }
    }
}

async fn rename_collection(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
    Json(req): Json<CollectionRenameRequest>,
) -> Response {
    if let Some(r) = reject_bad_collection(&collection_name) {
        return r;
    }
    if let Some(r) = reject_bad_collection(&req.name) {
        return r;
    }
    let creds_snap = state.credentials.read().await.clone();
    let admin_key = resolve_admin_key(&creds_snap);
    let url = creds_snap.qdrant_url.clone();

    // Vérifier que l'utilisateur est propriétaire de la collection
    let is_owner = can_write_collection(&state, &creds_snap, &collection_name).await;
    if !is_owner {
        return err(
            StatusCode::FORBIDDEN,
            "Seul le propriétaire peut renommer cette collection.",
        );
    }

    match rag::rename_collection(
        &collection_name,
        &req.name,
        admin_key.as_deref(),
        url.as_deref(),
    )
    .await
    {
        Ok(()) => {
            // Mettre à jour le nom dans les métadonnées
            if let Ok(Some(mut meta)) = state.db.get_collection_meta(collection_name.clone()).await
            {
                meta.collection_name = req.name.clone();
                let _ = state.db.delete_collection_meta(collection_name).await;
                let _ = state.db.upsert_collection_meta(meta).await;
            }
            Json(json!({ "ok": true })).into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e),
    }
}

async fn delete_collection_handler(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
    Query(_params): Query<EndpointParams>,
) -> Response {
    if let Some(r) = reject_bad_collection(&collection_name) {
        return r;
    }
    let creds_snap = state.credentials.read().await.clone();
    let admin_key = resolve_admin_key(&creds_snap);
    let url = creds_snap.qdrant_url.clone();

    // Vérifier que l'utilisateur est propriétaire de la collection
    let is_owner = can_write_collection(&state, &creds_snap, &collection_name).await;
    if !is_owner {
        return err(
            StatusCode::FORBIDDEN,
            "Seul le propriétaire peut supprimer cette collection.",
        );
    }

    match rag::delete_collection(&collection_name, admin_key.as_deref(), url.as_deref()).await {
        Ok(()) => {
            let _ = state.db.delete_collection_meta(collection_name).await;
            Json(json!({ "ok": true })).into_response()
        }
        Err(e) => err(StatusCode::BAD_GATEWAY, e),
    }
}

// ── Collection meta (visibilité & partage) ────────────────────────────────────

async fn get_collection_meta_handler(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
) -> Response {
    let user_email = state
        .credentials
        .read()
        .await
        .user_email
        .clone()
        .unwrap_or_default();
    match state.db.get_collection_meta(collection_name.clone()).await {
        Ok(Some(meta)) => Json(json!({
            "collection_name": meta.collection_name,
            "owner_id":        meta.owner_id,
            "is_owner":        meta.owner_id == user_email,
            "visibility":      meta.visibility.as_str(),
            "shared_with":     meta.shared_with,
        }))
        .into_response(),
        Ok(None) => {
            // Collection sans métadonnées (importée de l'extérieur) → traiter comme publique
            Json(json!({
                "collection_name": collection_name,
                "owner_id":        "",
                "is_owner":        false,
                "visibility":      "public",
                "shared_with":     [],
            }))
            .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn put_collection_meta_handler(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
    Json(req): Json<CollectionMetaUpdateRequest>,
) -> Response {
    if let Some(r) = reject_bad_collection(&collection_name) {
        return r;
    }
    let creds_snap = state.credentials.read().await.clone();
    let admin = is_admin(&creds_snap);
    let user_email = match known_email(&creds_snap) {
        Some(e) => e,
        None if admin => "admin".to_string(),
        None => {
            return err(
                StatusCode::FORBIDDEN,
                "Identité utilisateur inconnue : modification refusée.",
            )
        }
    };

    // Récupérer les méta existantes (une erreur de lecture refuse la modification)
    let existing = match state.db.get_collection_meta(collection_name.clone()).await {
        Ok(m) => m,
        Err(e) => {
            warn!("[acl] lecture des métadonnées impossible : {e}");
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "Métadonnées de collection indisponibles.",
            );
        }
    };

    let meta = match existing {
        Some(mut m) => {
            // Propriétaire identifié, ou administrateur (qui « adopte » alors une
            // collection orpheline : propriétaire vide ou « unknown »).
            let orphan = m.owner_id.is_empty() || m.owner_id == "unknown";
            if m.owner_id != user_email && !admin {
                return err(
                    StatusCode::FORBIDDEN,
                    "Seul le propriétaire peut modifier la visibilité.",
                );
            }
            if admin && orphan {
                m.owner_id = user_email;
            }
            m.visibility = req.visibility;
            m.shared_with = req.shared_with;
            m
        }
        None => {
            // Collection externe (sans métadonnées) : l'administrateur seul peut
            // l'« adopter » en lui attribuant un propriétaire et une visibilité.
            if !admin {
                return err(
                    StatusCode::FORBIDDEN,
                    "Collection externe : seul un administrateur peut en définir la visibilité.",
                );
            }
            crate::models::CollectionMeta {
                collection_name: collection_name.clone(),
                owner_id: user_email,
                visibility: req.visibility,
                shared_with: req.shared_with,
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        }
    };

    match state.db.upsert_collection_meta(meta.clone()).await {
        Ok(()) => Json(json!({
            "ok":         true,
            "visibility": meta.visibility.as_str(),
            "shared_with":meta.shared_with,
        }))
        .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

// ── Génération de token JWT pour partage ──────────────────────────────────────

async fn generate_collection_token(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
    Json(req): Json<TokenGenerateRequest>,
) -> Response {
    if let Some(r) = reject_bad_collection(&collection_name) {
        return r;
    }
    let creds = state.credentials.read().await.clone();

    // Seul un admin (clé brute) peut générer des tokens
    let admin_key = match resolve_admin_key(&creds) {
        Some(k) => k,
        None => {
            return err(
                StatusCode::FORBIDDEN,
                "Seul l'administrateur Qdrant (clé admin brute) peut générer des tokens.",
            )
        }
    };
    // D-19 : refuser de signer des jetons partageables avec une clé faible.
    if !admin_key_is_strong(&admin_key) {
        return err(
            StatusCode::BAD_REQUEST,
            "La clé d'administration Qdrant est trop faible pour signer des jetons partageables \
             (32 caractères aléatoires au moins). Générez-en une nouvelle (ex. `openssl rand -hex 32`), \
             configurez-la dans Qdrant, puis réessayez. Voir docs/SECURITE_QDRANT_JWT.md.",
        );
    }

    // Vérifier que la collection existe et que l'utilisateur en est propriétaire
    let user_email = creds.user_email.clone().unwrap_or_default();
    // Administrateur déjà exigé ci-dessus : il peut partir d'une collection externe
    // (sans métadonnées) ; si des métadonnées existent, seul le propriétaire (ou une
    // collection orpheline) peut en émettre. Une erreur de lecture refuse.
    let is_owner = match state.db.get_collection_meta(collection_name.clone()).await {
        Ok(Some(m)) => m.owner_id == user_email || m.owner_id.is_empty() || m.owner_id == "unknown",
        Ok(None) => true,
        Err(_) => false,
    };

    if !is_owner {
        return err(
            StatusCode::FORBIDDEN,
            "Seul le propriétaire peut générer un token pour cette collection.",
        );
    }

    let access_level = if req.access == "rw" { "rw" } else { "r" };
    // D-19 : durée par défaut 90 jours, bornée entre 1 minute et 1 an
    let ttl = req.ttl_seconds.unwrap_or(90 * 24 * 3600);
    if !(60..=365 * 24 * 3600).contains(&ttl) {
        return err(
            StatusCode::BAD_REQUEST,
            "ttl_seconds doit être compris entre 60 secondes et 1 an.",
        );
    }

    let col = QdrantCollectionAccess {
        collection: collection_name.clone(),
        access: access_level.to_string(),
    };

    match generate_qdrant_jwt_with_ttl(&admin_key, &[col], ttl) {
        Ok(token) => {
            let exp = chrono::Utc::now().timestamp() + ttl;
            let exp_date = chrono::DateTime::from_timestamp(exp, 0)
                .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
                .unwrap_or_else(|| "inconnue".to_string());
            Json(json!({
                "token":      token,
                "collection": collection_name,
                "access":     access_level,
                "expires_at": exp_date,
                "ttl_seconds": ttl,
            }))
            .into_response()
        }
        Err(e) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Erreur génération JWT : {}", e),
        ),
    }
}

async fn list_documents(
    State(state): State<Arc<AppState>>,
    Path(collection_name): Path<String>,
    Query(_params): Query<EndpointParams>,
) -> Response {
    let creds_snap = state.credentials.read().await.clone();
    let admin_key = resolve_admin_key(&creds_snap);
    let url = creds_snap.qdrant_url.clone();
    let user_email = creds_snap.user_email.clone().unwrap_or_default();

    // Vérifier que l'utilisateur a accès en lecture à cette collection
    let accessible = can_read_collection(&state, &user_email, &collection_name).await;
    if !accessible {
        return err(StatusCode::FORBIDDEN, "Accès refusé à cette collection.");
    }

    match rag::list_documents(&collection_name, admin_key.as_deref(), url.as_deref()).await {
        Ok(docs) => Json(json!({ "data": docs })).into_response(),
        Err(e) => err(StatusCode::BAD_GATEWAY, e),
    }
}

async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path(doc_id): Path<String>,
    Query(params): Query<EndpointParams>,
) -> Response {
    let creds_snap = state.credentials.read().await.clone();
    let admin_key = resolve_admin_key(&creds_snap);
    let url = creds_snap.qdrant_url.clone();
    // Le collection_name est passé dans le query param `endpoint` (réutilisé comme collection)
    let col = params.endpoint.clone();
    if let Some(r) = reject_bad_collection(&col) {
        return r;
    }

    // Écriture : administrateur ou propriétaire identifié (D-15)
    let can_write = can_write_collection(&state, &creds_snap, &col).await;
    if !can_write {
        return err(
            StatusCode::FORBIDDEN,
            "Seul le propriétaire peut supprimer des documents de cette collection.",
        );
    }

    match rag::delete_document(&col, &doc_id, admin_key.as_deref(), url.as_deref()).await {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::BAD_GATEWAY, e),
    }
}

async fn upload_document(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let creds = state.credentials.read().await.clone();

    info!("[upload] ── Nouvelle requête d'ingestion Qdrant ──");

    // ── Extraction des champs multipart ──────────────────────────────────────
    let mut file_name = String::new();
    let mut file_bytes: Option<Bytes> = None;
    let mut collection_name = String::new();
    let mut chunk_size = 2048usize;
    let mut chunk_overlap = 0usize;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("file").to_string();
                match field.bytes().await {
                    Ok(b) => {
                        tracing::debug!("[upload] fichier '{}' ({} octets)", file_name, b.len());
                        file_bytes = Some(b);
                    }
                    Err(e) => {
                        return err(
                            StatusCode::BAD_REQUEST,
                            format!("Erreur lecture fichier : {e}"),
                        );
                    }
                }
            }
            "collection_id" => {
                // Le frontend envoie collection_id ; ici c'est le nom de la collection Qdrant
                if let Ok(text) = field.text().await {
                    collection_name = text.trim().to_string();
                    info!("[upload] collection : '{}'", collection_name);
                }
            }
            "chunk_size" => {
                if let Ok(text) = field.text().await {
                    chunk_size = text.parse().unwrap_or(2048);
                }
            }
            "chunk_overlap" => {
                if let Ok(text) = field.text().await {
                    chunk_overlap = text.parse().unwrap_or(0);
                }
            }
            other => {
                info!("[upload] champ ignoré : '{}'", other);
                // consommer le field
                let _ = field.bytes().await;
            }
        }
    }

    // D-17 : bornes de découpage (évite boucle infinie / chunks absurdes)
    let chunk_size = chunk_size.clamp(128, 8192);
    let chunk_overlap = chunk_overlap.min(chunk_size / 2);
    if let Some(r) = reject_bad_collection(&collection_name) {
        return r;
    }
    if collection_name.is_empty() {
        return err(StatusCode::BAD_REQUEST, "collection_id manquant");
    }
    let bytes = match file_bytes {
        Some(b) => b,
        None => return err(StatusCode::BAD_REQUEST, "Fichier manquant"),
    };

    // Vérifier que l'utilisateur a un accès en écriture sur la collection
    let can_write = can_write_collection(&state, &creds, &collection_name).await;
    if !can_write {
        return err(
            StatusCode::FORBIDDEN,
            "Accès en écriture refusé : seul le propriétaire peut ajouter des documents.",
        );
    }

    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();

    let doc_id = uuid::Uuid::new_v4().to_string();

    match rag::ingest_document(
        &collection_name,
        &doc_id,
        &file_name,
        &bytes,
        &ext,
        chunk_size,
        chunk_overlap,
        &creds.endpoint,
        &creds.bearer,
        resolve_admin_key(&creds).as_deref(),
        Some(&[crate::rag::QdrantCollectionAccess {
            collection: collection_name.clone(),
            access: "rw".to_string(),
        }]),
        creds.qdrant_url.as_deref(),
    )
    .await
    {
        Ok(n) => {
            info!(
                "[upload] ✓ '{}' → {} chunks dans '{}'",
                file_name, n, collection_name
            );
            Json(json!({
                "ok":     true,
                "id":     doc_id,
                "name":   file_name,
                "chunks": n,
            }))
            .into_response()
        }
        Err(e) => {
            error!("[upload] ✗ '{}' : {}", file_name, e);
            err(StatusCode::UNPROCESSABLE_ENTITY, e)
        }
    }
}

// ── Audio : transcription (avec diarization) ─────────────────────────────────
// Proxifie vers POST {albert_base}/audio/transcriptions (multipart/form-data).
// Le frontend envoie : file, model, response_format (diarized_json par défaut),
// language (optionnel), temperature (optionnel).

async fn transcribe_audio(
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Response {
    let creds = state.credentials.read().await.clone();
    let (base, bearer) = match albert_target(&creds) {
        Ok(t) => t,
        Err(m) => return err(StatusCode::BAD_REQUEST, m),
    };

    info!("[audio] ── Nouvelle requête de transcription ──");

    let mut file_name = String::new();
    let mut file_bytes: Option<Bytes> = None;
    let mut model = String::new();
    let mut language: Option<String> = None;
    let mut response_format = "diarized_json".to_string();
    let mut temperature: Option<String> = None;
    let mut prompt: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_name = field.file_name().unwrap_or("audio").to_string();
                match field.bytes().await {
                    Ok(b) => {
                        tracing::debug!("[audio] fichier '{}' ({} octets)", file_name, b.len());
                        file_bytes = Some(b);
                    }
                    Err(e) => {
                        return err(
                            StatusCode::BAD_REQUEST,
                            format!("Erreur lecture fichier : {e}"),
                        );
                    }
                }
            }
            "model" => {
                if let Ok(t) = field.text().await {
                    model = t.trim().to_string();
                }
            }
            "language" => {
                if let Ok(t) = field.text().await {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        language = Some(t);
                    }
                }
            }
            "response_format" => {
                if let Ok(t) = field.text().await {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        response_format = t;
                    }
                }
            }
            "temperature" => {
                if let Ok(t) = field.text().await {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        temperature = Some(t);
                    }
                }
            }
            "prompt" => {
                if let Ok(t) = field.text().await {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        prompt = Some(t);
                    }
                }
            }
            other => {
                info!("[audio] champ ignoré : '{}'", other);
                let _ = field.bytes().await;
            }
        }
    }

    let bytes = match file_bytes {
        Some(b) => b,
        None => return err(StatusCode::BAD_REQUEST, "Fichier audio manquant."),
    };
    if model.is_empty() {
        return err(StatusCode::BAD_REQUEST, "Modèle de transcription manquant.");
    }

    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let mime = match ext.as_str() {
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "ogg" => "audio/ogg",
        "webm" => "audio/webm",
        _ => "application/octet-stream",
    };

    let file_part = match reqwest::multipart::Part::bytes(bytes.to_vec())
        .file_name(file_name.clone())
        .mime_str(mime)
    {
        Ok(p) => p,
        Err(e) => {
            return err(
                StatusCode::BAD_REQUEST,
                format!("Type de fichier audio invalide : {e}"),
            );
        }
    };

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", model.clone())
        .text("response_format", response_format.clone());

    if let Some(lang) = language {
        form = form.text("language", lang);
    }
    if let Some(temp) = temperature {
        form = form.text("temperature", temp);
    }
    if let Some(p) = prompt {
        form = form.text("prompt", p);
    }

    let client = Client::builder()
        .timeout(config::timeout_audio_transcription())
        .use_rustls_tls()
        .user_agent("Mozilla/5.0 (compatible; Demeter/1.0)")
        .build()
        .unwrap_or_default();

    info!(
        "[audio] → transcription '{}' via modèle '{}' (format={})",
        file_name, model, response_format
    );

    match client
        .post(format!("{}/audio/transcriptions", base))
        .bearer_auth(&bearer)
        .multipart(form)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            // Pour text/srt/vtt, Albert renvoie du texte brut (pas du JSON).
            if matches!(response_format.as_str(), "text" | "srt" | "vtt") {
                let text = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    info!("[audio] ✓ transcription reçue ({} caractères)", text.len());
                } else {
                    error!(
                        "[audio] ✗ échec transcription : {} — {}",
                        status,
                        truncate_utf8(&text, 500)
                    );
                }
                return (
                    StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                    Json(json!({ "text": text })),
                )
                    .into_response();
            }

            let body: Value = resp.json().await.unwrap_or(json!({}));
            if status.is_success() {
                info!("[audio] ✓ transcription reçue");
            } else {
                error!("[audio] ✗ échec transcription : {} — {:?}", status, body);
            }
            (
                StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK),
                Json(body),
            )
                .into_response()
        }
        Err(e) => {
            error!("[audio] ✗ erreur réseau : {}", e);
            err(StatusCode::BAD_GATEWAY, e.to_string())
        }
    }
}

#[cfg(test)]
mod acl_tests {
    use super::*;
    use crate::models::{CollectionMeta, CollectionVisibility};

    async fn state_with(email: Option<&str>, admin_key: Option<&str>) -> Arc<AppState> {
        // Une clé d'administration définie dans l'environnement fausserait le test.
        std::env::remove_var("QDRANT_API_KEY");
        let db = Database::in_memory().await.unwrap();
        let (_router, st) = build_router(
            db,
            PathBuf::from("prompts.yml"),
            None,
            "jeton-de-test".into(),
            45678,
            Default::default(),
        );
        {
            let mut c = st.credentials.write().await;
            c.user_email = email.map(str::to_string);
            c.qdrant_api_key = admin_key.map(str::to_string);
        }
        st
    }

    async fn add_meta(
        st: &AppState,
        name: &str,
        owner: &str,
        vis: CollectionVisibility,
        shared: &[&str],
    ) {
        st.db
            .upsert_collection_meta(CollectionMeta {
                collection_name: name.into(),
                owner_id: owner.into(),
                visibility: vis,
                shared_with: shared.iter().map(|s| s.to_string()).collect(),
                created_at: "2026-01-01T00:00:00Z".into(),
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn proprietaire_ecrit_les_autres_non() {
        let st = state_with(Some("alice@x.fr"), None).await;
        add_meta(&st, "rh", "alice@x.fr", CollectionVisibility::Private, &[]).await;
        add_meta(&st, "compta", "bob@x.fr", CollectionVisibility::Public, &[]).await;
        let creds = st.credentials.read().await.clone();
        assert!(can_write_collection(&st, &creds, "rh").await);
        assert!(
            !can_write_collection(&st, &creds, "compta").await,
            "publique ≠ modifiable"
        );
        // lecture : la publique est lisible, la privée d'autrui non
        assert!(can_read_collection(&st, "alice@x.fr", "compta").await);
        add_meta(
            &st,
            "secret",
            "bob@x.fr",
            CollectionVisibility::Private,
            &[],
        )
        .await;
        assert!(!can_read_collection(&st, "alice@x.fr", "secret").await);
        add_meta(
            &st,
            "partage",
            "bob@x.fr",
            CollectionVisibility::Shared,
            &["alice@x.fr"],
        )
        .await;
        assert!(can_read_collection(&st, "alice@x.fr", "partage").await);
        assert!(!can_write_collection(&st, &creds, "partage").await);
    }

    #[tokio::test]
    async fn collection_externe_lisible_mais_ecriture_reservee_a_l_admin() {
        let user = state_with(Some("alice@x.fr"), None).await;
        let creds = user.credentials.read().await.clone();
        assert!(can_read_collection(&user, "alice@x.fr", "externe").await);
        assert!(!can_write_collection(&user, &creds, "externe").await);

        let admin = state_with(Some("admin@x.fr"), Some("cle-admin-brute-123")).await;
        let creds = admin.credentials.read().await.clone();
        assert!(can_write_collection(&admin, &creds, "externe").await);
    }

    #[tokio::test]
    async fn proprietaire_orphelin_ou_unknown_n_ouvre_plus_rien() {
        let st = state_with(Some("alice@x.fr"), None).await;
        add_meta(
            &st,
            "ancienne",
            "unknown",
            CollectionVisibility::Private,
            &[],
        )
        .await;
        add_meta(&st, "vide", "", CollectionVisibility::Private, &[]).await;
        let creds = st.credentials.read().await.clone();
        assert!(!can_write_collection(&st, &creds, "ancienne").await);
        assert!(!can_write_collection(&st, &creds, "vide").await);
    }

    #[tokio::test]
    async fn sans_email_connu_aucune_ecriture() {
        let st = state_with(None, None).await;
        add_meta(&st, "rh", "unknown", CollectionVisibility::Public, &[]).await;
        let creds = st.credentials.read().await.clone();
        assert!(!can_write_collection(&st, &creds, "rh").await);
        let st2 = state_with(Some("unknown"), None).await;
        let creds2 = st2.credentials.read().await.clone();
        assert!(known_email(&creds2).is_none());
        assert!(!can_write_collection(&st2, &creds2, "rh").await);
    }

    #[tokio::test]
    async fn un_jwt_n_est_pas_un_administrateur() {
        let st = state_with(
            Some("alice@x.fr"),
            Some("eyJhbGciOiJIUzI1NiJ9.eyJhIjoiciJ9.sig"),
        )
        .await;
        let creds = st.credentials.read().await.clone();
        assert!(!is_admin(&creds));
        assert!(!can_write_collection(&st, &creds, "externe").await);
    }
}

#[cfg(test)]
mod hygiene_tests {
    use super::*;

    #[test]
    fn messages_publics_masquent_les_urls_et_tronquent() {
        let m = public_message("error sending request for url (http://10.0.0.5:6333/collections/x?api-key=SECRET): connection refused");
        assert!(!m.contains("10.0.0.5") && !m.contains("SECRET"), "{m}");
        assert!(m.contains("[url]"));
        let long = public_message(&"a".repeat(1000));
        assert!(long.chars().count() <= 241);
        assert_eq!(
            public_message("première\nseconde ligne secrète"),
            "première"
        );
        assert!(!public_message("").is_empty());
    }

    #[test]
    fn cle_admin_forte_ou_faible() {
        assert!(admin_key_is_strong(
            "0123456789abcdef0123456789abcdef0123456789abcdef"
        ));
        assert!(!admin_key_is_strong("secret"));
        assert!(
            !admin_key_is_strong(&"a".repeat(40)),
            "pas assez de variété"
        );
        assert!(!admin_key_is_strong("0123456789abcdef0123456789abcde")); // 31 caractères
    }
}
