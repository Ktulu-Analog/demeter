// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Secrets au repos (constats D-08 et D-09).
//
// - Les secrets de configuration (jeton Albert, clé Qdrant, clés des serveurs
//   MCP…) ne sont plus écrits en clair dans le localStorage de la WebView : ils
//   vivent dans le TROUSSEAU du système (Credential Manager sous Windows,
//   Keychain sous macOS, Secret Service sous Linux).
// - La clé de chiffrement de la base SQLite (SQLCipher) est une valeur aléatoire
//   générée au premier lancement et conservée dans le même trousseau.
//
// Aucun repli silencieux en clair : si le trousseau est indisponible, l'erreur
// est remontée à l'utilisateur (voir `SecurityStatus`). La seule dérogation est
// explicite et choisie par l'utilisateur : DEMETER_DB_KEY (clé fournie) ou
// DEMETER_ALLOW_PLAINTEXT_DB=1 (base non chiffrée, signalée à chaque lancement).
//
// Les identifiants du trousseau sont limités en taille (≈ 2,5 Ko sous Windows) :
// une valeur longue est donc découpée en morceaux de 1000 unités UTF-16.
// ============================================================================

use serde::Serialize;
use std::path::Path;
use tracing::{info, warn};

/// Nom de service dans le trousseau (identique à l'identifiant de l'application).
const SERVICE: &str = "fr.demeter.app";
/// Taille maximale d'un morceau, en unités UTF-16 (≈ 2 Ko : sous la limite Windows).
const CHUNK_UTF16: usize = 1000;
/// Nombre maximal de morceaux acceptés à la lecture (garde-fou : ≈ 4 Mo).
const MAX_CHUNKS: usize = 4096;

/// Valeur « secrets de configuration » (JSON), seule accessible au frontend.
pub const SETTINGS_BLOB: &str = "settings";
/// Clé de chiffrement de la base : jamais exposée au frontend.
pub const DB_KEY_BLOB: &str = "db_key";

// ── Abstraction du magasin (permet de tester sans trousseau réel) ─────────────

pub trait Backend {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// Trousseau réel du système.
pub struct KeyringBackend;

impl KeyringBackend {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
    }
}

impl Backend for KeyringBackend {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }
    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(value)
            .map_err(|e| e.to_string())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

// ── Valeurs longues : découpage en morceaux ───────────────────────────────────

