// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
// Année   : 2026
// ============================================================================

// Client MCP — transport stdio.
//
// Un serveur MCP stdio est un exécutable lancé en sous-processus par Demeter.
// La communication se fait par JSON-RPC 2.0, un message par ligne, sur
// stdin (Demeter → serveur) et stdout (serveur → Demeter). Les logs du
// serveur doivent partir sur stderr — Demeter les redirige vers ses propres
// logs sans jamais les interpréter comme du JSON-RPC.
//
// Les serveurs sont découverts dans un répertoire dédié : un sous-dossier
// par serveur, contenant un manifest.json + un binaire par plateforme.
// Ce répertoire peut contenir :
//   - les MCP "officiels" fournis avec Demeter (copiés depuis les resources
//     de l'app au premier lancement) ;
//   - des MCP tiers ajoutés manuellement par l'utilisateur.
//
// Chaque process est démarré à la demande (premier appel) et conservé en vie
// tant qu'il est activé dans les préférences — pas de redémarrage à chaque
// message, ce qui serait inutilement lent.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{info, warn};

// ── Manifest ──────────────────────────────────────────────────────────────────

/// Décrit un paramètre de configuration attendu par un serveur MCP stdio
/// (ex : une clé d'API), tel que le panel MCP doit le présenter à l'utilisateur.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StdioParam {
    /// Nom de la variable d'environnement transmise au process.
    pub key: String,
    /// Libellé affiché dans le panel de préférences.
    pub label: String,
    /// Si vrai, la valeur est affichée masquée (type "password") et jamais
    /// journalisée.
    #[serde(default)]
    pub secret: bool,
    /// Si vrai, le serveur ne peut pas fonctionner correctement sans cette
    /// valeur (affiché comme obligatoire dans le panel).
    #[serde(default)]
    pub required: bool,
    /// Valeur par défaut / placeholder indicatif (non transmise au process
    /// si l'utilisateur ne saisit rien — c'est alors le binaire lui-même
    /// qui applique son propre défaut).
    #[serde(default)]
    pub default: Option<String>,
    /// Texte d'aide optionnel affiché sous le champ.
    #[serde(default)]
    pub help: Option<String>,
}

/// Décrit un serveur MCP stdio disponible dans le répertoire dédié.
/// Fichier `manifest.json` attendu à la racine de chaque sous-dossier :
/// ```json
/// {
///   "id": "mcp-judilibre",
///   "name": "Judilibre",
///   "description": "Recherche de décisions de justice (Cour de cassation, CA, TJ).",
///   "bin": { "linux-x64": "bin/mcp-judilibre-linux-x64",
///            "macos-arm64": "bin/mcp-judilibre-macos-arm64",
///            "macos-x64": "bin/mcp-judilibre-macos-x64",
///            "windows-x64": "bin/mcp-judilibre-windows-x64.exe" },
///   "args": ["--transport", "stdio"],
///   "params": [
///     { "key": "JUDILIBRE_CLIENT_ID",     "label": "Client ID PISTE",     "required": true },
///     { "key": "JUDILIBRE_CLIENT_SECRET", "label": "Client Secret PISTE", "required": true, "secret": true }
///   ]
/// }
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StdioManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Chemins relatifs (au dossier du serveur) du binaire, par plateforme.
    /// Clé attendue : "{os}-{arch}", ex. "linux-x64", "macos-arm64",
    /// "windows-x64". Voir `platform_key()`.
    pub bin: HashMap<String, String>,
    /// Arguments passés au binaire (ex. ["--transport", "stdio"] si le
    /// binaire supporte plusieurs transports).
    #[serde(default)]
    pub args: Vec<String>,
    /// Paramètres de configuration attendus par ce serveur (clés d'API...),
    /// affichés et saisis dans le panel MCP dédié.
    #[serde(default)]
    pub params: Vec<StdioParam>,
}

/// Un serveur découvert = son manifest + le dossier où il vit + s'il fournit
/// un binaire pour la plateforme courante.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveredServer {
    #[serde(flatten)]
    pub manifest: StdioManifest,
    pub dir: PathBuf,
    pub available: bool, // false si aucun binaire pour cette plateforme
}

/// Clé de plateforme courante : "linux-x64", "macos-arm64", "windows-x64"...
pub fn platform_key() -> String {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        _ => "x64",
    };
    format!("{os}-{arch}")
}

