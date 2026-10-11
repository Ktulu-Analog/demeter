// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
// ----------------------------------------------------------------------------
// Persistance des réglages avec secrets dans le TROUSSEAU du système (D-08).
//
// Les réglages sont scindés en deux :
//   - partie publique (endpoint, modèle, polices…) : localStorage ;
//   - partie secrète (SECRET_FIELDS) : trousseau du système, via les commandes
//     IPC Tauri `secrets_get` / `secrets_set` (nom d'entrée fixe côté Rust).
//
// Aucun repli silencieux en clair : si le trousseau échoue, l'erreur est
// remontée à l'appelant, les secrets restent en mémoire pour la session et ne
// sont PAS réécrits dans le localStorage.
//
// Hors Tauri (développement dans un navigateur), tout reste dans le localStorage.
// ============================================================================

import type { Settings } from '../components/Modals';

/** Champs de Settings stockés dans le trousseau. */
export const SECRET_FIELDS = [
  'bearer',
  'qdrant_api_key',
  'mcp_stdio_config',
  'mcp_servers',      // les URL peuvent embarquer un jeton
  'web_search_mcp',
] as const satisfies readonly (keyof Settings)[];

type SecretField = (typeof SECRET_FIELDS)[number];
export type SecretPart = Partial<Pick<Settings, SecretField>>;

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const invoke = (): ((cmd: string, args?: any) => Promise<any>) | undefined => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const w = window as any;
  return w.__TAURI__?.core?.invoke ?? w.__TAURI_INTERNALS__?.invoke;
};

export const hasKeychain = (): boolean => typeof invoke() === 'function';

/** Sépare les réglages en partie publique et partie secrète. */
export function splitSettings(s: Settings): { publicPart: Partial<Settings>; secretPart: SecretPart } {
  const publicPart: Partial<Settings> = { ...s };
  const secretPart: Record<string, unknown> = {};
  for (const f of SECRET_FIELDS) {
    if (s[f] !== undefined) secretPart[f] = s[f];
    delete (publicPart as Record<string, unknown>)[f];
  }
  return { publicPart, secretPart: secretPart as SecretPart };
}

/** Extrait d'un objet (ex. ancien localStorage) les champs secrets présents. */
export function pickSecrets(obj: Record<string, unknown>): SecretPart {
  const out: Record<string, unknown> = {};
  for (const f of SECRET_FIELDS) {
    const v = obj[f];
    const nonEmpty =
      v !== undefined && v !== null && v !== '' &&
      !(Array.isArray(v) && v.length === 0) &&
      !(typeof v === 'object' && !Array.isArray(v) && Object.keys(v as object).length === 0);
    if (nonEmpty) out[f] = v;
  }
  return out as SecretPart;
}

/** Lit les secrets. `value: null` = aucun secret enregistré. Lève si le trousseau échoue. */
export async function readSecrets(): Promise<{ value: SecretPart | null }> {
  const inv = invoke();
  if (!inv) return { value: null };
  const raw: string | null = await inv('secrets_get');
  if (!raw) return { value: null };
  return { value: JSON.parse(raw) as SecretPart };
}

/** Enregistre les secrets. Lève si le trousseau échoue. */
export async function writeSecrets(part: SecretPart): Promise<void> {
  const inv = invoke();
  if (!inv) return;
  await inv('secrets_set', { value: JSON.stringify(part) });
}

export interface SecurityStatus {
  db_encrypted: boolean;
  db_persistent: boolean;
  keychain_available: boolean;
  warnings: string[];
}

export async function readSecurityStatus(): Promise<SecurityStatus | null> {
  const inv = invoke();
  if (!inv) return null;
  try { return (await inv('get_security_status')) as SecurityStatus; } catch { return null; }
}
