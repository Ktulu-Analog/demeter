// In Tauri, the embedded Rust backend listens on this port.
// En dev, Vite proxifie /api-proxy → http://localhost:45678 (voir vite.config.ts)
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
