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
import { API_BASE } from '../constants';

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
  web_search_mcp?: string;
  web_search_mcp_alias?: string;
  mcp_servers?: { url: string; alias?: string }[];
  font_family?: string;
  font_size?: number;
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
  const [form, setForm]         = useState<Settings>({ ...settings, mcp_servers: settings.mcp_servers || [] });
  const [newMcp, setNewMcp]     = useState('');
  const [newMcpAlias, setNewMcpAlias] = useState('');
  const [resetting, setResetting] = useState(false);
  const [resetDone, setResetDone] = useState(false);

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

  const addMcp = () => {
    let url = newMcp.trim();
    if (!url) return;
    if (!/^https?:\/\//i.test(url)) url = 'http://' + url;
    if ((form.mcp_servers || []).some(s => s.url === url)) { setNewMcp(''); setNewMcpAlias(''); return; }
    const alias = newMcpAlias.trim() || undefined;
    setForm(f => ({ ...f, mcp_servers: [...(f.mcp_servers || []), { url, alias }] }));
    setNewMcp('');
    setNewMcpAlias('');
  };

  const removeMcp = (i: number) =>
    setForm(f => ({ ...f, mcp_servers: (f.mcp_servers || []).filter((_, j) => j !== i) }));

  const updateMcpAlias = (i: number, alias: string) =>
    setForm(f => ({
      ...f,
      mcp_servers: (f.mcp_servers || []).map((s, j) =>
        j === i ? { ...s, alias: alias.trim() || undefined } : s
      )
    }));

  return (
    <div className="modal-overlay">
      <div className="modal settings-modal">
        <div className="modal-header">
          <div className="modal-header-left"><span className="modal-icon">⚙️</span><span className="modal-title">Configuration</span></div>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="modal-body settings-body">

          {/* Colonne gauche — LLM */}
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
              <label className="field-label">Modèle</label>
              <input className="field-input" value={form.model} onChange={update('model')} placeholder="openai/gpt-oss-120b" />
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
                  <input className="field-input mcp-web-alias-input" value={form.web_search_mcp_alias || ''} onChange={update('web_search_mcp_alias')} placeholder="Nom affiché sur le bouton (ex: Recherche web)" />
                </div>
              </div>
            </div>

            <div className="settings-section-title" style={{ marginTop: 16 }}>Apparence</div>

            {/* Police */}
            <div className="field-group">
              <label className="field-label">Police de caractères</label>
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

            {/* Taille */}
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

            {/* Aperçu */}
            <div
              className="font-preview"
              style={{
                fontFamily: FONT_PRESETS.find(fp => fp.id === (form.font_family ?? 'dm-sans'))?.stack,
                fontSize: form.font_size ?? 14,
              }}
            >
              Voici un aperçu de votre police — <em>italique</em>, <strong>gras</strong>, chiffres 0123.
            </div>
            <div className="field-hint" style={{ marginBottom: 10 }}>
              <span className="hint-icon">⚠️</span>
              <span>Réinitialise les espaces et prompts aux valeurs d'usine embarquées dans le binaire.</span>
            </div>
            <button
              className={`btn-danger${resetDone ? ' btn-danger--done' : ''}`}
              onClick={handleResetPrompts}
              disabled={resetting || resetDone}
            >
              {resetDone ? "✓ Réinitialisé — rechargez l'application" : resetting ? "Réinitialisation…" : "↺ Réinitialiser les prompts par défaut"}
            </button>
          </div>

          {/* Séparateur vertical */}
          <div className="settings-divider" />

          {/* Colonne droite — MCP */}
          <div className="settings-col">
            <div className="settings-section-title">Serveurs MCP</div>
            <div className="field-hint" style={{ marginBottom: 12 }}>
              <span className="hint-icon">🔌</span>
              <span>Connecteurs MCP qui étendent les capacités du LLM (filesystem, calendrier, SIRH…)</span>
            </div>

            <div className="mcp-cards-list">
              {(form.mcp_servers || []).length === 0 && (
                <div className="mcp-empty-state">
                  <span className="mcp-empty-icon">🔌</span>
                  <span>Aucun serveur configuré</span>
                </div>
              )}
              {(form.mcp_servers || []).map((srv, i) => (
                <div key={i} className="mcp-card">
                  <div className="mcp-card__accent" />
                  <div className="mcp-card__body">
                    <div className="mcp-card__top">
                      <span className="mcp-card__icon">⚡</span>
                      <input
                        className="mcp-card__alias-input"
                        value={srv.alias || ''}
                        onChange={e => updateMcpAlias(i, e.target.value)}
                        placeholder="Nom du serveur…"
                        spellCheck={false}
                      />
                      <button className="mcp-card__remove" onClick={() => removeMcp(i)} title="Supprimer">✕</button>
                    </div>
                    <span className="mcp-card__url" title={srv.url}>{srv.url}</span>
                  </div>
                </div>
              ))}
            </div>

            <div className="mcp-add-card">
              <div className="mcp-add-card__icon">＋</div>
              <div className="mcp-add-card__fields">
                <input
                  className="field-input"
                  value={newMcp}
                  onChange={e => setNewMcp(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && addMcp()}
                  placeholder="http://localhost:3001/sse"
                  spellCheck={false}
                />
                <input
                  className="field-input mcp-add-alias"
                  value={newMcpAlias}
                  onChange={e => setNewMcpAlias(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && addMcp()}
                  placeholder="Nom (optionnel)"
                  spellCheck={false}
                />
              </div>
              <button className="mcp-add-btn" onClick={addMcp}>Ajouter</button>
            </div>

            <div className="field-hint" style={{ marginTop: 12 }}>
              <span className="hint-icon">💡</span>
              <span>Ces paramètres sont sauvegardés localement et ne sont jamais transmis à un tiers.</span>
            </div>
          </div>

        </div>
        <div className="modal-footer">
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
            <div className="about-row"><span className="about-key">Version</span><span className="about-val">v2.1.1</span></div>
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
