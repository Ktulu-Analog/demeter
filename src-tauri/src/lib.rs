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

use std::net::SocketAddr;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Manager;
use tracing::info;

mod api;
mod config;
mod db;
mod extract;
mod image_fetch;
mod mcp;
mod mcp_pack;
mod mcp_stdio;
mod models;
mod rag;
mod secrets;
mod security;
mod spaces;

pub use api::{AppState, Credentials};
pub use db::Database;

// ── Commande Tauri : initialise les credentials dans AppState ─────────────────
// Appelée depuis le frontend via invoke() — IPC synchrone, pas HTTP.
// Garantit que AppState est peuplé avant toute requête HTTP au serveur Axum.

#[derive(serde::Deserialize)]
struct CredentialsPayload {
    endpoint: String,
    bearer: String,
    qdrant_api_key: Option<String>,
    qdrant_url: Option<String>,
    user_email: Option<String>,
    /// Tokens JWT additionnels par collection (utilisateurs non-admin multi-collections)
    #[serde(default)]
    qdrant_collection_tokens: Vec<crate::api::CollectionToken>,
    /// Dossier choisi par l'utilisateur dans le panel MCP pour ses serveurs
    /// MCP stdio (mcp-*). `None`/absent = pas encore configuré.
    #[serde(default)]
    mcp_dir: Option<String>,
    /// Valeurs de configuration saisies dans le panel MCP, par serveur
    /// (id du manifest) puis par clé de paramètre (ex : "GRIST_API_KEY").
    #[serde(default)]
    mcp_env: std::collections::HashMap<String, std::collections::HashMap<String, String>>,
    /// Serveurs MCP configurés (URL http(s) ou « stdio:<id> »). Seuls ces serveurs
    /// sont utilisables via l'API HTTP (D-03).
    #[serde(default)]
    mcp_servers: Vec<String>,
}

/// Retourne le jeton secret de l'API locale (D-01). Commande IPC : le jeton
/// n'est jamais exposé par HTTP.
#[tauri::command]
async fn get_api_token(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<String, String> {
    Ok(state.api_token.clone())
}

/// Retourne l'état de sécurité (chiffrement de la base, trousseau) pour que
/// l'interface avertisse l'utilisateur en cas de mode dégradé (D-08, D-09).
#[tauri::command]
async fn get_security_status(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
) -> Result<secrets::SecurityStatus, String> {
    Ok(state.security.clone())
}

/// Lit les secrets de configuration (JSON) dans le trousseau du système (D-08).
/// Le nom de l'entrée est FIXE : le frontend ne peut lire aucun autre secret
/// (en particulier pas la clé de chiffrement de la base).
#[tauri::command]
async fn secrets_get() -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(|| secrets::get_blob(secrets::SETTINGS_BLOB))
        .await
        .map_err(|e| e.to_string())?
}

/// Enregistre les secrets de configuration (objet JSON) dans le trousseau.
#[tauri::command]
async fn secrets_set(value: String) -> Result<(), String> {
    if value.len() > 512 * 1024 {
        return Err("Secrets trop volumineux".into());
    }
    if !serde_json::from_str::<serde_json::Value>(&value)
        .map(|v| v.is_object())
        .unwrap_or(false)
    {
        return Err("Les secrets doivent être un objet JSON".into());
    }
    tauri::async_runtime::spawn_blocking(move || secrets::set_blob(secrets::SETTINGS_BLOB, &value))
        .await
        .map_err(|e| e.to_string())?
}

