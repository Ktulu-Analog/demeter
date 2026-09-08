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
// ── Expansion d'archives (.zip / .7z) sûre en mémoire ────────────────────────
//
// Principe :
//  • Les entrées sont décompressées UNE PAR UNE et STREAMÉES vers le disque
//    (jamais tout le contenu en RAM) dans un dossier de travail fourni par
//    l'appelant (fichiers à plat, nommés e00001…, sans recréer les arborescences
//    — ce qui élimine tout risque de Zip-Slip sur le chemin d'écriture).
//  • Un `Budget` (garde-fous) bloque : la profondeur de récursion (archives
//    imbriquées), le nombre d'entrées, la taille décompressée par entrée et par
//    batch, et le ratio de compression déclaré (anti « zip bomb »). Un dépassement
//    des quotas « durs » annule TOUTE l'expansion (retour Err) : on ne décompresse
//    jamais plus que la limite, même en cas d'en-tête mensonger.
//  • Le point d'appel central `expand_archive` dispatche sur l'extension ;
//    pour ajouter un format (rar, tar.gz…) il suffit d'ajouter l'extension à
//    `ARCHIVE_EXTS` et d'ajouter un bras dans `expand_archive` + une fonction
//    dédiée (« un helper + un appel »).
// ============================================================================

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Extensions d'archives reconnues. Ajoutez ici + un bras dans `expand_archive`.
pub const ARCHIVE_EXTS: [&str; 2] = ["zip", "7z"];

/// Extensions de documents extractibles (feuilles d'une archive).
/// NB : `docx` est lui-même un conteneur zip mais reste un DOCUMENT (jamais une
/// récursion d'archive) — l'expansion s'arrête dessus.
const DOC_EXTS: [&str; 8] = ["pdf", "docx", "doc", "txt", "md", "markdown", "rst", "csv"];

const STREAM_BUF: usize = 128 * 1024;

pub fn is_archive_ext(ext: &str) -> bool {
    ARCHIVE_EXTS.contains(&ext)
}

pub fn is_doc_ext(ext: &str) -> bool {
    DOC_EXTS.contains(&ext)
}

// ── Limites / budget ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Limits {
    pub max_depth: usize,
    pub max_entries: usize,
    pub max_entry_bytes: u64,
    pub max_total_bytes: u64,
    pub max_ratio: u64,
}

impl Limits {
    pub fn from_config() -> Self {
        Self {
            max_depth: crate::config::archive_max_depth(),
            max_entries: crate::config::archive_max_entries(),
            max_entry_bytes: crate::config::archive_max_entry_bytes(),
            max_total_bytes: crate::config::archive_max_total_bytes(),
            max_ratio: crate::config::archive_max_ratio(),
        }
    }
}

/// Mémoire vive disponible (octets) — 0 si non déterminable.
pub fn available_memory_bytes() -> u64 {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let avail = sys.available_memory();
    if avail == 0 { 0 } else { avail }
}

/// Plafond « par entrée » adapté à la RAM : jamais plus d'une fraction de la
/// mémoire disponible (le fichier devra tenir en RAM pour être extrait ensuite).
pub fn effective_entry_cap(config_cap: u64) -> u64 {
    let avail = available_memory_bytes();
    if avail == 0 {
        return config_cap;
    }
    // On ne charge jamais plus d'un quart de la RAM dispo à la fois.
    config_cap.min(avail / 4).max(16 * 1024 * 1024)
}

// ── Types de sortie ───────────────────────────────────────────────────────────

/// Une feuille extraite sur le disque (fichier à plat sous `dest`).
#[derive(Debug)]
pub struct Leaf {
    /// Chemin d'affichage (relatif au sein de l'archive, assaini).
    pub name: String,
    pub ext: String,
    /// Octets réellement décompressés écrits.
    pub size: u64,
    /// Emplacement physique du fichier (sous le dossier `dest` fourni).
    pub path: PathBuf,
}

#[derive(Debug, Default)]
pub struct Outcome {
    pub leaves: Vec<Leaf>,
    /// Erreurs « douces » par entrée (format non supporté, symlink, profondeur) :
    /// l'expansion continue, l'entrée fautive est ignorée.
    pub errors: Vec<String>,
}

// ── Point d'appel central (dispatcher par format) ────────────────────────────