fn split_chunks(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut units = 0usize;
    for c in value.chars() {
        let u = c.len_utf16();
        if units + u > CHUNK_UTF16 {
            out.push(std::mem::take(&mut cur));
            units = 0;
        }
        cur.push(c);
        units += u;
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

fn count_account(name: &str) -> String {
    format!("{name}#n")
}
fn chunk_account(name: &str, i: usize) -> String {
    format!("{name}#{i}")
}

fn read_count<B: Backend>(b: &B, name: &str) -> Result<Option<usize>, String> {
    match b.get(&count_account(name))? {
        None => Ok(None),
        Some(s) => {
            let n: usize = s
                .trim()
                .parse()
                .map_err(|_| "secret corrompu (compteur illisible)".to_string())?;
            if n > MAX_CHUNKS {
                return Err("secret corrompu (compteur excessif)".into());
            }
            Ok(Some(n))
        }
    }
}

/// Enregistre `value` sous `name` (une valeur vide supprime l'entrée).
pub fn set_blob_with<B: Backend>(b: &B, name: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return delete_blob_with(b, name);
    }
    let old = read_count(b, name).unwrap_or(None);
    let chunks = split_chunks(value);
    if chunks.len() > MAX_CHUNKS {
        return Err("valeur trop volumineuse".into());
    }
    for (i, c) in chunks.iter().enumerate() {
        b.set(&chunk_account(name, i), c)?;
    }
    b.set(&count_account(name), &chunks.len().to_string())?;
    if let Some(old) = old {
        for i in chunks.len()..old {
            b.delete(&chunk_account(name, i))?;
        }
    }
    Ok(())
}

/// Lit la valeur enregistrée sous `name` (`None` si absente).
pub fn get_blob_with<B: Backend>(b: &B, name: &str) -> Result<Option<String>, String> {
    let Some(n) = read_count(b, name)? else {
        return Ok(None);
    };
    let mut out = String::new();
    for i in 0..n {
        match b.get(&chunk_account(name, i))? {
            Some(c) => out.push_str(&c),
            None => return Err("secret corrompu (morceau manquant)".into()),
        }
    }
    Ok(Some(out))
}

pub fn delete_blob_with<B: Backend>(b: &B, name: &str) -> Result<(), String> {
    if let Some(n) = read_count(b, name).unwrap_or(None) {
        for i in 0..n {
            b.delete(&chunk_account(name, i))?;
        }
    }
    b.delete(&count_account(name))
}

// Raccourcis sur le trousseau réel
pub fn set_blob(name: &str, value: &str) -> Result<(), String> {
    set_blob_with(&KeyringBackend, name, value)
}
pub fn get_blob(name: &str) -> Result<Option<String>, String> {
    get_blob_with(&KeyringBackend, name)
}

/// Vrai si le trousseau du système répond (écriture, lecture, suppression d'un témoin).
pub fn keychain_available() -> bool {
    let b = KeyringBackend;
    b.set("probe", "ok").is_ok()
        && matches!(b.get("probe"), Ok(Some(v)) if v == "ok")
        && b.delete("probe").is_ok()
}

// ── Clé de la base de données ────────────────────────────────────────────────

#[derive(Debug, PartialEq, Eq)]
pub enum DbKey {
    /// Base chiffrée avec cette clé.
    Key(String),
    /// Base NON chiffrée (dérogation explicite de l'utilisateur).
    Plaintext,
}

/// Génère une clé aléatoire de 256 bits (64 caractères hexadécimaux).
fn generate_db_key() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

/// Détermine la clé de la base (logique pure, testable).
pub fn resolve_db_key_with<B: Backend>(
    b: &B,
    db_path: &Path,
    env_key: Option<String>,
    allow_plaintext: bool,
) -> Result<DbKey, String> {
    // 1) Clé fournie explicitement par l'utilisateur (serveurs sans trousseau).
    if let Some(k) = env_key.filter(|k| k.len() >= 16) {
        return Ok(DbKey::Key(k));
    }

    let existing_encrypted = db_path.exists()
        && std::fs::metadata(db_path)
            .map(|m| m.len() > 0)
            .unwrap_or(false)
        && !crate::db::is_plaintext_sqlite(db_path);

    let fallback = |reason: String| -> Result<DbKey, String> {
        if allow_plaintext {
            warn!("Base NON chiffrée (DEMETER_ALLOW_PLAINTEXT_DB=1) — {reason}");
            Ok(DbKey::Plaintext)
        } else {
            Err(format!(
                "Trousseau du système indisponible ({reason}). Les conversations ne seront \
                 pas enregistrées. Définissez DEMETER_DB_KEY (clé d'au moins 16 caractères) \
                 ou, en dernier recours, DEMETER_ALLOW_PLAINTEXT_DB=1 (base non chiffrée)."
            ))
        }
    };

    match get_blob_with(b, DB_KEY_BLOB) {
        Ok(Some(k)) => Ok(DbKey::Key(k)),
        Ok(None) if existing_encrypted => Err(
            "La base de conversations est chiffrée mais sa clé est introuvable dans le \
             trousseau du système. Restaurez le trousseau, ou fournissez DEMETER_DB_KEY."
                .to_string(),
        ),
        Ok(None) => {
            let key = generate_db_key();
            match set_blob_with(b, DB_KEY_BLOB, &key) {
                Ok(()) => {
                    info!("Clé de chiffrement de la base créée dans le trousseau du système");
                    Ok(DbKey::Key(key))
                }
                Err(e) => fallback(e),
            }
        }
        Err(e) => fallback(e),
    }
}

/// Résout la clé de la base avec le trousseau réel et les variables d'environnement.
pub fn resolve_db_key(db_path: &Path) -> Result<DbKey, String> {
    let env_key = std::env::var("DEMETER_DB_KEY").ok();
    let allow_plaintext = std::env::var("DEMETER_ALLOW_PLAINTEXT_DB")
        .map(|v| v == "1")
        .unwrap_or(false);
    resolve_db_key_with(&KeyringBackend, db_path, env_key, allow_plaintext)
}

// ── État de sécurité exposé au frontend ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Default)]
pub struct SecurityStatus {
    /// La base de conversations est chiffrée (SQLCipher).
    pub db_encrypted: bool,
    /// La base est enregistrée sur disque (faux = mémoire seule, perdue à la fermeture).
    pub db_persistent: bool,
    /// Le trousseau du système répond.
    pub keychain_available: bool,
    /// Messages à présenter à l'utilisateur (erreurs, dérogations).
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Mem {
        m: RefCell<HashMap<String, String>>,
        fail: bool,
    }
    impl Backend for Mem {
        fn get(&self, a: &str) -> Result<Option<String>, String> {
            if self.fail {
                return Err("trousseau absent".into());
            }
            Ok(self.m.borrow().get(a).cloned())
        }
        fn set(&self, a: &str, v: &str) -> Result<(), String> {
            if self.fail {
                return Err("trousseau absent".into());
            }
            self.m.borrow_mut().insert(a.into(), v.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<(), String> {
            if self.fail {
                return Err("trousseau absent".into());
            }
            self.m.borrow_mut().remove(a);
            Ok(())
        }
    }

    #[test]
    fn valeur_longue_aller_retour() {
        let b = Mem::default();
        // 5000 caractères dont des émojis (2 unités UTF-16) et des accents
        let v: String = "été😀x".repeat(1000);
        set_blob_with(&b, "settings", &v).unwrap();
        assert!(b.m.borrow().len() > 3, "la valeur doit être découpée");
        assert!(b
            .m
            .borrow()
            .values()
            .all(|c| c.encode_utf16().count() <= CHUNK_UTF16));
        assert_eq!(
            get_blob_with(&b, "settings").unwrap().as_deref(),
            Some(v.as_str())
        );
    }

    #[test]
    fn reduction_supprime_les_morceaux_restants() {
        let b = Mem::default();
        set_blob_with(&b, "s", &"a".repeat(5000)).unwrap();
        let before = b.m.borrow().len();
        set_blob_with(&b, "s", "court").unwrap();
        assert!(b.m.borrow().len() < before);
        assert_eq!(get_blob_with(&b, "s").unwrap().as_deref(), Some("court"));
    }

    #[test]
    fn vide_supprime_et_absent_donne_none() {
        let b = Mem::default();
        assert_eq!(get_blob_with(&b, "x").unwrap(), None);
        set_blob_with(&b, "x", "v").unwrap();
        set_blob_with(&b, "x", "").unwrap();
        assert_eq!(get_blob_with(&b, "x").unwrap(), None);
        assert!(b.m.borrow().is_empty());
    }

    #[test]
    fn morceau_manquant_est_une_erreur() {
        let b = Mem::default();
        set_blob_with(&b, "s", &"a".repeat(3000)).unwrap();
        b.m.borrow_mut().remove("s#1");
        assert!(get_blob_with(&b, "s").is_err());
    }

    #[test]
    fn cle_de_base_creee_puis_reutilisee() {
        let b = Mem::default();
        let p = std::env::temp_dir().join("demeter-inexistante.db");
        let k1 = resolve_db_key_with(&b, &p, None, false).unwrap();
        let k2 = resolve_db_key_with(&b, &p, None, false).unwrap();
        assert_eq!(k1, k2);
        match k1 {
            DbKey::Key(k) => assert_eq!(k.len(), 64),
            DbKey::Plaintext => panic!("clé attendue"),
        }
    }

    #[test]
    fn trousseau_indisponible_pas_de_repli_silencieux() {
        let b = Mem {
            fail: true,
            ..Default::default()
        };
        let p = std::env::temp_dir().join("demeter-inexistante.db");
        assert!(resolve_db_key_with(&b, &p, None, false).is_err());
        // dérogations explicites seulement
        assert_eq!(
            resolve_db_key_with(&b, &p, None, true).unwrap(),
            DbKey::Plaintext
        );
        let env = Some("une-cle-fournie-par-l-utilisateur".to_string());
        assert!(matches!(
            resolve_db_key_with(&b, &p, env, false),
            Ok(DbKey::Key(_))
        ));
    }
}