/// Ouvre la base de conversations : chiffrée (SQLCipher) avec une clé du
/// trousseau ; migre une ancienne base en clair ; en cas d'échec, bascule en
/// mémoire (aucune donnée détruite) et le signale par `SecurityStatus` (D-09).
async fn open_database(path: &std::path::Path) -> (Database, secrets::SecurityStatus) {
    let keychain_ok = tauri::async_runtime::spawn_blocking(secrets::keychain_available)
        .await
        .unwrap_or(false);
    let mut status = secrets::SecurityStatus {
        keychain_available: keychain_ok,
        ..Default::default()
    };

    let degraded = |mut status: secrets::SecurityStatus, msg: String| async move {
        tracing::error!("{msg}");
        status.warnings.push(msg);
        let db = Database::in_memory()
            .await
            .expect("Failed to open in-memory database");
        (db, status)
    };

    let path_buf = path.to_path_buf();
    let key = match tauri::async_runtime::spawn_blocking(move || secrets::resolve_db_key(&path_buf))
        .await
    {
        Ok(Ok(k)) => k,
        Ok(Err(msg)) => return degraded(status, msg).await,
        Err(e) => return degraded(status, e.to_string()).await,
    };
    let key_str = match &key {
        secrets::DbKey::Key(k) => Some(k.clone()),
        secrets::DbKey::Plaintext => None,
    };

    let migrated = match db::prepare_file(path, key_str.as_deref()) {
        Ok(m) => m,
        Err(e) => {
            return degraded(status, format!("Migration de la base impossible : {e:#}")).await
        }
    };
    match Database::new(path, key_str.as_deref()).await {
        Ok(db) => {
            status.db_encrypted = key_str.is_some();
            status.db_persistent = true;
            if key_str.is_none() {
                status.warnings.push(
                    "Base de conversations NON chiffrée (DEMETER_ALLOW_PLAINTEXT_DB=1).".into(),
                );
            }
            // La sauvegarde en clair d'une migration précédente est supprimée dès
            // que la base chiffrée a été ouverte avec succès lors d'un lancement ultérieur.
            if !migrated {
                db::purge_pre_encryption_backup(path);
            }
            (db, status)
        }
        Err(e) => degraded(status, format!("Ouverture de la base impossible : {e:#}")).await,
    }
}

/// L'endpoint LLM doit être une URL http(s) (D-02) ; toute autre forme est refusée.
fn validate_http_url(url: &str) -> Result<(), String> {
    let u = url.trim().to_ascii_lowercase();
    if u.is_empty() || u.starts_with("http://") || u.starts_with("https://") {
        Ok(())
    } else {
        Err("L'endpoint doit commencer par http:// ou https://".to_string())
    }
}

