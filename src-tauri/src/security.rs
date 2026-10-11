// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Sécurisation de l'API HTTP locale (constat D-01).
//
// « localhost » n'est pas une frontière de confiance : toute page web ouverte
// dans le navigateur de l'utilisateur peut émettre des requêtes vers
// 127.0.0.1 (fetch direct, DNS rebinding). Chaque requête doit donc prouver
// qu'elle vient de l'application :
//
//   1. en-tête `Host`  : doit désigner la boucle locale ET le port d'écoute
//      (neutralise le DNS rebinding) ;
//   2. en-tête `Origin` : s'il est présent, il doit être une origine de
//      l'application (WebView Tauri, ou serveur Vite en build de debug) ;
//   3. jeton secret `X-Demeter-Token` : 256 bits aléatoires générés à chaque
//      lancement, transmis au frontend uniquement par la commande IPC Tauri
//      `get_api_token` (jamais par HTTP), comparé en temps constant.
//
// Seule exception : GET /health (ne renvoie aucune donnée).
// ============================================================================

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

/// Nom de l'en-tête portant le jeton d'API (en minuscules, comme l'exige http).
pub const TOKEN_HEADER: &str = "x-demeter-token";

/// Origines de la WebView Tauri en production (macOS/Linux : `tauri://localhost`,
/// Windows : `http(s)://tauri.localhost` selon `useHttpsScheme`).
const APP_ORIGINS: &[&str] = &[
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

/// Origines du serveur de développement Vite — uniquement en build de debug.
#[cfg(debug_assertions)]
const DEV_ORIGINS: &[&str] = &["http://localhost:3010", "http://127.0.0.1:3010"];
#[cfg(not(debug_assertions))]
const DEV_ORIGINS: &[&str] = &[];

/// Liste des origines autorisées (pour la couche CORS).
pub fn allowed_origins() -> Vec<HeaderValue> {
    APP_ORIGINS
        .iter()
        .chain(DEV_ORIGINS.iter())
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect()
}

fn origin_allowed(origin: &str) -> bool {
    APP_ORIGINS
        .iter()
        .chain(DEV_ORIGINS.iter())
        .any(|o| *o == origin)
}

/// Vrai si l'en-tête `Host` désigne la boucle locale sur le port d'écoute.
fn host_allowed(host: &str, port: u16) -> bool {
    let host = host.trim().to_ascii_lowercase();
    let (name, p) = if let Some(rest) = host.strip_prefix('[') {
        // IPv6 littérale : "[::1]:45678"
        match rest.split_once("]:") {
            Some((ip, p)) => (format!("[{ip}]"), p.to_string()),
            None => return false,
        }
    } else {
        match host.rsplit_once(':') {
            Some((n, p)) => (n.to_string(), p.to_string()),
            None => return false,
        }
    };
    let name_ok = matches!(name.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    name_ok && p.parse::<u16>().map(|v| v == port).unwrap_or(false)
}

/// Comparaison en temps constant (pour ne pas révéler le jeton par le timing).
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Génère le jeton d'API de cette session : 2 × UUID v4 (CSPRNG du système),
/// soit 244 bits d'aléa. En build de debug, `DEMETER_API_TOKEN` (≥ 16 car.)
/// permet de fixer le jeton pour le développement dans un navigateur.
pub fn generate_token() -> String {
    #[cfg(debug_assertions)]
    if let Ok(t) = std::env::var("DEMETER_API_TOKEN") {
        if t.len() >= 16 {
            return t;
        }
    }
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// État du garde : jeton attendu et port d'écoute.
#[derive(Clone)]
pub struct ApiGuard {
    token: Arc<String>,
    port: u16,
}

impl ApiGuard {
    pub fn new(token: String, port: u16) -> Self {
        Self {
            token: Arc::new(token),
            port,
        }
    }
}

fn deny(status: StatusCode, msg: &'static str) -> Response {
    (status, msg).into_response()
}

/// Middleware axum à appliquer à TOUT le routeur (routeur imbriqué compris).
pub async fn guard(State(g): State<ApiGuard>, req: Request, next: Next) -> Response {
    // 1) Host : boucle locale + port d'écoute (anti DNS rebinding).
    let host_ok = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| host_allowed(h, g.port))
        .unwrap_or(false);
    if !host_ok {
        return deny(StatusCode::FORBIDDEN, "Hôte non autorisé");
    }

    // 2) Origin : s'il est présent, il doit être une origine de l'application.
    if let Some(origin) = req.headers().get(header::ORIGIN) {
        let ok = origin.to_str().map(origin_allowed).unwrap_or(false);
        if !ok {
            return deny(StatusCode::FORBIDDEN, "Origine non autorisée");
        }
    }

    // 3) Seule exception au jeton : GET /health.
    let path = req.uri().path();
    if req.method() == Method::GET && (path == "/health" || path == "/api-proxy/health") {
        return next.run(req).await;
    }

    // 4) Jeton secret, comparé en temps constant.
    let token_ok = req
        .headers()
        .get(TOKEN_HEADER)
        .map(|v| ct_eq(v.as_bytes(), g.token.as_bytes()))
        .unwrap_or(false);
    if !token_ok {
        return deny(StatusCode::UNAUTHORIZED, "Jeton d'API manquant ou invalide");
    }

    next.run(req).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_accepte_boucle_locale_avec_bon_port() {
        assert!(host_allowed("localhost:45678", 45678));
        assert!(host_allowed("127.0.0.1:45678", 45678));
        assert!(host_allowed("[::1]:45678", 45678));
        assert!(host_allowed("LocalHost:45678", 45678));
    }

    #[test]
    fn host_refuse_rebinding_et_mauvais_port() {
        assert!(!host_allowed("evil.example:45678", 45678));
        assert!(!host_allowed("localhost.evil.example:45678", 45678));
        assert!(!host_allowed("localhost:8080", 45678));
        assert!(!host_allowed("localhost", 45678));
        assert!(!host_allowed("", 45678));
        assert!(!host_allowed("[::1]", 45678));
        assert!(!host_allowed("127.0.0.1.evil.example:45678", 45678));
    }

    #[test]
    fn origin_refuse_sites_tiers() {
        assert!(origin_allowed("tauri://localhost"));
        assert!(origin_allowed("http://tauri.localhost"));
        assert!(!origin_allowed("https://evil.example"));
        assert!(!origin_allowed("null"));
        assert!(!origin_allowed("http://localhost:45678"));
    }

    #[test]
    fn comparaison_temps_constant() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"abcd"));
        assert!(!ct_eq(b"", b"a"));
    }

    #[test]
    fn jetons_longs_et_uniques() {
        let a = generate_token();
        let b = generate_token();
        assert!(a.len() >= 16);
        assert_ne!(a, b);
    }
}
