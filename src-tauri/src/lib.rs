use std::net::SocketAddr;
use tauri::Manager;
use tauri::menu::{Menu, Submenu, MenuItem, PredefinedMenuItem};
use tracing::info;

mod api;
mod db;
mod mcp;
mod models;
mod rag;
mod spaces;
mod web_search;
mod extract;

pub use db::Database;

/// Port on which the embedded Axum server listens.
const API_PORT: u16 = 45678;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

            let menu = Menu::with_items(handle, &[
                // Demeter (app menu sur macOS)
                &Submenu::with_items(handle, "Demeter", true, &[
                    &MenuItem::with_id(handle, "new_conv",      "Nouvelle conversation", true, Some("CmdOrCtrl+N"))?,
                    &PredefinedMenuItem::separator(handle)?,
                    &MenuItem::with_id(handle, "settings",      "Paramètres…",          true, Some("CmdOrCtrl+,"))?,
                    &PredefinedMenuItem::separator(handle)?,
                    &MenuItem::with_id(handle, "quit",          "Quitter",              true, Some("CmdOrCtrl+Q"))?,
                ])?,
                // Espaces & Prompts
                &Submenu::with_items(handle, "Espaces", true, &[
                    &MenuItem::with_id(handle, "spaces_editor", "Espaces & prompts…",   true, Some("CmdOrCtrl+E"))?,
                    &MenuItem::with_id(handle, "rag",           "Ingestion RAG…",       true, Some("CmdOrCtrl+R"))?,
                ])?,
                // Affichage
                &Submenu::with_items(handle, "Affichage", true, &[
                    &MenuItem::with_id(handle, "toggle_sidebar","Afficher/Masquer sidebar", true, Some("CmdOrCtrl+B"))?,
                    &PredefinedMenuItem::separator(handle)?,
                    &PredefinedMenuItem::fullscreen(handle, None)?,
                ])?,
                // Aide
                &Submenu::with_items(handle, "Aide", true, &[
                    &MenuItem::with_id(handle, "about",         "À propos de Demeter…", true, None::<&str>)?,
                ])?,
            ])?;

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

            tauri::async_runtime::spawn(async move {
                let db = Database::new(&db_path_clone)
                    .await
                    .expect("Failed to open database");

                let router = api::build_router(db, prompts_path_clone);

                let addr = SocketAddr::from(([127, 0, 0, 1], API_PORT));
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
        .run(tauri::generate_context!())
        .expect("error while running Demeter");
}
