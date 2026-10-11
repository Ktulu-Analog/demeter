/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Jeton d'API pour le développement dans un navigateur (hors Tauri), voir utils/api-auth.ts */
  readonly VITE_DEMETER_API_TOKEN?: string;
}
