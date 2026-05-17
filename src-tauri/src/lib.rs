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
mod mcp;
mod models;
mod rag;
mod spaces;
mod web_search;

pub use api::{AppState, Credentials};
pub use db::Database;

// ── Commande Tauri : initialise les credentials dans AppState ─────────────────
// Appelée depuis le frontend via invoke() — IPC synchrone, pas HTTP.
// Garantit que AppState est peuplé avant toute requête HTTP au serveur Axum.

#[derive(serde::Deserialize)]
struct CredentialsPayload {
    endpoint: String,
    bearer: String,
    #[serde(default)]
    tavily_key: String,
}

#[tauri::command]
async fn init_credentials(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    payload: CredentialsPayload,
) -> Result<(), String> {
    let mut creds = state.credentials.write().await;
    creds.endpoint = payload.endpoint;
    creds.bearer = payload.bearer;
    creds.tavily_key = payload.tavily_key;
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
                                "Ingestion RAG…",
                                true,
                                Some("CmdOrCtrl+R"),
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

            info!("Data dir: {}", app_data_dir.display());
            info!("DB path: {}", db_path.display());
            info!("Prompts: {}", prompts_path.display());

            let db_path_clone = db_path.clone();
            let prompts_path_clone = prompts_path.clone();

            let db = tauri::async_runtime::block_on(async {
                Database::new(&db_path_clone)
                    .await
                    .expect("Failed to open database")
            });

            let (router, app_state) = api::build_router(db, prompts_path_clone);

            // Partager AppState avec les commandes Tauri
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
