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
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{oneshot, Mutex};
use tracing::{info, warn};

use sha2::{Digest, Sha256};

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
    let rel = server.manifest.bin.get(&platform_key())?;
    bin_within_dir(&server.dir, rel)
}

/// D-12 : le binaire doit rester DANS le dossier du serveur. Sont refusés : les
/// chemins absolus, tout composant « .. » et les liens symboliques qui sortent
/// du dossier (le manifest vient d'un dossier choisi par l'utilisateur et ne doit
/// pas pouvoir faire exécuter un binaire arbitraire du système).
fn bin_within_dir(dir: &Path, rel: &str) -> Option<PathBuf> {
    use std::path::Component;
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        warn!("MCP stdio: chemin de binaire refusé (hors dossier) : {rel}");
        return None;
    }
    let candidate = dir.join(rel_path);
    if let (Ok(real_dir), Ok(real_bin)) = (dir.canonicalize(), candidate.canonicalize()) {
        if !real_bin.starts_with(&real_dir) {
            warn!("MCP stdio: le binaire sort du dossier du serveur : {rel}");
            return None;
        }
    }
    Some(candidate)
}

/// Variables d'environnement transmises aux serveurs MCP (D-12). Tout le reste
/// (clés API de Demeter, QDRANT_API_KEY, jetons…) n'est PAS hérité.
const PASS_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "TEMP",
    "TMP",
    "TMPDIR",
    "SYSTEMROOT",
    "SystemRoot",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "LANG",
    "LC_ALL",
    "TZ",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "REQUESTS_CA_BUNDLE",
];

/// Une variable déclarée par un manifest est acceptée si son nom est de la forme
/// MAJUSCULES_CHIFFRES_ET_SOULIGNÉS et n'est pas une variable sensible du système,
/// d'un interpréteur, d'un runtime ou du réseau (injection de code, détournement
/// du trafic ou de la confiance TLS par variable d'environnement).
///
/// Un serveur MCP n'a besoin que de variables « métier » (clés d'API, identifiants) :
/// tout nom appartenant aux familles ci-dessous est refusé, ainsi que toute variable
/// que Demeter transmet déjà elle-même (`PASS_ENV`, comparaison insensible à la casse).
fn is_safe_env_key(key: &str) -> bool {
    let well_formed = !key.is_empty()
        && key.len() <= 64
        && key.starts_with(|c: char| c.is_ascii_uppercase())
        && key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    if !well_formed {
        return false;
    }
    const DENY_EXACT: &[&str] = &[
        "IFS",
        "ENV",
        "SHELL",
        "BASH_ENV",
        "GCONV_PATH",
        "LOCPATH",
        "NLSPATH",
        "SSLKEYLOGFILE",
        "CLASSPATH",
        "CURL_CA_BUNDLE",
        "OPENSSL_CONF",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "GOPATH",
        "GOROOT",
        "GOFLAGS",
        "GOPROXY",
        "GOENV",
        "GOTOOLCHAIN",
    ];
    // Familles de variables : interpréteurs, runtimes, chargeur dynamique, outils.
    const DENY_PREFIXES: &[&str] = &[
        "LD_", "DYLD_", "PYTHON", "NODE_", "NPM_", "JAVA", "_JAVA", "JDK_", "DOTNET_", "COR_",
        "CORECLR_", "GIT_", "PERL", "RUBY", "LUA_", "BASH_", "OPENSSL_", "SSL_", "CARGO_", "RUST",
        "PIP_", "UV_",
    ];
    let upper = key.to_ascii_uppercase();
    if PASS_ENV.iter().any(|p| p.eq_ignore_ascii_case(&upper)) {
        return false;
    }
    if DENY_EXACT.contains(&upper.as_str()) {
        return false;
    }
    !DENY_PREFIXES.iter().any(|p| upper.starts_with(p))
}

/// Lit une ligne en plafonnant sa taille (D-12) : une ligne sans fin ne peut plus
/// faire croître la mémoire sans limite.
async fn read_line_capped<R>(
    reader: &mut BufReader<R>,
    max: usize,
) -> std::io::Result<Option<String>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::{AsyncBufReadExt, AsyncReadExt};
    let mut buf = Vec::new();
    let n = reader
        .take((max + 1) as u64)
        .read_until(b'\n', &mut buf)
        .await?;
    if n == 0 {
        return Ok(None);
    }
    if buf.len() > max && buf.last() != Some(&b'\n') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ligne trop longue",
        ));
    }
    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
}

