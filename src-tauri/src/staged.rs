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
//
// ── Store de staging des documents décompressés ──────────────────────────────
//
// Une « VFS éphémère » par batch :
//  • Les fichiers (feuilles d'archives OU documents déposés directement) sont
//    SPOOLÉS sur disque dans un TempDir possédé par le batch (RAM ≪ fichiers).
//  • L'index en RAM est un `BTreeMap<String /*id*/, StagedDoc>` ordonné — on
//    peut donc lister les docs dans un ordre stable, en ajouter, et SURTOUT en
//    SUPPRIMER individuellement (fichier disque + entrée d'index) avant
//    ingestion ou purge.
//  • Chaque batch est éphémère : il est purgé par une tâche GC (TTL) ou
//    explicitement (annulation / ingestion terminée) ; le TempDir est alors
//    détruit (RAII).
// ============================================================================

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Un document en staging (contenu réellement sur disque).
#[derive(Debug, Clone)]
pub struct StagedDoc {
    pub id: String,
    /// Nom d'affichage (chemin relatif assaini au sein de l'archive, sinon nom du fichier).
    pub name: String,
    pub ext: String,
    pub size: u64,
    /// Chemin physique du spool.
    pub path: PathBuf,
}

/// Un batch de staging = un dossier temporaire + un index trié de documents.
#[derive(Debug)]
pub struct StagedBatch {
    pub id: String,
    pub dir: tempfile::TempDir,
    pub docs: BTreeMap<String, StagedDoc>,
    pub created: Instant,
    pub touched: Instant,
}

impl StagedBatch {
    fn new(id: String, dir: tempfile::TempDir) -> Self {
        let now = Instant::now();
        Self {
            id,
            dir,
            docs: BTreeMap::new(),
            created: now,
            touched: now,
        }
    }

    pub fn dir(&self) -> &Path {
        self.dir.path()
    }
}

#[derive(Debug, Default)]
pub struct StagedStore {
    pub batches: BTreeMap<String, StagedBatch>,
}

impl StagedStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Crée un batch et retourne `(batch_id, dossier_de_spool)`.
    pub fn create_batch(&mut self) -> Result<(String, PathBuf), String> {
        let id = uuid::Uuid::new_v4().to_string();
        let dir = tempfile::Builder::new()
            .prefix("demeter_batch_")
            .tempdir()
            .map_err(|e| format!("Création dossier de staging impossible : {e}"))?;
        let path = dir.path().to_path_buf();
        self.batches.insert(id.clone(), StagedBatch::new(id.clone(), dir));
        Ok((id, path))
    }

    pub fn get(&self, batch_id: &str) -> Option<&StagedBatch> {
        self.batches.get(batch_id)
    }

    pub fn touch(&mut self, batch_id: &str) {
        if let Some(b) = self.batches.get_mut(batch_id) {
            b.touched = Instant::now();
        }
    }

    /// Enregistre les fichiers déjà écrits sur disque (dans le dossier du batch)
    /// comme documents du batch. Retourne le manifest complet du batch.
    pub fn register_files(
        &mut self,
        batch_id: &str,
        files: Vec<(String, String, u64, PathBuf)>, // (name, ext, size, path)
    ) -> Result<Vec<Value>, String> {
        let batch = self
            .batches
            .get_mut(batch_id)
            .ok_or_else(|| "Batch de staging introuvable ou expiré.".to_string())?;
        batch.touched = Instant::now();
        for (name, ext, size, path) in files {
            let id = uuid::Uuid::new_v4().to_string();
            batch.docs.insert(
                id.clone(),
                StagedDoc {
                    id: id.clone(),
                    name,
                    ext,
                    size,
                    path,
                },
            );
        }
        Ok(Self::manifest(batch))
    }

    /// Supprime un document du staging (fichier + index). Retourne `true` si trouvé.
    pub fn remove_doc(&mut self, batch_id: &str, file_id: &str) -> bool {
        let Some(batch) = self.batches.get_mut(batch_id) else {
            return false;
        };
        batch.touched = Instant::now();
        if let Some(doc) = batch.docs.remove(file_id) {
            let _ = std::fs::remove_file(&doc.path);
            true
        } else {
            false
        }
    }

    /// Annule / purge un batch complet.
    pub fn remove_batch(&mut self, batch_id: &str) -> bool {
        self.batches.remove(batch_id).is_some()
    }

    /// Liste des docs d'un batch (ordre stable par id).
    pub fn batch_manifest(&self, batch_id: &str) -> Option<Vec<Value>> {
        self.batches.get(batch_id).map(Self::manifest)
    }

    /// Supprime les batchs inactifs depuis plus de `ttl`. Retourne le nb purgé.
    pub fn purge_expired(&mut self, ttl: Duration) -> usize {
        let now = Instant::now();
        let expired: Vec<String> = self
            .batches
            .iter()
            .filter(|(_, b)| now.duration_since(b.touched) > ttl)
            .map(|(id, _)| id.clone())
            .collect();
        let n = expired.len();
        for id in expired {
            self.batches.remove(&id);
        }
        n
    }

    fn manifest(batch: &StagedBatch) -> Vec<Value> {
        batch
            .docs
            .values()
            .map(|d| {
                json!({
                    "id": d.id,
                    "name": d.name,
                    "ext": d.ext,
                    "size": d.size,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_store() -> StagedStore {
        StagedStore::new()
    }

    #[test]
    fn add_list_remove() {
        let mut store = with_store();
        let (id, dir) = store.create_batch().unwrap();
        let p1 = dir.join("d1");
        std::fs::write(&p1, b"x").unwrap();
        let files = vec![("a.txt".to_string(), "txt".to_string(), 1, p1)];
        let manifest = store.register_files(&id, files).unwrap();
        assert_eq!(manifest.len(), 1);
        let fid = manifest[0]["id"].as_str().unwrap().to_string();

        assert!(store.remove_doc(&id, &fid));
        assert!(!std::path::Path::new(&dir.join("d1")).exists());
        let m2 = store.batch_manifest(&id).unwrap();
        assert_eq!(m2.len(), 0);
        assert!(store.remove_batch(&id));
    }

    #[test]
    fn purge_ttl() {
        let mut store = with_store();
        let (id, _) = store.create_batch().unwrap();
        assert_eq!(store.purge_expired(Duration::from_secs(0)), 1);
        assert!(store.get(&id).is_none());
    }
}
