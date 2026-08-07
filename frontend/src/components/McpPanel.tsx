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

import React, { useEffect, useState, useCallback } from 'react';
import { API_BASE } from '../constants';
import type { Settings, StdioServerInfo } from './Modals';

interface McpPanelProps {
  settings: Settings;
  /// Sauvegarde les settings (localStorage + sync backend via IPC). Retourne
  /// une promesse pour permettre d'attendre que le backend soit à jour avant
  /// de rafraîchir la liste des serveurs découverts.
  onSave: (settings: Settings) => Promise<void> | void;
  onClose: () => void;
}

const stdioUrl = (id: string) => `stdio:${id}`;

export function McpPanel({ settings, onSave, onClose }: McpPanelProps) {
  const [form, setForm] = useState<Settings>({
    ...settings,
    mcp_servers: settings.mcp_servers || [],
    mcp_stdio_config: settings.mcp_stdio_config || {},
  });
  const [discovered, setDiscovered] = useState<StdioServerInfo[] | null>(null);
  const [loading, setLoading]       = useState(true);
  const [installing, setInstalling] = useState(false);
  const [installMsg, setInstallMsg] = useState<string | null>(null);
  const [expanded, setExpanded]     = useState<string | null>(null);
  const [newMcp, setNewMcp]         = useState('');
  const [newMcpAlias, setNewMcpAlias] = useState('');
  const [packStatus, setPackStatus] = useState<{ installed: string | null; latest: string | null; update_available: boolean } | null>(null);
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updating, setUpdating]     = useState(false);

  const refreshDiscovered = useCallback(async () => {
    setLoading(true);
    try {
      const res  = await fetch(`${API_BASE}/api-proxy/api/mcp/stdio/servers`);
      const data: { dir?: string | null; servers?: StdioServerInfo[] } = await res.json();
      setDiscovered(data.servers || []);
    } catch {
      setDiscovered([]);
    } finally {
      setLoading(false);
    }
  }, []);

  const checkPackUpdate = useCallback(async () => {
    setCheckingUpdate(true);
    try {
      const res  = await fetch(`${API_BASE}/api-proxy/api/mcp/stdio/pack-update`);
      const data = await res.json();
      setPackStatus(data);
    } catch {
      setPackStatus(null);
    } finally {
      setCheckingUpdate(false);
    }
  }, []);

  useEffect(() => { refreshDiscovered(); }, [refreshDiscovered]);
  useEffect(() => { checkPackUpdate(); }, [checkPackUpdate]);

  // Persiste un nouvel état de settings (localStorage + backend), puis
  // rafraîchit la liste découverte (le backend peut désormais pointer vers
  // un nouveau dossier, ou avoir de nouvelles valeurs de config).
  const persist = async (next: Settings) => {
    setForm(next);
    await onSave(next);
  };

  const chooseDirectory = async () => {
    const dialog = (window as any).__TAURI__?.dialog;
    if (!dialog?.open) {
      alert("Le sélecteur de dossier n'est disponible que dans l'application desktop Demeter.");
      return;
    }
    const picked = await dialog.open({ directory: true, multiple: false, title: 'Dossier de vos serveurs MCP' });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    await persist({ ...form, mcp_dir: path });
    await refreshDiscovered();
  };

  const installOfficial = async () => {
    setInstalling(true);
    setInstallMsg(null);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/mcp/stdio/install-official`, { method: 'POST' });
      const data = await res.json();
      if (!res.ok) {
        setInstallMsg(data?.error || 'Échec de l\'installation.');
      } else {
        const installed: string[] = data.installed || [];
        setInstallMsg(installed.length ? `Installés : ${installed.join(', ')}` : 'Aucun nouveau MCP à installer.');
        await refreshDiscovered();
        await checkPackUpdate();
      }
    } catch {
      setInstallMsg('Échec de l\'installation.');
    } finally {
      setInstalling(false);
      setTimeout(() => setInstallMsg(null), 5000);
    }
  };

  const applyPackUpdate = async () => {
    setUpdating(true);
    setInstallMsg(null);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/mcp/stdio/pack-update`, { method: 'POST' });
      const data = await res.json();
      if (!res.ok) {
        setInstallMsg(data?.error || 'Échec de la mise à jour.');
      } else {
        const installed: string[] = data.installed || [];
        setInstallMsg(installed.length ? `Mis à jour : ${installed.join(', ')}` : 'Rien à mettre à jour.');
        await refreshDiscovered();
        await checkPackUpdate();
      }
    } catch {
      setInstallMsg('Échec de la mise à jour (vérifiez votre connexion).');
    } finally {
      setUpdating(false);
      setTimeout(() => setInstallMsg(null), 5000);
    }
  };

  const isStdioEnabled = (id: string) =>
    (form.mcp_servers || []).some(s => s.url === stdioUrl(id));

  const toggleStdio = async (srv: StdioServerInfo) => {
    if (isStdioEnabled(srv.id)) {
      await persist({ ...form, mcp_servers: (form.mcp_servers || []).filter(s => s.url !== stdioUrl(srv.id)) });
      fetch(`${API_BASE}/api-proxy/api/mcp/stdio/stop`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ id: srv.id }),
      }).catch(() => {});
    } else {
      await persist({ ...form, mcp_servers: [...(form.mcp_servers || []), { url: stdioUrl(srv.id), alias: srv.name }] });
    }
  };

  const setParamValue = (serverId: string, key: string, value: string) => {
    setForm(f => ({
      ...f,
      mcp_stdio_config: {
        ...(f.mcp_stdio_config || {}),
        [serverId]: { ...((f.mcp_stdio_config || {})[serverId] || {}), [key]: value },
      },
    }));
  };

  const saveParams = async (serverId: string) => {
    await persist(form);
    // La config ne prend effet qu'au (re)démarrage du process — on l'arrête
    // pour qu'il redémarre avec les nouvelles valeurs au prochain appel.
    fetch(`${API_BASE}/api-proxy/api/mcp/stdio/stop`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ id: serverId }),
    }).catch(() => {});
    setExpanded(null);
  };

  const addMcp = () => {
    let url = newMcp.trim();
    if (!url) return;
    if (!/^https?:\/\//i.test(url)) url = 'http://' + url;
    if ((form.mcp_servers || []).some(s => s.url === url)) { setNewMcp(''); setNewMcpAlias(''); return; }
    const alias = newMcpAlias.trim() || undefined;
    persist({ ...form, mcp_servers: [...(form.mcp_servers || []), { url, alias }] });
    setNewMcp('');
    setNewMcpAlias('');
  };

  const removeMcp = (i: number) =>
    persist({ ...form, mcp_servers: (form.mcp_servers || []).filter((_, j) => j !== i) });

  const updateMcpAlias = (i: number, alias: string) =>
    setForm(f => ({
      ...f,
      mcp_servers: (f.mcp_servers || []).map((s, j) => j === i ? { ...s, alias: alias.trim() || undefined } : s),
    }));

  const httpServers = (form.mcp_servers || []).filter(s => !s.url.startsWith('stdio:'));

  // Entrées "stdio:xxx" enregistrées dans les settings mais dont l'id ne
  // correspond plus à aucun serveur présent dans le dossier MCP (renommage,
  // dossier supprimé/déplacé, ancienne version…). Invisibles dans la section
  // "Serveurs découverts" (qui n'affiche que ce qui existe sur disque) ET
  // dans "MCP distants" (qui exclut tout ce qui commence par stdio:) — sans
  // cette section, impossible de les retirer depuis l'interface.
  const discoveredIds = new Set((discovered || []).map(s => s.id));
  const orphanedStdio = (form.mcp_servers || [])
    .map((s, i) => ({ s, i, id: s.url.startsWith('stdio:') ? s.url.slice('stdio:'.length) : null }))
    .filter(({ id }) => id !== null && !discoveredIds.has(id));

  return (
    <div className="modal-overlay">
      <div className="modal settings-modal mcp-panel-modal">
        <div className="modal-header">
          <div className="modal-header-left"><span className="modal-icon">🔌</span><span className="modal-title">Serveurs MCP</span></div>
          <button className="icon-btn" onClick={() => { persist(form); onClose(); }}>✕</button>
        </div>

        <div className="modal-body mcp-panel-body">

          {/* ── Dossier des serveurs MCP ── */}
          <div className="settings-section-title">Dossier des serveurs MCP</div>
          <div className="field-hint">
            <span className="hint-icon">📁</span>
            <span>Choisissez le dossier où résident vos serveurs MCP stdio (un sous-dossier <code>mcp-*</code> par serveur, avec son <code>manifest.json</code>).</span>
          </div>
          <div className="mcp-dir-row">
            <div className="mcp-dir-path" title={form.mcp_dir}>
              {form.mcp_dir || <span className="mcp-dir-path--empty">Aucun dossier configuré</span>}
            </div>
            <button className="mcp-add-btn" onClick={chooseDirectory}>
              {form.mcp_dir ? 'Changer…' : 'Choisir un dossier…'}
            </button>
            {form.mcp_dir && (
              <button className="mcp-add-btn mcp-add-btn--ghost" onClick={installOfficial} disabled={installing}>
                {installing ? 'Installation…' : 'Installer les MCP officiels'}
              </button>
            )}
            {form.mcp_dir && packStatus?.update_available && (
              <button className="mcp-add-btn" onClick={applyPackUpdate} disabled={updating} title={`Version installée : ${packStatus.installed ?? 'aucune'} → ${packStatus.latest}`}>
                {updating ? 'Mise à jour…' : `Mettre à jour (v${packStatus.latest})`}
              </button>
            )}
          </div>
          {form.mcp_dir && !checkingUpdate && packStatus && !packStatus.update_available && packStatus.installed && (
            <div className="field-hint mcp-pack-uptodate">
              <span className="hint-icon">✅</span>
              <span>MCP officiels à jour (v{packStatus.installed}).</span>
            </div>
          )}
          {installMsg && <div className="field-hint mcp-install-msg">{installMsg}</div>}

          {/* ── Serveurs découverts (stdio) ── */}
          {form.mcp_dir && (
            <>
              <div className="settings-section-subtitle" style={{ marginTop: 18 }}>
                Serveurs découverts
                <span className="field-label-hint">lancés automatiquement par Demeter</span>
              </div>

              {loading && <div className="mcp-empty-state"><span className="mcp-empty-icon">⏳</span><span>Recherche des serveurs…</span></div>}

              {!loading && discovered !== null && discovered.length === 0 && (
                <div className="mcp-empty-state">
                  <span className="mcp-empty-icon">📭</span>
                  <span>Aucun serveur trouvé dans ce dossier — ajoutez-y un dossier <code>mcp-*</code> avec un <code>manifest.json</code>, ou installez les MCP officiels ci-dessus.</span>
                </div>
              )}

              {!loading && discovered && discovered.length > 0 && (
                <div className="mcp-cards-list mcp-cards-list--panel">
                  {discovered.map(srv => {
                    const enabled = isStdioEnabled(srv.id);
                    const isOpen  = expanded === srv.id;
                    const values  = (form.mcp_stdio_config || {})[srv.id] || {};
                    const missingRequired = srv.params.some(p => p.required && !(values[p.key] || '').trim());
                    return (
                      <div key={srv.id} className={`mcp-card mcp-card--toggle${enabled ? ' mcp-card--enabled' : ''}${!srv.available ? ' mcp-card--disabled' : ''}`}>
                        <div className="mcp-card__accent" />
                        <div className="mcp-card__body">
                          <label className="mcp-card__top" style={{ cursor: srv.available ? 'pointer' : 'not-allowed' }}>
                            <input
                              type="checkbox"
                              checked={enabled}
                              disabled={!srv.available}
                              onChange={() => toggleStdio(srv)}
                            />
                            <span className="mcp-card__icon">🖥️</span>
                            <span className="mcp-card__alias-input" style={{ display: 'flex', alignItems: 'center' }}>
                              {srv.name}
                            </span>
                            {missingRequired && srv.params.length > 0 && (
                              <span className="mcp-config-warning" title="Configuration requise">⚠</span>
                            )}
                            {srv.params.length > 0 && (
                              <button
                                type="button"
                                className="mcp-configure-btn"
                                onClick={e => { e.preventDefault(); e.stopPropagation(); setExpanded(isOpen ? null : srv.id); }}
                              >
                                {isOpen ? 'Fermer' : 'Configurer'}
                              </button>
                            )}
                          </label>
                          {srv.description && <span className="mcp-card__url">{srv.description}</span>}
                          {!srv.available && (
                            <span className="mcp-card__url" style={{ color: '#dc2626' }}>Indisponible sur cette plateforme</span>
                          )}

                          {isOpen && srv.params.length > 0 && (
                            <div className="mcp-config-form">
                              {srv.params.map(p => (
                                <div className="field-group" key={p.key}>
                                  <label className="field-label">
                                    {p.label}{p.required && <span className="mcp-required-star"> *</span>}
                                  </label>
                                  <input
                                    className="field-input"
                                    type={p.secret ? 'password' : 'text'}
                                    value={values[p.key] || ''}
                                    onChange={e => setParamValue(srv.id, p.key, e.target.value)}
                                    placeholder={p.default || ''}
                                    spellCheck={false}
                                  />
                                  {p.help && <div className="mcp-param-help">{p.help}</div>}
                                </div>
                              ))}
                              <button className="mcp-add-btn" onClick={() => saveParams(srv.id)}>Enregistrer</button>
                            </div>
                          )}
                        </div>
                      </div>
                    );
                  })}
                </div>
              )}
            </>
          )}

          {!form.mcp_dir && (
            <div className="mcp-empty-state" style={{ marginTop: 12 }}>
              <span className="mcp-empty-icon">👆</span>
              <span>Choisissez d'abord un dossier pour voir les serveurs MCP disponibles.</span>
            </div>
          )}

          {/* ── Entrées stdio orphelines (id sans serveur découvert correspondant) ── */}
          {orphanedStdio.length > 0 && (
            <>
              <div className="settings-section-subtitle" style={{ marginTop: 20 }}>
                Entrées orphelines
                <span className="field-label-hint">configurées mais introuvables sur disque — à nettoyer</span>
              </div>
              <div className="mcp-cards-list mcp-cards-list--panel">
                {orphanedStdio.map(({ s, i, id }) => (
                  <div key={i} className="mcp-card mcp-card--disabled">
                    <div className="mcp-card__accent" />
                    <div className="mcp-card__body">
                      <div className="mcp-card__top">
                        <span className="mcp-card__icon">⚠️</span>
                        <span className="mcp-card__alias-input">
                          {s.alias || id}
                          <span className="field-label-hint" style={{ display: 'block' }}>
                            id « {id} » — aucun serveur correspondant dans le dossier MCP actuel
                          </span>
                        </span>
                        <button className="icon-btn" title="Supprimer cette entrée" onClick={() => removeMcp(i)}>🗑️</button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </>
          )}

          {/* ── MCP distants (HTTP) ── */}
          <div className="settings-section-subtitle" style={{ marginTop: 20 }}>
            MCP distants (HTTP)
          </div>
          <div className="mcp-cards-list mcp-cards-list--panel">
            {httpServers.length === 0 && (
              <div className="mcp-empty-state">
                <span className="mcp-empty-icon">🔌</span>
                <span>Aucun serveur configuré</span>
              </div>
            )}
            {(form.mcp_servers || []).map((srv, i) => srv.url.startsWith('stdio:') ? null : (
              <div key={i} className="mcp-card">
                <div className="mcp-card__accent" />
                <div className="mcp-card__body">
                  <div className="mcp-card__top">
                    <span className="mcp-card__icon">⚡</span>
                    <input
                      className="mcp-card__alias-input"
                      value={srv.alias || ''}
                      onChange={e => updateMcpAlias(i, e.target.value)}
                      onBlur={() => persist(form)}
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

          <div className="field-hint">
            <span className="hint-icon">💡</span>
            <span>Ces paramètres sont sauvegardés localement (préférences Demeter) et ne sont jamais transmis à un tiers.</span>
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-primary" onClick={() => { persist(form); onClose(); }}>Fermer</button>
        </div>
      </div>
    </div>
  );
}