#[tauri::command]
async fn init_credentials(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    payload: CredentialsPayload,
) -> Result<(), String> {
    validate_http_url(&payload.endpoint)?;
    if let Some(u) = payload.qdrant_url.as_deref() {
        validate_http_url(u)?;
    }
    let mut creds = state.credentials.write().await;
    creds.endpoint = payload.endpoint;
    creds.bearer = payload.bearer;
    creds.qdrant_api_key = payload.qdrant_api_key.filter(|k| !k.trim().is_empty());
    creds.qdrant_url = payload.qdrant_url.filter(|u| !u.trim().is_empty());
    creds.user_email = payload.user_email.filter(|e| !e.trim().is_empty());
    creds.qdrant_collection_tokens = payload
        .qdrant_collection_tokens
        .into_iter()
        .filter(|ct| !ct.token.trim().is_empty() && !ct.collection.trim().is_empty())
        .collect();
    drop(creds);

    // ── MCP stdio : dossier + paramètres de configuration ──────────────────
    let mcp_dir = payload
        .mcp_dir
        .filter(|p| !p.trim().is_empty())
        .map(std::path::PathBuf::from);
    state.mcp_stdio.set_dir(mcp_dir).await;
    *state.mcp_env.write().await = payload.mcp_env;
    *state.mcp_allowed.write().await = payload
        .mcp_servers
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    // Aucune donnée sensible (clé, e-mail, chemins) n'est journalisée (D-20).
    tracing::debug!(
        "init_credentials : {} jeton(s) de collection, {} serveur(s) MCP autorisé(s)",
        state
            .credentials
            .read()
            .await
            .qdrant_collection_tokens
            .len(),
        state.mcp_allowed.read().await.len(),
    );

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Charge le fichier .env s'il existe — UNIQUEMENT en build de debug (D-22) : en
    // production, un .env présent dans le dossier de lancement ne doit pas pouvoir
    // rediriger QDRANT_URL / API_PORT ni les clés.
    #[cfg(debug_assertions)]
    {
        let _ = dotenvy::dotenv();
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "demeter=info,tower_http=info".to_string()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // ── DevTools en mode debug ────────────────────────────────────────
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") {
                w.open_devtools();
            }

            // ── Menu natif ────────────────────────────────────────────────────
            let handle = app.handle();

            let menu = Menu::with_items(
                handle,
                &[
                    // Demeter (app menu sur macOS)
                    &Submenu::with_items(
                        handle,
                        "Demeter",
                        true,
                        &[
                            &MenuItem::with_id(
                                handle,
                                "new_conv",
                                "Nouvelle conversation",
                                true,
                                Some("CmdOrCtrl+N"),
                            )?,
                            &PredefinedMenuItem::separator(handle)?,
                            &MenuItem::with_id(
                                handle,
                                "settings",
                                "Paramètres…",
                                true,
                                Some("CmdOrCtrl+,"),
                            )?,
                            &PredefinedMenuItem::separator(handle)?,
                            &MenuItem::with_id(
                                handle,
                                "quit",
                                "Quitter",
                                true,
                                Some("CmdOrCtrl+Q"),
                            )?,
                        ],
                    )?,
                    // Espaces & Prompts
                    &Submenu::with_items(
                        handle,
                        "Espaces",
                        true,
                        &[
                            &MenuItem::with_id(
                                handle,
                                "spaces_editor",
                                "Espaces & prompts…",
                                true,
                                Some("CmdOrCtrl+E"),
                            )?,
                            &MenuItem::with_id(
                                handle,
                                "rag",
                                "Collections RAG…",
                                true,
                                Some("CmdOrCtrl+R"),
                            )?,
                            &MenuItem::with_id(
                                handle,
                                "audio_transcription",
                                "Transcription audio…",
                                true,
                                Some("CmdOrCtrl+T"),
                            )?,
                            &PredefinedMenuItem::separator(handle)?,
                            &MenuItem::with_id(
                                handle,
                                "mcp_panel",
                                "Serveurs MCP…",
                                true,
                                Some("CmdOrCtrl+M"),
                            )?,
                        ],
                    )?,
                    // Affichage
                    &Submenu::with_items(
                        handle,
                        "Affichage",
                        true,
                        &[
                            &MenuItem::with_id(
                                handle,
                                "toggle_sidebar",
                                "Afficher/Masquer sidebar",
                                true,
                                Some("CmdOrCtrl+B"),
                            )?,
                            &PredefinedMenuItem::separator(handle)?,
                            &PredefinedMenuItem::fullscreen(handle, None)?,
                        ],
                    )?,
                    // Aide
                    &Submenu::with_items(
                        handle,
                        "Aide",
                        true,
                        &[&MenuItem::with_id(
                            handle,
                            "about",
                            "À propos de Demeter…",
                            true,
                            None::<&str>,
                        )?],
                    )?,
                ],
            )?;

            app.set_menu(menu)?;

            // Déclencher un event DOM sur clic de menu via eval JS
            app.on_menu_event(move |app, event| {
                let id = event.id().0.clone();
                if id == "quit" {
                    app.exit(0);
                    return;
                }
                if let Some(window) = app.get_webview_window("main") {
                    let js = format!(
                        "window.dispatchEvent(new CustomEvent('tauri-menu', {{ detail: '{}' }}))",
                        id
                    );
                    let _ = window.eval(&js);
                }
            });

            // ── API server ────────────────────────────────────────────────────
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to resolve app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create data dir");

            let prompts_path = app
                .path()
                .resource_dir()
                .unwrap_or_else(|_| app_data_dir.clone())
                .join("prompts.yml");

            let prompts_path = if prompts_path.exists() {
                prompts_path
            } else {
                let p = app_data_dir.join("prompts.yml");
                if !p.exists() {
                    std::fs::write(&p, include_str!("../prompts.yml"))
                        .expect("Failed to write default prompts.yml");
                }
                p
            };

            let db_path = app_data_dir.join("conversations.db");

            // ── MCP stdio ─────────────────────────────────────────────────────
            // Le dossier scanné pour les serveurs MCP stdio est choisi par
            // l'utilisateur depuis le panel MCP dédié (pas de dossier imposé
            // par Demeter) — il est transmis au backend via la commande Tauri
            // `init_credentials`, comme les autres préférences. On résout ici
            // uniquement le dossier des MCP "officiels" embarqués dans les
            // resources de l'app, utilisé par le bouton "Installer les MCP
            // officiels" du panel pour les copier dans le dossier choisi.
            let official_mcp_dir = app
                .path()
                .resource_dir()
                .ok()
                .map(|d| d.join("mcp-servers"))
                .filter(|d| d.exists());

            info!("Data dir: {}", app_data_dir.display());
            info!("DB path: {}", db_path.display());
            info!("Prompts: {}", prompts_path.display());
            match &official_mcp_dir {
                Some(d) => info!("MCP officiels embarqués: {}", d.display()),
                None => info!("Aucun MCP officiel embarqué (build de dev sans resources)."),
            }

            let db_path_clone = db_path.clone();
            let prompts_path_clone = prompts_path.clone();

            let (db, security_status) =
                tauri::async_runtime::block_on(async { open_database(&db_path_clone).await });

            // D-01 : jeton secret de session + port d'écoute connu du garde (Host).
            let api_port = config::api_port();
            let api_token = security::generate_token();
            let (router, app_state) = api::build_router(
                db,
                prompts_path_clone,
                official_mcp_dir,
                api_token,
                api_port,
                security_status,
            );

            // Arrêt propre des process MCP stdio quand la fenêtre se ferme.
            if let Some(window) = app.get_webview_window("main") {
                let mcp_state_for_exit = app_state.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        let mcp_state = mcp_state_for_exit.clone();
                        tauri::async_runtime::block_on(async move {
                            mcp_state.mcp_stdio.stop_all().await;
                        });
                    }
                });
            }

            // Partager AppState avec les commandes Tauri (init_credentials...)
            handle.manage(app_state);

            // D-22 : le port est réservé AVANT d'ouvrir l'interface. S'il est déjà
            // pris, Demeter s'arrête avec un message clair : l'interface enverrait
            // sinon son jeton d'API à n'importe quel programme qui écoute sur ce port.
            let addr = SocketAddr::from(([127, 0, 0, 1], api_port));
            let std_listener = match std::net::TcpListener::bind(addr) {
                Ok(l) => l,
                Err(e) => {
                    let msg = format!(
                        "Le port local {api_port} est déjà utilisé par un autre programme ({e}).\n\n\
                         Par sécurité, Demeter ne démarre pas : cet autre programme pourrait \
                         recevoir vos identifiants. Fermez-le, puis relancez Demeter."
                    );
                    tracing::error!("{msg}");
                    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                    app.dialog()
                        .message(msg)
                        .title("Demeter — port indisponible")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(e.into());
                }
            };
            std_listener.set_nonblocking(true)?;

            tauri::async_runtime::spawn(async move {
                info!("API server listening on http://{}", addr);

                let listener = tokio::net::TcpListener::from_std(std_listener)
                    .expect("Failed to register API listener");

                axum::serve(listener, router)
                    .await
                    .expect("API server crashed");
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            init_credentials,
            get_api_token,
            get_security_status,
            secrets_get,
            secrets_set
        ])
        .run(tauri::generate_context!())
        .expect("error while running Demeter");
}
