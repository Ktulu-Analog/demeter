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

// En développement, Vite proxifie /api-proxy/* vers http://localhost:45678,
// donc une base vide suffit. En production Tauri, le frontend est servi depuis
// tauri://localhost (Linux/macOS) ou https://tauri.localhost (Windows) : il
// n'y a plus de proxy, il faut cibler le serveur Axum directement.
const isTauri = typeof (window as any).__TAURI__ !== 'undefined';
export const API_BASE: string = isTauri ? 'http://localhost:45678' : '';

export const IMAGE_MIME: string[] = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
export const IMAGE_EXTS: string[] = ['png', 'jpg', 'jpeg', 'gif', 'webp'];
export const DOC_EXTS: string[]   = ['pdf', 'docx', 'doc'];
export const ACCEPTED_DOC_EXTS    = new Set(['.pdf', '.docx', '.doc', '.txt', '.md', '.markdown', '.rst', '.csv']);

export const MIME_TO_EXT: Record<string, string> = {
  'image/png':  'png',
  'image/jpeg': 'jpg',
  'image/gif':  'gif',
  'image/webp': 'webp',
};

export interface DotOption {
  value: string;
  label: string;
  color: string;
  border?: string;
}

export const DOT_OPTIONS: DotOption[] = [
  { value: '',       label: 'Aucun',   color: 'transparent', border: '#ccc' },
  { value: 'green',  label: 'Vert',    color: '#1a9e72' },
  { value: 'blue',   label: 'Bleu',    color: '#3b82f6' },
  { value: 'amber',  label: 'Ambre',   color: '#f59e0b' },
  { value: 'red',    label: 'Rouge',   color: '#ef4444' },
  { value: 'purple', label: 'Violet',  color: '#8b5cf6' },
  { value: 'pink',   label: 'Rose',    color: '#ec4899' },
  { value: 'cyan',   label: 'Cyan',    color: '#06b6d4' },
  { value: 'gray',   label: 'Gris',    color: '#6b7280' },
];

export interface IconOption {
  value: string;
  label: string;
}

export const ICON_OPTIONS: IconOption[] = [
  { value: '💬', label: 'Chat' },
  { value: '🤖', label: 'Robot' },
  { value: '🧠', label: 'Cerveau' },
  { value: '⭐', label: 'Étoile' },
  { value: '🔥', label: 'Feu' },
  { value: '🌿', label: 'Feuille' },
  { value: '🌱', label: 'Plante' },
  { value: '🌸', label: 'Fleur' },
  { value: '🎯', label: 'Cible' },
  { value: '📋', label: 'Presse-papiers' },
  { value: '📝', label: 'Note' },
  { value: '📄', label: 'Document' },
  { value: '📁', label: 'Dossier' },
  { value: '📊', label: 'Graphique' },
  { value: '📈', label: 'Hausse' },
  { value: '📉', label: 'Baisse' },
  { value: '🔍', label: 'Recherche' },
  { value: '🔧', label: 'Outil' },
  { value: '⚙️', label: 'Paramètres' },
  { value: '🛠️', label: 'Outils' },
  { value: '🔐', label: 'Sécurité' },
  { value: '🔑', label: 'Clé' },
  { value: '👥', label: 'Équipe' },
  { value: '👤', label: 'Personne' },
  { value: '🧑‍💼', label: 'Employé' },
  { value: '🏢', label: 'Bureau' },
  { value: '🏛️', label: 'Institution' },
  { value: '🎓', label: 'Formation' },
  { value: '📚', label: 'Livres' },
  { value: '✉️', label: 'Email' },
  { value: '📬', label: 'Courrier' },
  { value: '📞', label: 'Téléphone' },
  { value: '💼', label: 'Valise' },
  { value: '🗂️', label: 'Classeur' },
  { value: '🌍', label: 'Monde' },
  { value: '🏖️', label: 'Congés' },
  { value: '⚖️', label: 'Justice' },
  { value: '💰', label: 'Finance' },
  { value: '🎨', label: 'Illustration' },
  { value: '🌴', label: 'Palmier' },
  { value: '✨', label: 'Étincelles' },
  { value: '🚀', label: 'Rocket' },
  { value: '💡', label: 'Idée' },
  { value: '🎤', label: 'Micro' },
  { value: '🗓️', label: 'Calendrier' },
  { value: '⏰', label: 'Horloge' },
];

export const HISTORY_ITEMS: unknown[] = [];
