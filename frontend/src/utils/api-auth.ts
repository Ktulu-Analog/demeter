// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
// ----------------------------------------------------------------------------
// Authentification de l'API HTTP locale (constat D-01).
//
// Le backend exige un jeton secret (en-tête X-Demeter-Token) généré à chaque
// lancement. Le jeton n'est jamais exposé par HTTP : il est lu par la commande
// IPC Tauri `get_api_token`. Ce module enveloppe `window.fetch` pour ajouter
// l'en-tête à toute requête visant l'API locale (et à elle seule : le jeton
// n'est jamais envoyé à une autre destination).
//
// Hors Tauri (développement dans un navigateur), le jeton peut être fourni via
// VITE_DEMETER_API_TOKEN, qui doit être identique à DEMETER_API_TOKEN côté
// backend (build de debug uniquement).
// ============================================================================

import { API_BASE } from '../constants';

const TOKEN_HEADER = 'X-Demeter-Token';

let tokenPromise: Promise<string> | null = null;

function loadToken(): Promise<string> {
  if (!tokenPromise) {
    tokenPromise = (async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const w = window as any;
      const invoke = w.__TAURI__?.core?.invoke ?? w.__TAURI_INTERNALS__?.invoke;
      if (typeof invoke === 'function') {
        return (await invoke('get_api_token')) as string;
      }
      return (import.meta.env.VITE_DEMETER_API_TOKEN as string | undefined) ?? '';
    })().catch((e) => {
      tokenPromise = null; // permet de réessayer
      throw e;
    });
  }
  return tokenPromise;
}

function requestUrl(input: RequestInfo | URL): string {
  if (typeof input === 'string') return input;
  if (input instanceof URL) return input.href;
  return input.url;
}

function isLocalApiUrl(url: string): boolean {
  return url.startsWith(`${API_BASE}/api-proxy/`) || url.startsWith(`${API_BASE}/api/`);
}

let installed = false;

/** Installe l'enveloppe de fetch (idempotent). À appeler une seule fois, au démarrage. */
export function installApiAuth(): void {
  if (installed) return;
  installed = true;
  const nativeFetch = window.fetch.bind(window);
  window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    if (!isLocalApiUrl(requestUrl(input))) return nativeFetch(input, init);
    const token = await loadToken();
    const headers = new Headers(init?.headers ?? (input instanceof Request ? input.headers : undefined));
    if (token) headers.set(TOKEN_HEADER, token);
    return nativeFetch(input, { ...init, headers });
  };
}