/// Décompresse `spool` (fichier archive sur disque) dans `dest`, sans jamais
/// charger plus d'une entrée en RAM.
///
/// `dest` doit être un dossier temporaire détenu par l'appelant (sa durée de vie
/// détermine celle des fichiers feuilles). Les erreurs « dures » (bombe, quota
/// dépassé, archive corrompue) retournent `Err` et annulent toute l'expansion.
pub fn expand_archive(
    spool: &Path,
    ext: &str,
    dest: &Path,
    limits: &Limits,
) -> Result<Outcome, String> {
    let mut w = Walker {
        dest,
        limits,
        leaves: Vec::new(),
        errors: Vec::new(),
        total_bytes: 0u64,
        processed: 0usize,
        seq: 0u64,
        hard_error: None,
    };
    w.walk(spool, ext, limits.max_depth)?;
    Ok(Outcome {
        leaves: w.leaves,
        errors: w.errors,
    })
}

// ── Walker interne ────────────────────────────────────────────────────────────

struct Walker<'a> {
    dest: &'a Path,
    limits: &'a Limits,
    leaves: Vec<Leaf>,
    errors: Vec<String>,
    total_bytes: u64,
    processed: usize,
    seq: u64,
    hard_error: Option<String>,
}

impl Walker<'_> {
    /// Ouvre une archive `spool` et traite ses entrées. `depth` = niveaux restants
    /// (le niveau courant est consommé : les imbriquées ne sont ouvertes que si depth > 1).
    fn walk(&mut self, spool: &Path, ext: &str, depth: usize) -> Result<(), String> {
        if depth == 0 {
            return Err(format!(
                "Archives imbriquées au-delà de la profondeur max ({}) — arrêt.",
                self.limits.max_depth
            ));
        }
        match ext {
            "zip" => self.walk_zip(spool, depth),
            "7z" => self.walk_7z(spool, depth),
            other => Err(format!("Format d'archive non supporté : .{other}")),
        }
    }

    // ── ZIP ────────────────────────────────────────────────────────────────

    fn walk_zip(&mut self, spool: &Path, depth: usize) -> Result<(), String> {
        let file = File::open(spool).map_err(|e| format!("Lecture archive ZIP impossible : {e}"))?;
        let mut arc =
            zip::ZipArchive::new(file).map_err(|e| format!("Archive ZIP invalide : {e}"))?;

        for i in 0..arc.len() {
            if self.hard_error.is_some() {
                break;
            }
            let mut zf = match arc.by_index(i) {
                Ok(f) => f,
                Err(e) => {
                    self.errors
                        .push(format!("Entrée #{i} illisible (ignorée) : {e}"));
                    continue;
                }
            };

            if zf.is_dir() {
                continue;
            }
            // Symlinks : on les ignore (jamais suivis, jamais écrits).
            if let Some(mode) = zf.unix_mode() {
                const S_IFMT: u32 = 0o170000;
                const S_IFLNK: u32 = 0o120000;
                if mode & S_IFMT == S_IFLNK {
                    self.errors.push(format!("Lien symbolique ignoré : {}", zf.name()));
                    continue;
                }
            }
            self.bump_processed()?;

            let raw_name = zf.name();
            let Some(name) = sanitize_name(raw_name) else {
                self.errors.push(format!("Chemin non sûr ignoré : {raw_name}"));
                continue;
            };
            let ext = file_ext(&name);

            // Contrôles « durs » pré-déclaration (anti bombe).
            self.precheck(&name, zf.size(), zf.compressed_size())?;

            if is_archive_ext(&ext) {
                // Archive imbriquée → récursion (après spool) si profondeur restante.
                if depth > 1 {
                    let scratch = self.new_file()?;
                    let written =
                        stream_to(&mut zf, &scratch, self.limits.max_entry_bytes).map_err(|e| {
                            let _ = std::fs::remove_file(&scratch);
                            e
                        })?;
                    self.add_total(written, &scratch)?;
                    // Récursion : écrit ses feuilles dans le même `dest`.
                    let r = self.walk(&scratch, &ext, depth - 1);
                    let _ = std::fs::remove_file(&scratch);
                    r?;
                } else {
                    self.errors.push(format!(
                        "{name} : archive imbriquée au-delà de la profondeur max — ignorée."
                    ));
                }
                continue;
            }

            if is_doc_ext(&ext) {
                let path = self.new_file()?;
                let written = stream_to(&mut zf, &path, self.limits.max_entry_bytes).map_err(
                    |e| {
                        let _ = std::fs::remove_file(&path);
                        e
                    },
                )?;
                self.add_total(written, &path)?;
                self.leaves.push(Leaf {
                    name,
                    ext: ext.to_string(),
                    size: written,
                    path,
                });
            } else {
                self.errors
                    .push(format!("Format non supporté ignoré : {name}"));
            }
        }
        self.take_hard_error()
    }

    // ── 7Z ─────────────────────────────────────────────────────────────────

    fn walk_7z(&mut self, spool: &Path, depth: usize) -> Result<(), String> {
        use sevenz_rust2::{ArchiveReader, Password};

        let file =
            File::open(spool).map_err(|e| format!("Lecture archive 7z impossible : {e}"))?;
        let mut reader = ArchiveReader::new(file, Password::empty())
            .map_err(|e| format!("Archive 7z invalide : {e}"))?;

        // Pré-scan des métadonnées (tailles déclarées) : on rejette très tôt les
        // archives dont les déclarations dépassent les quotas durs.
        {
            let files: Vec<sevenz_rust2::ArchiveEntry> = reader.archive().files.clone();
            if files.len() > self.limits.max_entries {
                return Err(format!(
                    "Archive 7z : trop d'entrées ({} > {})",
                    files.len(),
                    self.limits.max_entries
                ));
            }
            for f in &files {
                if f.is_directory() {
                    continue;
                }
                let Some(name) = sanitize_name(f.name()) else {
                    continue; // traité en douceur pendant le décodage
                };
                self.precheck(&name, f.size(), 0)?;
            }
        }

        // Décodage séquentiel. Pour un .7z (souvent « solide ») on ne peut PAS
        // sauter une entrée : chaque flux est lu (écrit sur disque, ou purgé si
        // non supporté) ; tout dépassement de quota stoppe le décodage.
        let result = reader.for_each_entries(|entry, rdr| {
            if self.hard_error.is_some() {
                return Ok(false);
            }
            match self.handle_7z_entry(entry, rdr, depth) {
                Ok(true) => Ok(true),
                Ok(false) => Ok(false), // arrêt demandé (quota dur) → flag posé
                Err(e) => {
                    self.hard_error = Some(e);
                    Ok(false)
                }
            }
        });

        match result {
            Ok(()) => self.take_hard_error(),
            Err(e) => Err(format!("Décompression 7z : {e}")),
        }
    }

    fn handle_7z_entry(
        &mut self,
        entry: &sevenz_rust2::ArchiveEntry,
        rdr: &mut dyn Read,
        depth: usize,
    ) -> Result<bool, String> {
        if entry.is_directory() || entry.is_anti_item() || !entry.has_stream() {
            return Ok(true);
        }
        self.bump_processed()?;

        let raw_name = entry.name();
        let Some(name) = sanitize_name(raw_name) else {
            self.errors.push(format!("Chemin non sûr ignoré : {raw_name}"));
            // Doit malgré tout purger le flux (7z solide).
            drain_reader(rdr, self.limits.max_entry_bytes)?;
            return Ok(true);
        };
        let ext = file_ext(&name);

        self.precheck(&name, entry.size(), 0)?;

        if is_archive_ext(&ext) {
            if depth > 1 {
                let scratch = self.new_file()?;
                let written =
                    stream_to(rdr, &scratch, self.limits.max_entry_bytes).map_err(|e| {
                        let _ = std::fs::remove_file(&scratch);
                        e
                    })?;
                self.add_total(written, &scratch)?;
                let r = self.walk(&scratch, &ext, depth - 1);
                let _ = std::fs::remove_file(&scratch);
                r?;
            } else {
                self.errors.push(format!(
                    "{name} : archive imbriquée au-delà de la profondeur max — ignorée."
                ));
                drain_reader(rdr, self.limits.max_entry_bytes)?;
            }
            return Ok(true);
        }

        if is_doc_ext(&ext) {
            let path = self.new_file()?;
            let written =
                stream_to(rdr, &path, self.limits.max_entry_bytes).map_err(|e| {
                    let _ = std::fs::remove_file(&path);
                    e
                })?;
            self.add_total(written, &path)?;
            self.leaves.push(Leaf {
                name,
                ext: ext.to_string(),
                size: written,
                path,
            });
        } else {
            self.errors
                .push(format!("Format non supporté ignoré : {name}"));
            drain_reader(rdr, self.limits.max_entry_bytes)?;
        }
        Ok(true)
    }

    // ── Helpers de budget ──────────────────────────────────────────────────

    fn bump_processed(&mut self) -> Result<(), String> {
        self.processed += 1;
        if self.processed > self.limits.max_entries {
            return Err(format!(
                "Archive : trop d'entrées traitées (>{}) — arrêt.",
                self.limits.max_entries
            ));
        }
        Ok(())
    }

    /// Contrôles « durs » déclarés avant décompression.
    fn precheck(&mut self, name: &str, declared: u64, compressed: u64) -> Result<(), String> {
        // Ratio suspect : une entrée qui déclare décompresser bien plus que son
        // ratio autorisé (par rapport à sa taille compressée) = candidat zip-bomb.
        if compressed > 0 && declared > 0 && self.limits.max_ratio > 0 {
            if declared / compressed > self.limits.max_ratio {
                return Err(format!(
                    "{name} : ratio de compression excessif (déclaré {declared} octets \
                     pour {compressed} compressés) — archive suspecte (bombe ?)."
                ));
            }
        }
        if declared > self.limits.max_entry_bytes {
            return Err(format!(
                "{name} : entrée déclarée trop volumineuse ({declared} octets > max {}) — \
                 refuse d'allouer.",
                self.limits.max_entry_bytes
            ));
        }
        Ok(())
    }

    fn new_file(&mut self) -> Result<PathBuf, String> {
        self.seq += 1;
        let p = self.dest.join(format!("e{:08}", self.seq));
        if self.dest.is_dir() {
            Ok(p)
        } else {
            std::fs::create_dir_all(self.dest)
                .map_err(|e| format!("Impossible de créer le dossier de staging : {e}"))?;
            Ok(p)
        }
    }

    fn add_total(&mut self, bytes: u64, path: &Path) -> Result<(), String> {
        self.total_bytes += bytes;
        if self.total_bytes > self.limits.max_total_bytes {
            let _ = std::fs::remove_file(path);
            return Err(format!(
                "Taille totale décompressée dépassée (>{}) — arrêt.",
                self.limits.max_total_bytes
            ));
        }
        Ok(())
    }

    fn take_hard_error(&mut self) -> Result<(), String> {
        match self.hard_error.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Nettoie le nom d'une entrée : supprime les composantes dangereuses (`..`,
/// racine, préfixes), conserve un chemin relatif affichable séparé par `/`.
/// Retourne `None` si le chemin est invalide / traversant.
fn sanitize_name(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    let mut out = Vec::new();
    for comp in normalized.split('/') {
        match comp {
            "" | "." => {}
            ".." => return None, // Zip-Slip
            _ => {
                if comp.contains('\0') || comp.contains(':') {
                    return None; // NUL ou préfixe de lecteur Windows
                }
                // Cas d'un chemin absolu (ex: "/etc/passwd" → premier comp vide ignoré).
                out.push(comp);
            }
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out.join("/"))
}

fn file_ext(name: &str) -> String {
    name.rsplit('/')
        .next()
        .unwrap_or(name)
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase()
}

/// Stream `reader` vers `path`, en plafonnant à `cap` octets écrits.
/// Au dépassement, le fichier partiel est supprimé et une erreur est retournée.
fn stream_to<R: Read + ?Sized>(reader: &mut R, path: &Path, cap: u64) -> Result<u64, String> {
    let mut out = File::create(path).map_err(|e| format!("Création fichier temporaire : {e}"))?;
    let mut buf = [0u8; STREAM_BUF];
    let mut written = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Erreur décompression : {e}"))?;
        if n == 0 {
            break;
        }
        written += n as u64;
        if written > cap {
            drop(out);
            let _ = std::fs::remove_file(path);
            return Err(format!(
                "Entrée décompressée trop volumineuse (>{cap} octets) — arrêt."
            ));
        }
        out.write_all(&buf[..n])
            .map_err(|e| format!("Écriture fichier temporaire : {e}"))?;
    }
    Ok(written)
}

/// Purge (lit et jette) un flux pour rester aligné (nécessaire pour un .7z solide).
fn drain_reader(reader: &mut dyn Read, cap: u64) -> Result<u64, String> {
    let mut buf = [0u8; STREAM_BUF];
    let mut drained = 0u64;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("Erreur décompression : {e}"))?;
        if n == 0 {
            break;
        }
        drained += n as u64;
        if drained > cap {
            return Err(format!(
                "Entrée décompressée trop volumineuse (>{cap} octets) — arrêt."
            ));
        }
    }
    Ok(drained)
}