/// Scanne le répertoire dédié aux MCP stdio et retourne les serveurs trouvés.
/// Chaque sous-dossier direct contenant un `manifest.json` valide est retenu.
pub fn discover(mcp_dir: &Path) -> Vec<DiscoveredServer> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(mcp_dir) {
        Ok(e) => e,
        Err(_) => return found, // dossier absent → aucun serveur, pas une erreur
    };

    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("manifest.json");
        let text = match std::fs::read_to_string(&manifest_path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let manifest: StdioManifest = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                warn!(
                    "MCP stdio: manifest invalide {}: {}",
                    manifest_path.display(),
                    e
                );
                continue;
            }
        };
        let available = manifest.bin.contains_key(&platform_key());
        found.push(DiscoveredServer {
            manifest,
            dir,
            available,
        });
    }

    found
}

/// Résout le chemin absolu du binaire pour la plateforme courante.
fn resolve_bin(server: &DiscoveredServer) -> Option<PathBuf> {
    server
        .manifest
        .bin
        .get(&platform_key())
        .map(|rel| server.dir.join(rel))
}

// ── Process en vie ───────────────────────────────────────────────────────────

struct RunningProcess {
    child: Child,
    stdin: tokio::process::ChildStdin,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next_id: AtomicU64,
}

/// Gestionnaire de tous les process MCP stdio actifs, partagé dans AppState.
/// Le répertoire scanné est choisi par l'utilisateur depuis le panel MCP —
/// il n'est pas fixé au démarrage de l'application, et peut être changé à
/// tout moment (ce qui arrête proprement les process en cours).
#[derive(Clone)]
pub struct StdioManager {
    mcp_dir: Arc<tokio::sync::RwLock<Option<PathBuf>>>,
    processes: Arc<Mutex<HashMap<String, Arc<Mutex<RunningProcess>>>>>,
}

