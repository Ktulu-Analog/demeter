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

export const API_BASE = '';

export const IMAGE_MIME: string[] = ['image/png', 'image/jpeg', 'image/gif', 'image/webp'];
export const IMAGE_EXTS: string[] = ['png', 'jpg', 'jpeg', 'gif', 'webp'];
export const DOC_EXTS: string[]   = ['pdf', 'docx', 'doc'];
export const ACCEPTED_DOC_EXTS    = new Set(['.pdf', '.docx', '.doc', '.txt', '.md']);

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
  { value: '',      label: 'Aucun',  color: 'transparent', border: '#ccc' },
  { value: 'green', label: 'Vert',   color: '#1a9e72' },
  { value: 'blue',  label: 'Bleu',   color: '#3b82f6' },
  { value: 'amber', label: 'Ambre',  color: '#f59e0b' },
];

export const HISTORY_ITEMS: unknown[] = [];