// ── Tests unitaires ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn limits() -> Limits {
        Limits {
            max_depth: 3,
            max_entries: 100,
            max_entry_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_ratio: 50,
        }
    }

    /// Crée un zip en mémoire contenant les (name, content) fournis.
    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::{Cursor, Write};
        let mut buf = Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            for (name, content) in entries {
                w.start_file::<&str, ()>(*name, zip::write::FileOptions::default())
                    .unwrap();
                w.write_all(content).unwrap();
            }
            w.finish().unwrap();
        }
        buf.into_inner()
    }

    fn write_temp(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tmp();
        let p = dir.path().join("arch");
        std::fs::write(&p, bytes).unwrap();
        (dir, p)
    }

    #[test]
    fn zip_flat_docs() {
        let z = make_zip(&[
            ("a.txt", b"hello"),
            ("sub/b.md", b"world"),
            ("c.pdf", b"%PDF-1.4 fake"),
        ]);
        let (_d, p) = write_temp(&z);
        let out_dir = tmp();
        let res = expand_archive(&p, "zip", out_dir.path(), &limits()).unwrap();
        assert_eq!(res.leaves.len(), 3);
        assert!(res.leaves.iter().any(|l| l.name == "a.txt"));
        assert!(res.leaves.iter().any(|l| l.name == "sub/b.md"));
    }

    #[test]
    fn zip_path_traversal_rejected() {
        let z = make_zip(&[("../evil.txt", b"x"), ("ok.txt", b"y")]);
        let (_d, p) = write_temp(&z);
        let out_dir = tmp();
        let res = expand_archive(&p, "zip", out_dir.path(), &limits()).unwrap();
        // « ../evil.txt » est refusé (en erreurs), « ok.txt » présent.
        assert_eq!(res.leaves.len(), 1);
        assert_eq!(res.leaves[0].name, "ok.txt");
        assert!(!res.errors.is_empty());
    }

    #[test]
    fn zip_ratio_bomb_aborts() {
        // On fabrique une entrée qui déclare décompresser énormément par rapport
        // à sa taille compressée réelle n'est pas simple à générer ; on teste donc
        // le garde-fou « taille déclarée par entrée » avec des limites minuscules.
        let z = make_zip(&[("big.bin", &[0u8; 1024])]);
        let (_d, p) = write_temp(&z);
        let out_dir = tmp();
        let mut lim = limits();
        lim.max_entry_bytes = 128; // < 1024
        let res = expand_archive(&p, "zip", out_dir.path(), &lim);
        // L'entrée déclarée dépasse la limite → erreur dure (pas de panique).
        assert!(res.is_err());
    }

    #[test]
    fn depth_limit_blocks_nested_archives() {
        // zip A contient b.zip ; b.zip contient un txt.
        let inner = make_zip(&[("deep.txt", b"deep")]);
        let outer = make_zip(&[("b.zip", &inner)]);
        let (_d, p) = write_temp(&outer);
        let out_dir = tmp();
        let res = expand_archive(&p, "zip", out_dir.path(), &limits()).unwrap();
        // 1 seul niveau imbriqué autorisé par max_depth=3 → le txt est trouvé.
        assert!(res.leaves.iter().any(|l| l.name == "deep.txt"));
    }

    #[test]
    fn sanitize() {
        assert_eq!(sanitize_name("a/b.txt").as_deref(), Some("a/b.txt"));
        assert_eq!(sanitize_name("/abs.txt").as_deref(), Some("abs.txt"));
        assert_eq!(sanitize_name("../x"), None);
        assert_eq!(sanitize_name("a/../../x"), None);
        assert_eq!(sanitize_name("C:/x"), None);
        assert_eq!(sanitize_name("./x").as_deref(), Some("x"));
        assert_eq!(file_ext("dir/file.PDF"), "pdf");
    }
}
