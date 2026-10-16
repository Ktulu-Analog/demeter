// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Autorisation d'exécution automatique des outils MCP (constat F-01).
//
// L'utilisateur autorise UNE FOIS un serveur MCP (dans le panneau « Serveurs MCP »,
// ou à la première utilisation) ; l'assistant peut ensuite appeler ses outils sans
// redemander, d'un message à l'autre et d'une session à l'autre. L'autorisation est
// révocable à tout moment dans le panneau MCP.
//
// La liste des clés autorisées (`Settings.mcp_consent`) est conservée dans le
// trousseau et transmise au backend par la commande IPC `init_credentials` : c'est le
// BACKEND qui applique la règle (un serveur non autorisé ne fournit aucun outil).
// Pour un serveur stdio, l'autorisation enregistre aussi l'empreinte de son binaire
// (commande IPC `mcp_stdio_trust`, constat F-03).
// ============================================================================

import type { Settings } from '../components/Modals';

/** Clé d'un serveur : l'URL pour un MCP distant, « stdio:<id> » pour un MCP local. */
export const stdioKey = (id: string) => `stdio:${id}`;
export const isStdioKey = (key: string) => key.startsWith('stdio:');
export const stdioId = (key: string) => key.slice('stdio:'.length);

export interface ConsentEntry {
  key: string;
  label: string;
  kind: 'stdio' | 'http';
  /** Précision affichée sous le nom (jamais l'URL complète : elle peut contenir un jeton). */
  detail?: string;
}

/** Nom d'hôte seul : l'URL complète peut embarquer un jeton. */
export function hostOf(url: string): string {
  try { return new URL(url).host; } catch { return 'serveur distant'; }
}

export function entryFor(key: string, alias?: string, detail?: string): ConsentEntry {
  if (isStdioKey(key)) {
    return { key, kind: 'stdio', label: alias || stdioId(key), detail: detail || 'Programme local exécuté avec vos droits' };
  }
  return { key, kind: 'http', label: alias || hostOf(key), detail: detail || `Serveur distant (${hostOf(key)})` };
}

export const isConsented = (s: Settings, key: string): boolean =>
  (s.mcp_consent || []).includes(key);

/** Serveurs que la prochaine requête utiliserait (MCP activés + recherche web si active). */
export function serversInUse(s: Settings, webSearchActive: boolean): ConsentEntry[] {
  const out: ConsentEntry[] = (s.mcp_servers || []).map(m => entryFor(m.url, m.alias));
  if (webSearchActive && s.web_search_mcp) {
    out.push(entryFor(s.web_search_mcp, s.web_search_mcp_alias || 'Recherche web'));
  }
  return out;
}

export function withConsent(s: Settings, keys: string[]): Settings {
  return { ...s, mcp_consent: Array.from(new Set([...(s.mcp_consent || []), ...keys])) };
}

export function withoutConsent(s: Settings, keys: string[]): Settings {
  const drop = new Set(keys);
  return { ...s, mcp_consent: (s.mcp_consent || []).filter(k => !drop.has(k)) };
}

// ── Commandes IPC (stdio) ─────────────────────────────────────────────────────

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const invoke = (): ((cmd: string, args?: any) => Promise<any>) | undefined => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any;
  return w.__TAURI__?.core?.invoke ?? w.__TAURI_INTERNALS__?.invoke;
};

/** Enregistre l'empreinte du serveur stdio (autorisation). Lève en cas d'échec. */
export async function trustStdioServer(id: string): Promise<void> {
  const inv = invoke();
  if (!inv) throw new Error("disponible uniquement dans l'application desktop Demeter");
  await inv('mcp_stdio_trust', { id });
}

/** Retire l'autorisation (et arrête le processus) d'un serveur stdio. */
export async function untrustStdioServer(id: string): Promise<void> {
  const inv = invoke();
  if (!inv) return;
  await inv('mcp_stdio_untrust', { id });
}
