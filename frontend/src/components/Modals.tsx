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


import React, { useState, useEffect } from 'react';
import { API_BASE, DEFAULT_AUDIO_SUMMARY_PROMPTS } from '../constants';
import type { AudioSummaryPrompt } from '../constants';

// Applique immédiatement les préférences de police sur :root pour preview live
function applyFontPrefs(fontFamily?: string, fontSize?: number) {
  const FONT_STACKS: Record<string, string> = {
    'dm-sans':     "'DM Sans', sans-serif",
    'inter':       "'Inter', sans-serif",
    'system-ui':   "system-ui, -apple-system, sans-serif",
    'ubuntu':      "'Ubuntu', 'Cantarell', sans-serif",
  };
  const root = document.documentElement;
  root.style.setProperty('--font-body', FONT_STACKS[fontFamily ?? 'dm-sans'] ?? FONT_STACKS['dm-sans']);
  root.style.setProperty('--font-size-base', `${fontSize ?? 14}px`);
}

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Settings {
  endpoint: string;
  bearer: string;
  model: string;
  extra_models?: string[]; // liste de modèles saisis manuellement (fallback si /v1/models inaccessible)
  qdrant_url?: string;
  qdrant_api_key?: string;
  web_search_mcp?: string;
  web_search_mcp_alias?: string;
  mcp_servers?: { url: string; alias?: string }[];
  /// Dossier choisi par l'utilisateur pour ses serveurs MCP stdio (mcp-*).
  mcp_dir?: string;
  /// Valeurs de configuration saisies dans le panel MCP, par serveur (id du
  /// manifest) puis par clé de paramètre (ex : mcp_stdio_config['mcp-grist']['GRIST_API_KEY']).
  mcp_stdio_config?: Record<string, Record<string, string>>;
  /// Serveurs MCP dont l'utilisateur a autorisé l'exécution automatique des outils par
  /// l'assistant (F-01) : URL pour un MCP distant, « stdio:<id> » pour un MCP local.
  /// Conservé dans le trousseau ; appliqué côté backend.
  mcp_consent?: string[];
  font_family?: string;
  font_size?: number;
  audio_summary_prompts?: AudioSummaryPrompt[]; // prompts de résumé auto pour la transcription audio
}

interface McpTool {
  name: string;
  description?: string;
}

interface McpServerStatus {
  server: string;
  status: 'ok' | 'error';
  tools: McpTool[];
  tool_count: number;
}

/// Un paramètre de configuration attendu par un serveur MCP stdio (clé
/// d'API…), tel que retourné dans le manifest par GET /api/mcp/stdio/servers.
export interface StdioParam {
  key: string;
  label: string;
  secret?: boolean;
  required?: boolean;
  default?: string;
  help?: string;
}

/// Un MCP stdio découvert dans le répertoire dédié (officiel ou ajouté
/// manuellement), tel que retourné par GET /api/mcp/stdio/servers.
export interface StdioServerInfo {
  id: string;
  name: string;
  description?: string;
  available: boolean; // false si aucun binaire pour cette plateforme
  params: StdioParam[];
  /// État d'autorisation du binaire (F-03) : « trusted » = autorisé et inchangé,
  /// « untrusted » = jamais autorisé, « modified » = changé depuis l'autorisation.
  trust?: 'trusted' | 'untrusted' | 'modified';
}

// ── Préréglages de polices ─────────────────────────────────────────────────────
const FONT_PRESETS = [
  { id: 'dm-sans',      label: 'DM Sans',        stack: "'DM Sans', sans-serif",           hint: 'Défaut' },
  { id: 'inter',        label: 'Inter',           stack: "'Inter', sans-serif",             hint: 'Windows / Chrome OS' },
  { id: 'system-ui',   label: 'Système',         stack: "system-ui, -apple-system, sans-serif", hint: 'macOS / iOS natif' },
  { id: 'ubuntu',       label: 'Ubuntu',          stack: "'Ubuntu', 'Cantarell', sans-serif", hint: 'Linux / GNOME' },
] as const;

const FONT_SIZES = [
  { label: 'XS', value: 12 },
  { label: 'S',  value: 13 },
  { label: 'M',  value: 14, hint: 'défaut' },
  { label: 'L',  value: 15 },
  { label: 'XL', value: 16 },
] as const;

// ── SettingsModal ─────────────────────────────────────────────────────────────

interface SettingsModalProps {
  settings: Settings;
  onSave: (settings: Settings) => void;
  onClose: () => void;
}