impl StdioManager {
    pub fn new() -> Self {
        Self {
            mcp_dir: Arc::new(tokio::sync::RwLock::new(None)),
            processes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Répertoire actuellement configuré, s'il y en a un.
    pub async fn dir(&self) -> Option<PathBuf> {
        self.mcp_dir.read().await.clone()
    }

    /// Change le répertoire scanné. Arrête tous les process en cours (ils
    /// correspondaient à l'ancien répertoire) avant de basculer.
    pub async fn set_dir(&self, dir: Option<PathBuf>) {
        let changed = { self.mcp_dir.read().await.as_ref() != dir.as_ref() };
        if changed {
            self.stop_all().await;
            *self.mcp_dir.write().await = dir;
        }
    }

    pub async fn discover(&self) -> Vec<DiscoveredServer> {
        match self.dir().await {
            Some(dir) => discover(&dir),
            None => vec![],
        }
    }

    /// Copie les MCP officiels de Demeter (fournis en resources de l'app)
    /// dans le répertoire actuellement configuré. Un MCP déjà présent n'est
    /// jamais écrasé (pour ne pas perdre une configuration existante) —
    /// seuls `manifest.json` et `bin/` y sont rafraîchis.
    pub async fn install_official(&self, resource_dir: &Path) -> Result<Vec<String>, String> {
        let target = self
            .dir()
            .await
            .ok_or("Aucun dossier MCP configuré — choisissez-en un d'abord.")?;

        if !resource_dir.exists() {
            return Err("Aucun MCP officiel embarqué dans cette build de Demeter.".to_string());
        }

        let mut installed = Vec::new();
        let entries = std::fs::read_dir(resource_dir)
            .map_err(|e| format!("Lecture du dossier des MCP officiels impossible : {e}"))?;

        for entry in entries.flatten() {
            let src = entry.path();
            if !src.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let dst = target.join(&name);

            if !dst.exists() {
                copy_dir_recursive(&src, &dst)
                    .map_err(|e| format!("Copie de '{}' échouée : {e}", name.to_string_lossy()))?;
                info!(
                    "MCP stdio: '{}' installé dans {}",
                    name.to_string_lossy(),
                    dst.display()
                );
            } else {
                let src_manifest = src.join("manifest.json");
                if src_manifest.exists() {
                    let _ = std::fs::copy(&src_manifest, dst.join("manifest.json"));
                }
                let src_bin = src.join("bin");
                if src_bin.exists() {
                    let _ = copy_dir_recursive(&src_bin, &dst.join("bin"));
                }
            }
            installed.push(name.to_string_lossy().to_string());
        }

        // Trace la version du pack qui vient d'être installée, pour pouvoir
        // proposer une mise à jour plus tard (voir mcp_pack.rs). Le fichier
        // VERSION à la racine de `resource_dir` est déposé par le script de
        // récupération du pack (scripts/fetch-mcp-pack.sh) — absent en dev.
        let src_version = resource_dir.join("VERSION");
        if let Ok(v) = std::fs::read_to_string(&src_version) {
            let _ = std::fs::write(target.join(".pack-version"), v.trim());
        }

        Ok(installed)
    }

    /// Version du pack MCP officiel actuellement installée dans le dossier
    /// configuré (fichier `.pack-version`, déposé par `install_official`).
    /// `None` si jamais installé (dossier vide, MCP ajoutés manuellement…).
    pub async fn installed_pack_version(&self) -> Option<String> {
        let dir = self.dir().await?;
        std::fs::read_to_string(dir.join(".pack-version"))
            .ok()
            .map(|v| v.trim().to_string())
    }

    /// Arrête proprement tous les process (à appeler à la fermeture de l'app
    /// ou quand le répertoire configuré change).
    pub async fn stop_all(&self) {
        let mut procs = self.processes.lock().await;
        for (id, proc) in procs.drain() {
            let mut p = proc.lock().await;
            let _ = p.child.start_kill();
            info!("MCP stdio: process '{}' arrêté", id);
        }
    }

    /// Désactive/arrête un serveur précis (ex : décoché dans les préférences,
    /// ou reconfiguré — dans ce cas il redémarre avec la nouvelle config au
    /// prochain appel).
    pub async fn stop(&self, id: &str) {
        let mut procs = self.processes.lock().await;
        if let Some(proc) = procs.remove(id) {
            let mut p = proc.lock().await;
            let _ = p.child.start_kill();
            info!("MCP stdio: process '{}' arrêté", id);
        }
    }

    /// Démarre (si nécessaire) et retourne le process pour `id`, avec un
    /// éventuel jeu de variables d'environnement utilisateur (clés API...).
    async fn ensure_started(
        &self,
        id: &str,
        env_overrides: &HashMap<String, String>,
    ) -> Result<Arc<Mutex<RunningProcess>>, String> {
        {
            let procs = self.processes.lock().await;
            if let Some(p) = procs.get(id) {
                return Ok(p.clone());
            }
        }

        let servers = self.discover().await;
        let server = servers
            .into_iter()
            .find(|s| s.manifest.id == id)
            .ok_or_else(|| format!("Serveur MCP stdio inconnu : {id}"))?;

        if !server.available {
            return Err(format!(
                "Aucun binaire disponible pour '{}' sur cette plateforme ({})",
                id,
                platform_key()
            ));
        }

        let bin = resolve_bin(&server)
            .ok_or_else(|| format!("Binaire introuvable pour le serveur '{id}'"))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&bin) {
                let mut perms = meta.permissions();
                if perms.mode() & 0o111 == 0 {
                    perms.set_mode(perms.mode() | 0o111);
                    let _ = std::fs::set_permissions(&bin, perms);
                }
            }
        }

        info!("MCP stdio: démarrage de '{}' — {}", id, bin.display());

        let mut cmd = Command::new(&bin);
        cmd.args(&server.manifest.args)
            .current_dir(&server.dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        for param in &server.manifest.params {
            if let Some(val) = env_overrides.get(&param.key) {
                if !val.trim().is_empty() {
                    cmd.env(&param.key, val);
                }
            }
        }

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("Impossible de lancer '{id}' : {e}"))?;

        let stdin = child.stdin.take().ok_or("stdin indisponible")?;
        let stdout = child.stdout.take().ok_or("stdout indisponible")?;
        let stderr = child.stderr.take().ok_or("stderr indisponible")?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Tâche de lecture stdout → route chaque réponse JSON-RPC vers le
        // oneshot correspondant (via son "id").
        let pending_reader = pending.clone();
        let server_id = id.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let val: Value = match serde_json::from_str(trimmed) {
                            Ok(v) => v,
                            Err(e) => {
                                warn!("MCP stdio '{}': ligne non-JSON ignorée: {}", server_id, e);
                                continue;
                            }
                        };
                        let resp_id = val.get("id").and_then(|v| v.as_u64());
                        if let Some(rid) = resp_id {
                            let mut map = pending_reader.lock().await;
                            if let Some(tx) = map.remove(&rid) {
                                let _ = tx.send(val);
                            }
                        }
                    }
                    Ok(None) => {
                        info!("MCP stdio '{}': stdout fermé (process terminé)", server_id);
                        break;
                    }
                    Err(e) => {
                        warn!("MCP stdio '{}': erreur lecture stdout: {}", server_id, e);
                        break;
                    }
                }
            }
        });

        // Tâche de lecture stderr → simple relais vers les logs Demeter.
        let server_id_err = id.to_string();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                info!("MCP stdio '{}' (stderr): {}", server_id_err, line);
            }
        });

        let running = Arc::new(Mutex::new(RunningProcess {
            child,
            stdin,
            pending,
            next_id: AtomicU64::new(1),
        }));

        // Handshake MCP minimal (initialize + notifications/initialized).
        // Best-effort : certains serveurs répondent correctement dès
        // tools/list sans initialize, on ne bloque donc pas dessus.
        let init_body = json!({
            "jsonrpc": "2.0",
            "id": 0,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "demeter", "version": env!("CARGO_PKG_VERSION") }
            }
        });
        let _ = rpc_call(&running, &init_body, 10).await;
        let notif = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
        let _ = write_line(&running, &notif).await;

        self.processes
            .lock()
            .await
            .insert(id.to_string(), running.clone());

        Ok(running)
    }

    pub async fn list_tools(
        &self,
        id: &str,
        env_overrides: &HashMap<String, String>,
    ) -> (Vec<Value>, bool) {
        let running = match self.ensure_started(id, env_overrides).await {
            Ok(r) => r,
            Err(e) => {
                warn!("MCP stdio '{}': {}", id, e);
                return (vec![], false);
            }
        };

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        });

        match rpc_call(&running, &body, 10).await {
            Some(data) => {
                if let Some(tools) = data["result"]["tools"].as_array() {
                    return (tools.clone(), true);
                }
                if let Some(err) = data.get("error") {
                    warn!("MCP stdio '{}' tools/list error: {}", id, err);
                }
                (vec![], false)
            }
            None => {
                warn!("MCP stdio '{}': pas de réponse à tools/list", id);
                (vec![], false)
            }
        }
    }

    pub async fn call_tool(
        &self,
        id: &str,
        tool_name: &str,
        arguments: &Value,
        env_overrides: &HashMap<String, String>,
    ) -> crate::mcp::McpToolResult {
        let running = match self.ensure_started(id, env_overrides).await {
            Ok(r) => r,
            Err(e) => {
                return crate::mcp::McpToolResult {
                    text: format!("Erreur : impossible de démarrer le serveur MCP '{id}' : {e}"),
                    image_b64: None,
                    image_mime: None,
                }
            }
        };

        let body = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": tool_name, "arguments": arguments }
        });

        match rpc_call(&running, &body, 30).await {
            Some(data) => {
                if let Some(err) = data.get("error") {
                    return crate::mcp::McpToolResult {
                        text: format!("Erreur MCP : {}", err),
                        image_b64: None,
                        image_mime: None,
                    };
                }
                extract_content(&data["result"])
            }
            None => crate::mcp::McpToolResult {
                text: format!("Erreur : pas de réponse du serveur MCP '{id}' pour {tool_name}"),
                image_b64: None,
                image_mime: None,
            },
        }
    }
}

