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
mod archive;
mod config;
mod db;
mod extract;
mod mcp;
mod mcp_pack;
mod mcp_stdio;
mod models;
#[cfg(target_os = "windows")]
mod ocr_windows;
mod rag;
mod spaces;
mod staged;

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
}

#[tauri::command]
async fn init_credentials(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    payload: CredentialsPayload,
) -> Result<(), String> {
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

    // ── TRACE init_credentials ───────────────────────────────────────────────
    let creds = state.credentials.read().await;
    let is_jwt_key = creds
        .qdrant_api_key
        .as_deref()
        .map(|k| k.starts_with("eyJ"))
        .unwrap_or(false);
    tracing::info!(
        "[TRACE init_credentials] user_email={:?} qdrant_key_type={} key_prefix={:?}          qdrant_url={:?} nb_collection_tokens={} collections={:?} mcp_dir={:?}",
        creds.user_email,
        if is_jwt_key { "JWT" } else { "raw_key_or_none" },
        creds.qdrant_api_key.as_deref().map(|k| &k[..k.len().min(12)]),
        creds.qdrant_url,
        creds.qdrant_collection_tokens.len(),
        creds.qdrant_collection_tokens.iter().map(|ct| ct.collection.clone()).collect::<Vec<_>>(),
        state.mcp_stdio.dir().await,
    );

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Charge le fichier .env s'il existe (silencieux si absent)
    let _ = dotenvy::dotenv();

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

            let db = tauri::async_runtime::block_on(async {
                Database::new(&db_path_clone)
                    .await
                    .expect("Failed to open database")
            });

            let (router, app_state) = api::build_router(db, prompts_path_clone, official_mcp_dir);

            // ── GC des batchs de staging (uploads d'archives) ────────────────
            // Purge périodique des batchs inactifs (VFS éphémère). Les TempDir
            // sont détruits (RAII) → les fichiers spoolés sont supprimés du disque.
            {
                let uploads_store = app_state.uploads.clone();
                tauri::async_runtime::spawn(async move {
                    let ttl = config::upload_batch_ttl();
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
                    ticker.tick().await; // premier tick immédiat ignoré
                    loop {
                        ticker.tick().await;
                        let purged = {
                            let mut store = uploads_store.write().await;
                            store.purge_expired(ttl)
                        };
                        if purged > 0 {
                            info!("[staging] {purged} batch(s) expiré(s) purgé(s)");
                        }
                    }
                });
            }

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

            tauri::async_runtime::spawn(async move {
                let addr = SocketAddr::from(([127, 0, 0, 1], config::api_port()));
                info!("API server listening on http://{}", addr);

                let listener = tokio::net::TcpListener::bind(addr)
                    .await
                    .expect("Failed to bind API port");

                axum::serve(listener, router)
                    .await
                    .expect("API server crashed");
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![init_credentials])
        .run(tauri::generate_context!())
        .expect("error while running Demeter");
}
