import React, { useState, useEffect } from 'react';
import { API_BASE } from '../constants';

// ── Types ─────────────────────────────────────────────────────────────────────

export interface Settings {
  endpoint: string;
  bearer: string;
  model: string;
  tavily_key?: string;
  mcp_servers?: string[];
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

// ── SettingsModal ─────────────────────────────────────────────────────────────

interface SettingsModalProps {
  settings: Settings;
  onSave: (settings: Settings) => void;
  onClose: () => void;
}

export function SettingsModal({ settings, onSave, onClose }: SettingsModalProps) {
  const [form, setForm]     = useState<Settings>({ ...settings, mcp_servers: settings.mcp_servers || [] });
  const [newMcp, setNewMcp] = useState('');

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
      <div className="modal">
        <div className="modal-header">
          <div className="modal-header-left"><span className="modal-icon">⚙️</span><span className="modal-title">Configuration</span></div>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>
        <div className="modal-body">
          <div className="settings-section-title">LLM</div>
          <div className="field-group"><label className="field-label">Endpoint LLM</label><input className="field-input" value={form.endpoint} onChange={update('endpoint')} placeholder="https://albert.api.etalab.gouv.fr/v1" /></div>
          <div className="field-group"><label className="field-label">Bearer Token</label><input className="field-input" type="password" value={form.bearer} onChange={update('bearer')} placeholder="sk-…" /></div>
          <div className="field-group"><label className="field-label">Modèle</label><input className="field-input" value={form.model} onChange={update('model')} placeholder="openai/gpt-oss-120b" /></div>
          <div className="field-group">
            <label className="field-label">Clé Tavily <span style={{ fontWeight: 'normal', opacity: .6 }}>(recherche web)</span></label>
            <input className="field-input" type="password" value={form.tavily_key || ''} onChange={update('tavily_key')} placeholder="tvly-…" />
          </div>

          <div className="settings-section-title" style={{ marginTop: 18 }}>Serveurs MCP</div>
          <div className="field-hint" style={{ marginBottom: 8 }}><span className="hint-icon">🔌</span><span>Connecteurs MCP qui étendent les capacités du LLM (filesystem, calendrier, SIRH…)</span></div>
          {(form.mcp_servers || []).map((url, i) => (
            <div key={i} className="mcp-server-row">
              <span className="mcp-server-url">{url}</span>
              <button className="mcp-remove-btn" onClick={() => removeMcp(i)} title="Supprimer">✕</button>
            </div>
          ))}
          <div className="mcp-add-row">
            <input
              className="field-input" value={newMcp}
              onChange={e => setNewMcp(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && addMcp()}
              placeholder="https://mon-serveur-mcp.example.com"
            />
            <button className="btn-ghost" onClick={addMcp} style={{ flexShrink: 0 }}>Ajouter</button>
          </div>
          <div className="field-hint" style={{ marginTop: 12 }}><span className="hint-icon">💡</span><span>Ces paramètres sont sauvegardés localement et ne sont jamais transmis à un tiers.</span></div>
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
            <div className="about-row"><span className="about-key">Version</span><span className="about-val">v2.0 public</span></div>
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
