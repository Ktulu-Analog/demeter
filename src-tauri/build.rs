use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=prompts.yml");

    // tauri.conf.json embarque mcp-servers/**/* comme resource (voir
    // scripts/fetch-mcp-pack.sh). Ce dossier n'est pas versionné : sur un
    // clone tout frais, avant le premier fetch, le glob de resources ne
    // matche rien et fait planter TOUT `cargo build`/`cargo tauri dev` —
    // on garantit donc qu'il existe toujours (même vide) pour ne jamais
    // bloquer un build. Aucun appel réseau ici à dessein : c'est
    // `beforeDevCommand`/`beforeBuildCommand` (tauri.conf.json) qui se
    // chargent de la vraie récupération du pack.
    let mcp_dir = Path::new("mcp-servers");
    if !mcp_dir.exists() {
        println!(
            "cargo:warning=src-tauri/mcp-servers/ est absent — lancez ./scripts/fetch-mcp-pack.sh \
             pour récupérer les MCP officiels (voir README). Build avec un dossier vide en attendant."
        );
        std::fs::create_dir_all(mcp_dir).expect("Impossible de créer src-tauri/mcp-servers/");
        std::fs::write(mcp_dir.join(".gitkeep"), "")
            .expect("Impossible de créer mcp-servers/.gitkeep");
    }

    tauri_build::build()
}