/// Envoie une requête JSON-RPC et attend la réponse correspondante (via id).
async fn rpc_call(
    running: &Arc<Mutex<RunningProcess>>,
    body: &Value,
    timeout_secs: u64,
) -> Option<Value> {
    let (id, rx) = {
        let mut proc = running.lock().await;
        let id = proc.next_id.fetch_add(1, Ordering::SeqCst);
        let mut body = body.clone();
        body["id"] = json!(id);
        let (tx, rx) = oneshot::channel();
        proc.pending.lock().await.insert(id, tx);

        let mut line = serde_json::to_string(&body).ok()?;
        line.push('\n');
        if let Err(e) = proc.stdin.write_all(line.as_bytes()).await {
            warn!("MCP stdio: écriture stdin échouée: {}", e);
            proc.pending.lock().await.remove(&id);
            return None;
        }
        let _ = proc.stdin.flush().await;
        (id, rx)
    };

    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), rx).await {
        Ok(Ok(val)) => Some(val),
        _ => {
            // Timeout ou canal fermé : on nettoie l'entrée en attente.
            let proc = running.lock().await;
            proc.pending.lock().await.remove(&id);
            None
        }
    }
}

/// Envoie une notification JSON-RPC (sans attendre de réponse).
async fn write_line(running: &Arc<Mutex<RunningProcess>>, body: &Value) -> Option<()> {
    let mut proc = running.lock().await;
    let mut line = serde_json::to_string(body).ok()?;
    line.push('\n');
    proc.stdin.write_all(line.as_bytes()).await.ok()?;
    proc.stdin.flush().await.ok()
}

/// Extrait le contenu d'une réponse `tools/call` (texte + image optionnelle).
/// Même format que le transport HTTP.
fn extract_content(data: &Value) -> crate::mcp::McpToolResult {
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

        return crate::mcp::McpToolResult {
            text: texts.join("\n"),
            image_b64,
            image_mime,
        };
    }
    crate::mcp::McpToolResult {
        text: data.to_string(),
        image_b64: None,
        image_mime: None,
    }
}

/// Copie récursivement un dossier (utilisé par `install_official`).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