export function SettingsModal({ settings, onSave, onClose }: SettingsModalProps) {
  const [form, setForm]         = useState<Settings>({
    ...settings,
    mcp_servers: settings.mcp_servers || [],
    audio_summary_prompts: settings.audio_summary_prompts?.length ? settings.audio_summary_prompts : DEFAULT_AUDIO_SUMMARY_PROMPTS,
  });
  const [resetting, setResetting] = useState(false);
  const [resetDone, setResetDone] = useState(false);

  const updateSummaryPrompt = (i: number, field: keyof AudioSummaryPrompt, val: string) =>
    setForm(f => ({
      ...f,
      audio_summary_prompts: (f.audio_summary_prompts || []).map((p, j) => j === i ? { ...p, [field]: val } : p),
    }));

  const addSummaryPrompt = () =>
    setForm(f => ({
      ...f,
      audio_summary_prompts: [...(f.audio_summary_prompts || []), { id: `prompt-${Date.now()}`, label: '', prompt: '' }],
    }));

  const removeSummaryPrompt = (i: number) =>
    setForm(f => ({ ...f, audio_summary_prompts: (f.audio_summary_prompts || []).filter((_, j) => j !== i) }));

  const resetSummaryPrompts = () => setForm(f => ({ ...f, audio_summary_prompts: DEFAULT_AUDIO_SUMMARY_PROMPTS }));

  const handleResetPrompts = async () => {
    if (!window.confirm('Réinitialiser tous les espaces et prompts à leurs valeurs par défaut ? Cette action est irréversible.')) return;
    setResetting(true);
    try {
      await fetch(`${API_BASE}/api/spaces/reset`, { method: 'POST' });
      setResetDone(true);
      setTimeout(() => setResetDone(false), 3000);
    } catch (e) {
      console.error('Reset failed:', e);
    } finally {
      setResetting(false);
    }
  };

  const update = (k: keyof Settings) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm(f => ({ ...f, [k]: e.target.value }));

  const canSave = !!(form.endpoint && form.bearer && form.model);

  return (
    <div className="modal-overlay">
      <div className="modal settings-modal">
        <div className="modal-header">
          <div className="modal-header-left"><span className="modal-icon">⚙️</span><span className="modal-title">Configuration</span></div>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="modal-body settings-body">

          {/* ── Colonne 1 — LLM ── */}
          <div className="settings-col">
            <div className="settings-section-title">LLM</div>
            <div className="field-group">
              <label className="field-label">Endpoint LLM</label>
              <input className="field-input" value={form.endpoint} onChange={update('endpoint')} placeholder="https://albert.api.etalab.gouv.fr/v1" />
            </div>
            <div className="field-group">
              <label className="field-label">Bearer Token</label>
              <input className="field-input" type="password" value={form.bearer} onChange={update('bearer')} placeholder="sk-…" />
            </div>
            <div className="field-group">
              <label className="field-label">Modèle par défaut</label>
              <input className="field-input" value={form.model} onChange={update('model')} placeholder="openai/gpt-oss-120b" />
            </div>
            <div className="field-group">
              <label className="field-label">
                Modèles disponibles
                <span className="field-label-hint">un par ligne — utilisés si la liste API est inaccessible</span>
              </label>
              <textarea
                className="field-input field-textarea"
                rows={5}
                value={(form.extra_models || []).join('\n')}
                onChange={e => setForm(f => ({ ...f, extra_models: e.target.value.split('\n').map(s => s.trim()).filter(Boolean) }))}
                placeholder={"openai/gpt-oss-120b\nmistral-medium-2508\nmistralai/Mistral-Small-3.2-24B-Instruct-2506"}
                spellCheck={false}
              />
            </div>
            <div className="field-group mcp-web-field-group">
              <label className="field-label">
                <span className="mcp-web-badge">Web</span>
                MCP Web Search
              </label>
              <div className="mcp-web-card">
                <div className="mcp-web-card__icon">🌐</div>
                <div className="mcp-web-card__fields">
                  <input className="field-input" value={form.web_search_mcp || ''} onChange={update('web_search_mcp')} placeholder="http://localhost:6503/mcp" />
                  <input className="field-input mcp-web-alias-input" value={form.web_search_mcp_alias || ''} onChange={update('web_search_mcp_alias')} placeholder="Nom affiché (ex : Recherche web)" />
                </div>
              </div>
            </div>
          </div>

          {/* Séparateur */}
          <div className="settings-divider" />

          {/* ── Colonne 2 — RAG Qdrant + Apparence ── */}
          <div className="settings-col">

            <div className="settings-section-title">RAG — Qdrant</div>
            <div className="field-group">
              <label className="field-label">
                URL du serveur Qdrant
                {form.qdrant_api_key?.startsWith('eyJ') && !form.qdrant_url?.trim() && (
                  <span style={{ marginLeft: 8, color: '#dc2626', fontWeight: 600, fontSize: '0.82em' }}>
                    ⚠ Requise avec un token JWT
                  </span>
                )}
              </label>
              <input
                className="field-input"
                value={form.qdrant_url || ''}
                onChange={update('qdrant_url')}
                placeholder={form.qdrant_api_key?.startsWith('eyJ') ? 'https://qdrant.votre-org.fr' : 'http://localhost:6333'}
                spellCheck={false}
              />
              {form.qdrant_api_key?.startsWith('eyJ') && !form.qdrant_url?.trim() && (
                <div className="field-hint" style={{ color: '#dc2626', marginTop: 4 }}>
                  Sans URL Qdrant, le RAG échoue silencieusement (fallback localhost inaccessible).
                </div>
              )}
            </div>
            <div className="field-group">
              <label className="field-label">
                Clé / Token Qdrant
                <span className="field-label-hint">optionnelle</span>
                {form.qdrant_api_key && (
                  <span className={`qdrant-role-badge ${form.qdrant_api_key.startsWith('eyJ') ? 'qdrant-role-badge--user' : 'qdrant-role-badge--admin'}`}>
                    {form.qdrant_api_key.startsWith('eyJ') ? '🎟 Utilisateur' : '🔑 Admin'}
                  </span>
                )}
              </label>
              <div className="qdrant-key-field">
                <input
                  className="field-input"
                  type="password"
                  value={form.qdrant_api_key || ''}
                  onChange={update('qdrant_api_key')}
                  placeholder="Clé admin ou token JWT utilisateur"
                  autoComplete="off"
                />
                {form.qdrant_api_key && (
                  <button
                    className="qdrant-key-clear"
                    onClick={() => setForm(f => ({ ...f, qdrant_api_key: '' }))}
                    title="Effacer"
                    type="button"
                  >✕</button>
                )}
              </div>
              <div className="field-hint" style={{ marginTop: 4 }}>
                <span className="hint-icon">🔒</span>
                <span>
                  <strong>Admin</strong> : clé brute Qdrant, peut tout faire et générer des tokens.&ensp;
                  <strong>Utilisateur</strong> : token JWT généré par l'admin, accès limité aux collections autorisées.
                </span>
              </div>
            </div>

            <div className="settings-section-title" style={{ marginTop: 8 }}>Apparence</div>

            <div className="field-group">
              <label className="field-label">Police</label>
              <div className="font-preset-row">
                {FONT_PRESETS.map(fp => (
                  <button
                    key={fp.id}
                    className={`font-preset-btn${(form.font_family ?? 'dm-sans') === fp.id ? ' font-preset-btn--active' : ''}`}
                    style={{ fontFamily: fp.stack }}
                    onClick={() => {
                      const next = { ...form, font_family: fp.id };
                      setForm(next);
                      applyFontPrefs(fp.id, next.font_size);
                    }}
                    title={fp.hint}
                  >
                    <span className="font-preset-name">{fp.label}</span>
                    <span className="font-preset-hint">{fp.hint}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className="field-group">
              <label className="field-label">Taille du texte</label>
              <div className="font-size-row">
                {FONT_SIZES.map(fs => (
                  <button
                    key={fs.value}
                    className={`font-size-btn${(form.font_size ?? 14) === fs.value ? ' font-size-btn--active' : ''}`}
                    onClick={() => {
                      const next = { ...form, font_size: fs.value };
                      setForm(next);
                      applyFontPrefs(next.font_family, fs.value);
                    }}
                    title={`${fs.value}px${'hint' in fs && fs.hint ? ' — ' + fs.hint : ''}`}
                  >
                    <span style={{ fontSize: fs.value }}>{fs.label}</span>
                  </button>
                ))}
              </div>
            </div>

            <div
              className="font-preview"
              style={{
                fontFamily: FONT_PRESETS.find(fp => fp.id === (form.font_family ?? 'dm-sans'))?.stack,
                fontSize: form.font_size ?? 14,
              }}
            >
              Aperçu — <em>italique</em>, <strong>gras</strong>, 0123.
            </div>

            <div style={{ flex: 1 }} />
          </div>

          {/* Séparateur */}
          <div className="settings-divider" />

          {/* ── Colonne 3 — Résumé audio ── */}
          <div className="settings-col">
            <div className="settings-section-title">Résumé audio</div>
            <div className="field-hint">
              <span className="hint-icon">📝</span>
              <span>Prompts proposés dans la transcription audio pour générer un résumé automatique.</span>
            </div>

            <div className="mcp-cards-list">
              {(form.audio_summary_prompts || []).map((p, i) => (
                <div key={i} className="mcp-card">
                  <div className="mcp-card__accent" />
                  <div className="mcp-card__body">
                    <div className="mcp-card__top">
                      <span className="mcp-card__icon">💬</span>
                      <input
                        className="mcp-card__alias-input"
                        value={p.label}
                        onChange={e => updateSummaryPrompt(i, 'label', e.target.value)}
                        placeholder="Nom du prompt (ex : Synthèse)"
                        spellCheck={false}
                      />
                      <button className="mcp-card__remove" onClick={() => removeSummaryPrompt(i)} title="Supprimer">✕</button>
                    </div>
                    <textarea
                      className="field-input field-textarea"
                      rows={5}
                      value={p.prompt}
                      onChange={e => updateSummaryPrompt(i, 'prompt', e.target.value)}
                      placeholder="Instructions envoyées au LLM avec la transcription…"
                      spellCheck={false}
                    />
                  </div>
                </div>
              ))}
            </div>

            <div className="pe-add-prompt-btn-row" style={{ display: 'flex', gap: 8 }}>
              <button className="pe-add-prompt-btn" onClick={addSummaryPrompt}>
                <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11">
                  <path d="M8 2a1 1 0 011 1v4h4a1 1 0 110 2H9v4a1 1 0 11-2 0V9H3a1 1 0 110-2h4V3a1 1 0 011-1z"/>
                </svg>
                Ajouter un prompt
              </button>
              <button className="btn-ghost" onClick={resetSummaryPrompts} title="Restaurer les 3 prompts par défaut">
                ↺ Valeurs par défaut
              </button>
            </div>
          </div>

        </div>
        <div className="modal-footer">
          <button
            className={`btn-danger${resetDone ? ' btn-danger--done' : ''}`}
            onClick={handleResetPrompts}
            disabled={resetting || resetDone}
            title="Réinitialise les espaces et prompts aux valeurs d'usine"
            style={{ marginRight: 'auto' }}
          >
            {resetDone ? "✓ Réinitialisé" : resetting ? "Réinitialisation…" : "↺ Réinitialiser les prompts"}
          </button>
          <button className="btn-ghost" onClick={onClose}>Fermer</button>
          <button className="btn-primary" onClick={() => { onSave(form); onClose(); }} disabled={!canSave}>Enregistrer</button>
        </div>
      </div>
    </div>
  );
}

// ── McpStatusPanel ────────────────────────────────────────────────────────────

interface McpStatusPanelProps {
  servers: { url: string; alias?: string }[];
  webSearchMcp?: { url: string; alias?: string };
  webSearchActive?: boolean;
  onClose: () => void;
}

export function McpStatusPanel({ servers, webSearchMcp, webSearchActive, onClose }: McpStatusPanelProps) {
  const [statuses, setStatuses]   = useState<McpServerStatus[] | null>(null);
  const [loading, setLoading]     = useState(true);
  const [expanded, setExpanded]   = useState<Record<number, boolean>>({});

  // Normalise au cas où des strings legacy passeraient encore
  const safeServers = servers.map((s: unknown) =>
    typeof s === 'string' ? { url: s, alias: undefined } : s as { url: string; alias?: string }
  );
  const serverUrls    = safeServers.map(s => s.url).join('|');
  const serverAliases = safeServers.map(s => s.alias || '').join('|');
  const webMcpUrl     = webSearchMcp?.url ?? '';
  const webMcpAlias   = webSearchMcp?.alias ?? '';

  useEffect(() => {
    const allServers = [
      ...safeServers.map(s => s.url),
      ...(webSearchActive && webSearchMcp ? [webSearchMcp.url] : []),
    ];

    const aliasMap: Record<string, string> = {};
    safeServers.forEach(s => { if (s.alias) aliasMap[s.url] = s.alias; });
    if (webSearchMcp?.alias) aliasMap[webSearchMcp.url] = webSearchMcp.alias;

    if (!allServers.length) { setLoading(false); setStatuses([]); return; }
    setLoading(true);
    fetch(`${API_BASE}/api-proxy/api/mcp/tools`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ servers: allServers }),
    })
      .then(r => r.json())
      .then((data: { servers?: McpServerStatus[] }) => setStatuses(data.servers || []))
      .catch(() => setStatuses(allServers.map(s => ({ server: s, status: 'error' as const, tools: [], tool_count: 0 }))))
      .finally(() => setLoading(false));
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serverUrls, serverAliases, webMcpUrl, webMcpAlias, webSearchActive]);

  // Build alias map for render (stable, derived from same primitives)
  const aliasMap: Record<string, string> = {};
  safeServers.forEach(s => { if (s.alias) aliasMap[s.url] = s.alias; });
  if (webSearchMcp?.alias) aliasMap[webSearchMcp.url] = webSearchMcp.alias;

  const toggleExpanded = (i: number) =>
    setExpanded(prev => ({ ...prev, [i]: !prev[i] }));

  const displayName = (url: string) => {
    if (aliasMap[url]) return aliasMap[url];
    try {
      const u = new URL(url);
      return u.port ? `localhost:${u.port}` : u.hostname.replace(/^www\./, '');
    } catch {
      return url.length > 40 ? url.slice(0, 37) + '…' : url;
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal mcp-status-modal" onClick={e => e.stopPropagation()} style={{ width: 580 }}>
        <div className="modal-header">
          <span className="modal-title">🔌 Serveurs MCP actifs</span>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="modal-body mcp-modal-body">
          {loading && (
            <div style={{ textAlign: 'center', padding: '24px 0', opacity: .6 }}>
              Connexion aux serveurs…
            </div>
          )}
          {!loading && (!statuses || statuses.length === 0) && (
            <div style={{ textAlign: 'center', padding: '24px 0', opacity: .6 }}>
              <div style={{ fontSize: 32, marginBottom: 8 }}>🔌</div>
              Aucun serveur MCP configuré.<br />
              <span style={{ fontSize: 12 }}>Ajoutez des serveurs dans les Paramètres.</span>
            </div>
          )}
          {!loading && statuses && statuses.map((srv, i) => {
            const open = !!expanded[i];
            const hasTools = srv.tools?.length > 0;
            return (
              <div key={i} className={`mcp-pill-card mcp-pill-card--${srv.status}`}>
                {/* ── Pill header (always visible) ── */}
                <button
                  className="mcp-pill-header"
                  onClick={() => hasTools && toggleExpanded(i)}
                  aria-expanded={open}
                  style={{ cursor: hasTools ? 'pointer' : 'default' }}
                >
                  <span className={`mcp-status-dot mcp-status-dot--${srv.status}`} />
                  <span className="mcp-pill-name" title={srv.server}>{displayName(srv.server)}</span>
                  <span className={`mcp-pill-badge mcp-pill-badge--${srv.status}`}>
                    {srv.status === 'ok'
                      ? `${srv.tool_count} outil${srv.tool_count !== 1 ? 's' : ''}`
                      : 'Injoignable'}
                  </span>
                  {hasTools && (
                    <span className={`mcp-pill-chevron${open ? ' mcp-pill-chevron--open' : ''}`}>▾</span>
                  )}
                </button>

                {/* ── Collapsible tools list ── */}
                {open && hasTools && (
                  <div className="mcp-pill-tools">
                    {srv.tools.map((t, j) => (
                      <div key={j} className="mcp-tool-item">
                        <span className="mcp-tool-name">{t.name}</span>
                        {t.description && <span className="mcp-tool-desc">{t.description}</span>}
                      </div>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <div className="modal-footer">
          <button className="btn-ghost" onClick={onClose}>Fermer</button>
        </div>
      </div>
    </div>
  );
}

// ── AboutModal ────────────────────────────────────────────────────────────────

export function AboutModal({ onClose }: { onClose: () => void }) {
  return (
    <div className="modal-overlay">
      <div className="modal about-modal">
        <div className="about-logo-wrap">
          <img src="/logo-demeter.png" alt="Logo Demeter" className="about-logo" />
        </div>
        <div className="about-info">
          <div className="about-meta">
            <div className="about-row"><span className="about-key">Auteur</span><span className="about-val">Pierre COUGET</span></div>
            <div className="about-row"><span className="about-key">Version</span><span className="about-val">v2.4.5</span></div>
            <div className="about-row">
              <span className="about-key">Contact</span>
              <a className="about-val about-link" href="mailto:ktulu.analog@gmail.com">ktulu.analog@gmail.com ↗</a>
            </div>
            <div className="about-row">
              <span className="about-key">GitHub</span>
              <a className="about-val about-link" href="https://github.com/Ktulu-Analog/demeter" target="_blank" rel="noopener noreferrer">Ktulu-Analog/demeter ↗</a>
            </div>
            <div className="about-row">
              <span className="about-key">Licence</span>
              <a className="about-val about-link" href="https://www.gnu.org/licenses/agpl-3.0.html" target="_blank" rel="noopener noreferrer">AGPL 3.0 ↗</a>
            </div>
          </div>
          <button className="about-close-btn" onClick={onClose}>Fermer</button>
        </div>
      </div>
    </div>
  );
}

// ── McpTooltipButton ──────────────────────────────────────────────────────────
// Bouton pill avec info-bulle au survol affichant statut des serveurs MCP.
// Le clic est délégué au parent (inchangé).

export interface McpEntry { url: string; alias?: string; }

interface McpTooltipButtonProps {
  entries: McpEntry[];          // serveurs à afficher dans la bulle
  label: React.ReactNode;       // contenu du bouton
  className?: string;
  onClick?: () => void;
  type?: 'button' | 'submit';
}

// Cache global : url → { status, ts }
const _statusCache: Record<string, { ok: boolean; ts: number }> = {};
const CACHE_TTL = 15_000; // 15 s

async function fetchStatuses(entries: McpEntry[]): Promise<Record<string, boolean>> {
  const toFetch = entries.filter(e => {
    const c = _statusCache[e.url];
    return !c || Date.now() - c.ts > CACHE_TTL;
  });

  if (toFetch.length > 0) {
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/mcp/tools`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ servers: toFetch.map(e => e.url) }),
      });
      const data: { servers?: Array<{ server: string; status: string }> } = await res.json();
      (data.servers || []).forEach(s => {
        _statusCache[s.server] = { ok: s.status === 'ok', ts: Date.now() };
      });
    } catch {
      toFetch.forEach(e => { _statusCache[e.url] = { ok: false, ts: Date.now() }; });
    }
  }

  return Object.fromEntries(entries.map(e => [e.url, _statusCache[e.url]?.ok ?? false]));
}

export function McpTooltipButton({ entries, label, className, onClick, type = 'button' }: McpTooltipButtonProps) {
  const [visible,  setVisible]  = useState(false);
  const [statuses, setStatuses] = useState<Record<string, boolean>>({});
  const [loading,  setLoading]  = useState(false);
  const timerRef = React.useRef<ReturnType<typeof setTimeout> | null>(null);

  const handleMouseEnter = () => {
    timerRef.current = setTimeout(async () => {
      setVisible(true);
      if (entries.length === 0) return;
      setLoading(true);
      const s = await fetchStatuses(entries);
      setStatuses(s);
      setLoading(false);
    }, 300); // délai avant affichage
  };

  const handleMouseLeave = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setVisible(false);
  };

  const displayName = (e: McpEntry) => {
    if (e.alias) return e.alias;
    try {
      const u = new URL(e.url);
      return u.port ? `localhost:${u.port}` : u.hostname;
    } catch { return e.url; }
  };

  return (
    <div className="mcp-tooltip-wrapper" onMouseEnter={handleMouseEnter} onMouseLeave={handleMouseLeave}>
      <button className={className} onClick={onClick} type={type}>
        {label}
      </button>
      {visible && (
        <div className="mcp-tooltip">
          <div className="mcp-tooltip__arrow" />
          {entries.length === 0 && (
            <div className="mcp-tooltip__empty">Aucun serveur configuré</div>
          )}
          {loading && entries.length > 0 && (
            <div className="mcp-tooltip__loading">
              {entries.map(e => (
                <div key={e.url} className="mcp-tooltip__row">
                  <span className="mcp-tooltip__dot mcp-tooltip__dot--loading" />
                  <span className="mcp-tooltip__name">{displayName(e)}</span>
                </div>
              ))}
            </div>
          )}
          {!loading && entries.map(e => (
            <div key={e.url} className="mcp-tooltip__row">
              <span className={`mcp-tooltip__dot mcp-tooltip__dot--${statuses[e.url] ? 'ok' : 'err'}`} />
              <span className="mcp-tooltip__name">{displayName(e)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
