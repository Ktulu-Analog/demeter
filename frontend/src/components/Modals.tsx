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
  tavily_key?: string;
  mcp_servers?: string[];
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
    if (!/^https?:\/\//i.test(url)) url = 'https://' + url;
    if ((form.mcp_servers || []).includes(url)) { setNewMcp(''); return; }
    setForm(f => ({ ...f, mcp_servers: [...(f.mcp_servers || []), url] }));
    setNewMcp('');
  };

  const removeMcp = (i: number) =>
    setForm(f => ({ ...f, mcp_servers: (f.mcp_servers || []).filter((_, j) => j !== i) }));

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
            <div className="field-group">
              <label className="field-label">Clé Tavily <span style={{ fontWeight: 'normal', opacity: .6 }}>(recherche web)</span></label>
              <input className="field-input" type="password" value={form.tavily_key || ''} onChange={update('tavily_key')} placeholder="tvly-…" />
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
            <div className="field-hint" style={{ marginBottom: 8 }}>
              <span className="hint-icon">🔌</span>
              <span>Connecteurs MCP qui étendent les capacités du LLM (filesystem, calendrier, SIRH…)</span>
            </div>
            <div className="settings-mcp-list">
              {(form.mcp_servers || []).map((url, i) => (
                <div key={i} className="mcp-server-row">
                  <span className="mcp-server-url">{url}</span>
                  <button className="mcp-remove-btn" onClick={() => removeMcp(i)} title="Supprimer">✕</button>
                </div>
              ))}
              {(form.mcp_servers || []).length === 0 && (
                <div className="settings-mcp-empty">Aucun serveur configuré.</div>
              )}
            </div>
            <div className="mcp-add-row" style={{ marginTop: 8 }}>
              <input
                className="field-input" value={newMcp}
                onChange={e => setNewMcp(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && addMcp()}
                placeholder="https://mon-serveur-mcp.example.com"
              />
              <button className="btn-ghost" onClick={addMcp} style={{ flexShrink: 0 }}>Ajouter</button>
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
  servers: string[];
  onClose: () => void;
}

export function McpStatusPanel({ servers, onClose }: McpStatusPanelProps) {
  const [statuses, setStatuses] = useState<McpServerStatus[] | null>(null);
  const [loading, setLoading]   = useState(true);

  useEffect(() => {
    if (!servers || servers.length === 0) { setLoading(false); setStatuses([]); return; }
    setLoading(true);
    fetch(`${API_BASE}/api-proxy/api/mcp/tools`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ servers }),
    })
      .then(r => r.json())
      .then((data: { servers?: McpServerStatus[] }) => setStatuses(data.servers || []))
      .catch(() => setStatuses(servers.map(s => ({ server: s, status: 'error' as const, tools: [], tool_count: 0 }))))
      .finally(() => setLoading(false));
  }, [servers]);

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal mcp-status-modal" onClick={e => e.stopPropagation()} style={{ width: 560 }}>
        <div className="modal-header">
          <span className="modal-title">🔌 Serveurs MCP actifs</span>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="modal-body">
          {loading && <div style={{ textAlign: 'center', padding: '24px 0', opacity: .6 }}>Connexion aux serveurs…</div>}
          {!loading && (!statuses || statuses.length === 0) && (
            <div style={{ textAlign: 'center', padding: '24px 0', opacity: .6 }}>
              <div style={{ fontSize: 32, marginBottom: 8 }}>🔌</div>
              Aucun serveur MCP configuré.<br />
              <span style={{ fontSize: 12 }}>Ajoutez des serveurs dans les Paramètres.</span>
            </div>
          )}
          {!loading && statuses && statuses.map((srv, i) => (
            <div key={i} className={`mcp-status-card mcp-status-card--${srv.status}`}>
              <div className="mcp-status-header">
                <span className={`mcp-status-dot mcp-status-dot--${srv.status}`} />
                <span className="mcp-status-url">{srv.server}</span>
                <span className="mcp-status-badge">{srv.status === 'ok' ? `${srv.tool_count} outil${srv.tool_count > 1 ? 's' : ''}` : 'Injoignable'}</span>
              </div>
              {srv.tools?.length > 0 && (
                <div className="mcp-tools-list">
                  {srv.tools.map((t, j) => (
                    <div key={j} className="mcp-tool-item">
                      <span className="mcp-tool-name">{t.name}</span>
                      {t.description && <span className="mcp-tool-desc">{t.description}</span>}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
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