/// Taille maximale d'une ligne JSON-RPC sur stdout (les images base64 sont grosses).
const MAX_STDOUT_LINE: usize = 32 * 1024 * 1024;
/// Taille maximale d'une ligne de stderr relayée dans les journaux.
const MAX_STDERR_LINE: usize = 64 * 1024;

// ── Autorisation d'un binaire : empreintes (constat F-03) ────────────────────
//
// Un serveur MCP stdio est un exécutable lancé avec les droits de l'utilisateur.
// Demeter enregistre, au moment où l'utilisateur AUTORISE un serveur, l'empreinte
// SHA-256 de son binaire et de son `manifest.json` (qui fixe les arguments). Elle est
// stockée dans le trousseau du système (nom d'entrée fixe, illisible par le frontend).
// Avant CHAQUE démarrage, les empreintes sont recalculées et comparées :
//   - aucune empreinte  → serveur non autorisé : il n'est pas lancé ;
//   - empreinte modifiée → binaire ou manifeste altéré (ou mis à jour) : il n'est pas
//     lancé tant que l'utilisateur ne l'a pas de nouveau autorisé.
// Les mises à jour du pack officiel (signé) ré-enregistrent automatiquement les
// empreintes des serveurs DÉJÀ autorisés.
//
// Limites assumées : seuls le binaire désigné et le manifeste sont couverts (un serveur
// peut écrire dans son dossier à l'exécution) ; l'intervalle entre le contrôle et le
// lancement n'est pas atomique ; le processus n'est pas confiné (voir docs).

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrustRecord {
    pub bin_sha256: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrustStatus {
    /// Autorisé, empreintes identiques.
    Trusted,
    /// Jamais autorisé.
    Untrusted,
    /// Autorisé, mais le binaire ou le manifeste a changé depuis.
    Modified,
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 d'un fichier, lu par blocs (pas de chargement complet en mémoire).
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

/// Empreintes courantes d'un serveur : binaire résolu + manifest.json.
pub fn fingerprint(server_dir: &Path, bin: &Path) -> Result<TrustRecord, String> {
    let bin_sha256 = sha256_file(bin)
        .map_err(|e| format!("lecture du binaire impossible ({}) : {e}", bin.display()))?;
    let manifest = server_dir.join("manifest.json");
    let manifest_sha256 = sha256_file(&manifest)
        .map_err(|e| format!("lecture du manifeste impossible ({}) : {e}", manifest.display()))?;
    Ok(TrustRecord {
        bin_sha256,
        manifest_sha256,
    })
}

/// Compare l'empreinte enregistrée à l'empreinte courante (logique pure, testable).
pub fn evaluate_trust(recorded: Option<&TrustRecord>, current: &TrustRecord) -> TrustStatus {
    match recorded {
        None => TrustStatus::Untrusted,
        Some(r) if r == current => TrustStatus::Trusted,
        Some(_) => TrustStatus::Modified,
    }
}

fn load_trust_blocking() -> HashMap<String, TrustRecord> {
    match crate::secrets::get_blob(crate::secrets::MCP_TRUST_BLOB) {
        Ok(Some(json)) => serde_json::from_str(&json).unwrap_or_else(|e| {
            warn!("MCP stdio: registre d'autorisations illisible, ignoré : {e}");
            HashMap::new()
        }),
        Ok(None) => HashMap::new(),
        Err(e) => {
            warn!("MCP stdio: registre d'autorisations inaccessible (trousseau) : {e}");
            HashMap::new()
        }
    }
}

fn save_trust_blocking(map: &HashMap<String, TrustRecord>) -> Result<(), String> {
    let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
    crate::secrets::set_blob(crate::secrets::MCP_TRUST_BLOB, &json)
}

#[derive(Default)]
struct TrustState {
    loaded: bool,
    map: HashMap<String, TrustRecord>,
}

/// Enregistre le registre dans le trousseau. En cas d'échec, l'autorisation reste valable
/// pour la session en cours (mémoire) mais devra être redonnée au prochain lancement.
async fn persist_trust(map: HashMap<String, TrustRecord>) {
    match tokio::task::spawn_blocking(move || save_trust_blocking(&map)).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!("MCP stdio: autorisations non persistées (trousseau) : {e}"),
        Err(e) => warn!("MCP stdio: autorisations non persistées : {e}"),
    }
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
    /// Empreintes des serveurs autorisés (F-03), chargées paresseusement du trousseau.
    trust: Arc<Mutex<TrustState>>,
}

impl Default for StdioManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioManager {
    pub fn new() -> Self {
        Self {
            mcp_dir: Arc::new(tokio::sync::RwLock::new(None)),
            processes: Arc::new(Mutex::new(HashMap::new())),
            trust: Arc::new(Mutex::new(TrustState::default())),
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

    // ── Autorisation des binaires (F-03) ──────────────────────────────────────

    async fn ensure_trust_loaded(&self) {
        let mut st = self.trust.lock().await;
        if !st.loaded {
            st.map = tokio::task::spawn_blocking(load_trust_blocking)
                .await
                .unwrap_or_default();
            st.loaded = true;
        }
    }

    /// Empreintes courantes d'un serveur découvert (calcul bloquant déporté).
    async fn current_fingerprint(server: &DiscoveredServer) -> Result<TrustRecord, String> {
        let bin = resolve_bin(server).ok_or_else(|| {
            format!("Binaire introuvable ou hors du dossier du serveur '{}'", server.manifest.id)
        })?;
        let dir = server.dir.clone();
        tokio::task::spawn_blocking(move || fingerprint(&dir, &bin))
            .await
            .map_err(|e| e.to_string())?
    }

    /// État d'autorisation d'un serveur découvert.
    pub async fn trust_status_of(&self, server: &DiscoveredServer) -> TrustStatus {
        let Ok(current) = Self::current_fingerprint(server).await else {
            return TrustStatus::Untrusted;
        };
        self.ensure_trust_loaded().await;
        let st = self.trust.lock().await;
        evaluate_trust(st.map.get(&server.manifest.id), &current)
    }

    /// Serveurs découverts avec leur état d'autorisation (pour le panneau MCP).
    pub async fn discover_with_trust(&self) -> Vec<(DiscoveredServer, TrustStatus)> {
        let mut out = Vec::new();
        for srv in self.discover().await {
            let status = if srv.available {
                self.trust_status_of(&srv).await
            } else {
                TrustStatus::Untrusted
            };
            out.push((srv, status));
        }
        out
    }

    /// Autorise un serveur : enregistre les empreintes de son binaire et de son manifeste
    /// telles qu'elles sont MAINTENANT. À appeler uniquement sur action explicite de l'utilisateur.
    pub async fn trust_server(&self, id: &str) -> Result<(), String> {
        let server = self
            .discover()
            .await
            .into_iter()
            .find(|s| s.manifest.id == id)
            .ok_or_else(|| format!("Serveur MCP stdio inconnu : {id}"))?;
        if !server.available {
            return Err(format!("Aucun binaire disponible pour '{id}' sur cette plateforme"));
        }
        let current = Self::current_fingerprint(&server).await?;
        self.ensure_trust_loaded().await;
        let snapshot = {
            let mut st = self.trust.lock().await;
            st.map.insert(id.to_string(), current);
            st.map.clone()
        };
        persist_trust(snapshot).await;
        info!("MCP stdio: serveur '{}' autorisé (empreintes enregistrées)", id);
        Ok(())
    }

    /// Retire l'autorisation d'un serveur et arrête son processus.
    pub async fn untrust_server(&self, id: &str) {
        self.ensure_trust_loaded().await;
        let snapshot = {
            let mut st = self.trust.lock().await;
            st.map.remove(id);
            st.map.clone()
        };
        persist_trust(snapshot).await;
        self.stop(id).await;
        info!("MCP stdio: autorisation de '{}' retirée", id);
    }

    /// Contrôle exécuté avant chaque démarrage : le serveur doit être autorisé ET inchangé.
    async fn verify_trust(&self, server: &DiscoveredServer) -> Result<(), String> {
        #[cfg(debug_assertions)]
        if std::env::var("DEMETER_MCP_SKIP_TRUST").map(|v| v == "1").unwrap_or(false) {
            warn!("MCP stdio: contrôle d'autorisation désactivé (DEMETER_MCP_SKIP_TRUST=1, debug)");
            return Ok(());
        }
        let id = &server.manifest.id;
        match self.trust_status_of(server).await {
            TrustStatus::Trusted => Ok(()),
            TrustStatus::Untrusted => Err(format!(
                "Le serveur MCP '{id}' n'est pas autorisé. Autorisez-le dans le panneau « Serveurs MCP »."
            )),
            TrustStatus::Modified => Err(format!(
                "Le binaire ou le manifeste du serveur MCP '{id}' a changé depuis son autorisation \
                 (mise à jour ou altération). Vérifiez-le puis autorisez-le de nouveau dans le \
                 panneau « Serveurs MCP »."
            )),
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

        // F-03 : les serveurs DÉJÀ autorisés le restent après une installation/mise à jour du
        // pack officiel (contenu embarqué dans l'application, ou pack dont la signature a été
        // vérifiée par `mcp_pack`) : leurs empreintes sont ré-enregistrées. Un serveur jamais
        // autorisé reste non autorisé.
        self.ensure_trust_loaded().await;
        let previously: std::collections::HashSet<String> =
            self.trust.lock().await.map.keys().cloned().collect();
        if !previously.is_empty() {
            let mut refreshed: Vec<(String, TrustRecord)> = Vec::new();
            for srv in discover(&target) {
                let dir_name = srv
                    .dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if srv.available
                    && installed.contains(&dir_name)
                    && previously.contains(&srv.manifest.id)
                {
                    if let Ok(cur) = Self::current_fingerprint(&srv).await {
                        refreshed.push((srv.manifest.id.clone(), cur));
                    }
                }
            }
            if !refreshed.is_empty() {
                let snapshot = {
                    let mut st = self.trust.lock().await;
                    for (id, rec) in refreshed {
                        st.map.insert(id, rec);
                    }
                    st.map.clone()
                };
                persist_trust(snapshot).await;
            }
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

        // F-03 : le serveur doit avoir été autorisé par l'utilisateur et être inchangé.
        self.verify_trust(&server).await?;

        // F-03 : on ne force plus le bit exécutable. Un fichier non exécutable est signalé
        // (l'ancien comportement rendait exécutable n'importe quel fichier désigné par un manifeste).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&bin) {
                if meta.permissions().mode() & 0o111 == 0 {
                    return Err(format!(
                        "Le binaire de '{id}' n'est pas exécutable ({}). Vérifiez son origine puis \
                         rendez-le exécutable (chmod +x) si vous lui faites confiance.",
                        bin.display()
                    ));
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

        // D-12 : environnement minimal. On repart d'un environnement VIDE et on ne
        // transmet que la liste blanche ci-dessus + les paramètres déclarés.
        cmd.env_clear();
        for key in PASS_ENV {
            if let Some(v) = std::env::var_os(key) {
                cmd.env(key, v);
            }
        }
        for param in &server.manifest.params {
            if !is_safe_env_key(&param.key) {
                warn!(
                    "MCP stdio '{}': variable '{}' refusée (nom non autorisé)",
                    id, param.key
                );
                continue;
            }
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
            let mut reader = BufReader::new(stdout);
            loop {
                match read_line_capped(&mut reader, MAX_STDOUT_LINE).await {
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
            let mut reader = BufReader::new(stderr);
            while let Ok(Some(line)) = read_line_capped(&mut reader, MAX_STDERR_LINE).await {
                // D-12/D-20 : niveau debug (stderr peut contenir des secrets)
                tracing::debug!(
                    "MCP stdio '{}' (stderr): {}",
                    server_id_err,
                    line.trim_end()
                );
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
        // D-12 : les liens symboliques ne sont jamais suivis ni copiés
        let ftype = entry.file_type()?;
        if ftype.is_symlink() {
            continue;
        }
        if ftype.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binaire_confine_au_dossier() {
        let dir = std::env::temp_dir();
        assert!(bin_within_dir(&dir, "bin/serveur").is_some());
        assert!(bin_within_dir(&dir, "./bin/serveur").is_some());
        assert!(bin_within_dir(&dir, "../evil").is_none());
        assert!(bin_within_dir(&dir, "bin/../../evil").is_none());
        #[cfg(unix)]
        assert!(bin_within_dir(&dir, "/bin/sh").is_none());
        #[cfg(windows)]
        assert!(bin_within_dir(&dir, "C:\\Windows\\System32\\cmd.exe").is_none());
    }

    #[test]
    fn variables_d_environnement_dangereuses_refusees() {
        for bad in [
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "SSL_CERT_FILE",
            "NODE_EXTRA_CA_CERTS",
            "REQUESTS_CA_BUNDLE",
            "NODE_PATH",
            "PYTHONWARNINGS",
            "DOTNET_STARTUP_HOOKS",
            "GIT_SSH_COMMAND",
            "OPENSSL_CONF",
            "GOFLAGS",
            "TMPDIR",
            "APPDATA",
            "LD_LIBRARY_PATH",
            "RUBYOPT",
            "PERL5OPT",
        ] {
            assert!(!is_safe_env_key(bad), "{bad} devrait être refusée");
        }
        for ok in ["GRIST_API_KEY", "GOOGLE_API_KEY", "PISTE_CLIENT_SECRET", "MCP_BASE_URL"] {
            assert!(is_safe_env_key(ok), "{ok} devrait être acceptée");
        }
    }

    // ── F-03 : empreintes ────────────────────────────────────────────────────

    fn tmp_server_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("demeter-trust-{tag}-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(d.join("bin")).unwrap();
        std::fs::write(d.join("manifest.json"), b"{\"id\":\"mcp-x\"}").unwrap();
        std::fs::write(d.join("bin/srv"), b"binaire v1").unwrap();
        d
    }

    #[test]
    fn sha256_connu() {
        let d = tmp_server_dir("sha");
        let f = d.join("vide");
        std::fs::write(&f, b"").unwrap();
        assert_eq!(
            sha256_file(&f).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let g = d.join("abc");
        std::fs::write(&g, b"abc").unwrap();
        assert_eq!(
            sha256_file(&g).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn autorisation_detecte_modification_du_binaire_et_du_manifeste() {
        let d = tmp_server_dir("trust");
        let bin = d.join("bin/srv");
        let rec = fingerprint(&d, &bin).unwrap();

        // jamais autorisé
        assert_eq!(evaluate_trust(None, &rec), TrustStatus::Untrusted);
        // autorisé et inchangé
        assert_eq!(evaluate_trust(Some(&rec), &fingerprint(&d, &bin).unwrap()), TrustStatus::Trusted);

        // binaire remplacé
        std::fs::write(&bin, b"binaire v2 malveillant").unwrap();
        assert_eq!(evaluate_trust(Some(&rec), &fingerprint(&d, &bin).unwrap()), TrustStatus::Modified);

        // retour à l'original → de nouveau autorisé
        std::fs::write(&bin, b"binaire v1").unwrap();
        assert_eq!(evaluate_trust(Some(&rec), &fingerprint(&d, &bin).unwrap()), TrustStatus::Trusted);

        // manifeste modifié (arguments détournés)
        std::fs::write(d.join("manifest.json"), b"{\"id\":\"mcp-x\",\"args\":[\"--evil\"]}").unwrap();
        assert_eq!(evaluate_trust(Some(&rec), &fingerprint(&d, &bin).unwrap()), TrustStatus::Modified);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn empreinte_impossible_sans_binaire() {
        let d = tmp_server_dir("nobin");
        assert!(fingerprint(&d, &d.join("bin/absent")).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn variables_d_environnement_sures() {
        assert!(is_safe_env_key("GRIST_API_KEY"));
        assert!(is_safe_env_key("PISTE_CLIENT_ID"));
        for bad in [
            "PATH",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "NODE_OPTIONS",
            "path",
            "",
            "A-B",
            "1A",
            "PYTHONPATH",
        ] {
            assert!(!is_safe_env_key(bad), "{bad}");
        }
    }
}
