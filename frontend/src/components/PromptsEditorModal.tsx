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


import React, { useState, useEffect, useRef } from 'react';
import ReactDOM from 'react-dom';
import { API_BASE, DOT_OPTIONS, ICON_OPTIONS } from '../constants';
import { slugify } from '../utils/text';

/* ── Contexte singleton : un seul IconPicker ouvert à la fois ────────────── */
const IconPickerCtx = React.createContext<{
  openId: string | null;
  setOpenId: (id: string | null) => void;
}>({ openId: null, setOpenId: () => {} });

let _iconPickerCounter = 0;

/* ── Sélecteur d'icône style gélule (portal — toujours au premier plan) ──── */
function IconPicker({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const id = useRef(`ip_${++_iconPickerCounter}`).current;
  const { openId, setOpenId } = React.useContext(IconPickerCtx);
  const open = openId === id;

  const [pos, setPos] = useState<{ top?: number; bottom?: number; left: number } | null>(null);
  const pillRef = useRef<HTMLButtonElement>(null);
  const popRef  = useRef<HTMLDivElement>(null);

  /* fermer au clic extérieur */
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      const t = e.target as Node;
      if (pillRef.current?.contains(t) || popRef.current?.contains(t)) return;
      setOpenId(null);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open]);

  /* fermer au scroll / resize */
  useEffect(() => {
    if (!open) return;
    const close = () => setOpenId(null);
    window.addEventListener('scroll', close, true);
    window.addEventListener('resize', close);
    return () => {
      window.removeEventListener('scroll', close, true);
      window.removeEventListener('resize', close);
    };
  }, [open]);

  const handleOpen = () => {
    if (open) { setOpenId(null); return; }
    if (pillRef.current) {
      const r = pillRef.current.getBoundingClientRect();
      const spaceBelow = window.innerHeight - r.bottom;
      if (spaceBelow < 260) {
        setPos({ bottom: window.innerHeight - r.top + 6, left: r.left });
      } else {
        setPos({ top: r.bottom + 6, left: r.left });
      }
    }
    setOpenId(id);
  };

  const popover = open && pos ? ReactDOM.createPortal(
    <div
      ref={popRef}
      className="pe-icon-popover"
      style={{ position: 'fixed', zIndex: 9999, ...pos }}
    >
      {ICON_OPTIONS.map(opt => (
        <button
          key={opt.value}
          type="button"
          title={opt.label}
          className={`pe-icon-cell${opt.value === value ? ' pe-icon-cell--active' : ''}`}
          onClick={() => { onChange(opt.value); setOpenId(null); }}
        >
          {opt.value}
        </button>
      ))}
    </div>,
    document.body
  ) : null;

  return (
    <div className="pe-icon-picker">
      <button
        ref={pillRef}
        type="button"
        className="pe-icon-pill"
        onClick={handleOpen}
        title="Choisir une icône"
      >
        <span className="pe-icon-pill-emoji">{value || '💬'}</span>
        <span className="pe-icon-pill-chevron">{open ? '▲' : '▼'}</span>
      </button>
      {popover}
    </div>
  );
}

interface Prompt {
  icon: string;
  label: string;
  prompt: string;
  _key: number;
}

interface Space {
  id: string;
  icon: string;
  label: string;
  dot: string;
  system: string;
  prompts: Prompt[];
  _key: number;
  _autoId?: boolean;
}

interface PromptsEditorModalProps {
  onClose: () => void;
  onSpacesUpdated: () => void;
}

const newPrompt = (): Prompt => ({ icon: '💬', label: '', prompt: '', _key: Math.random() });
const newSpace  = (): Space  => ({ id: '', icon: '💬', label: '', dot: '', system: '', prompts: [newPrompt()], _key: Math.random() });

export function PromptsEditorModal({ onClose, onSpacesUpdated }: PromptsEditorModalProps) {
  const [openPickerId, setOpenPickerId] = useState<string | null>(null);
  const [spaces, setSpaces]           = useState<Space[] | null>(null);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [saving, setSaving]           = useState(false);
  const [error, setError]             = useState('');
  const [dirty, setDirty]             = useState(false);
  const [dragOver, setDragOver]             = useState<number | null>(null);
  const [dragIdx, setDragIdx]               = useState<number | null>(null);
  const [dragSpaceIdx, setDragSpaceIdx]     = useState<number | null>(null);
  const [dragSpaceOver, setDragSpaceOver]   = useState<number | null>(null);

  useEffect(() => {
    fetch(`${API_BASE}/api-proxy/api/spaces`)
      .then(r => r.json())
      .then((data: { spaces?: Space[] }) => {
        const loaded = (data.spaces || []).map(s => ({
          ...s, _key: Math.random(),
          prompts: (s.prompts || []).map(p => ({ ...p, _key: Math.random() })),
        }));
        setSpaces(loaded);
        setSelectedIdx(0);
      })
      .catch(() => setError('Impossible de charger les espaces.'));
  }, []);

  const space     = spaces?.[selectedIdx] ?? null;
  const markDirty = () => setDirty(true);

  const updateSpace = (field: keyof Space, val: unknown) => {
    setSpaces(prev => prev!.map((s, i) => i === selectedIdx ? { ...s, [field]: val } : s));
    markDirty();
  };

  const addSpace    = () => { const ns = newSpace(); setSpaces(prev => [...prev!, ns]); setSelectedIdx(spaces!.length); markDirty(); };
  const deleteSpace = (idx: number) => {
    setSpaces(prev => prev!.filter((_, i) => i !== idx));
    setSelectedIdx(prev => Math.max(0, prev >= idx ? prev - 1 : prev));
    markDirty();
  };

  const updatePrompt = (pIdx: number, field: keyof Prompt, val: string) => {
    setSpaces(prev => prev!.map((s, i) => {
      if (i !== selectedIdx) return s;
      return { ...s, prompts: s.prompts.map((p, j) => j === pIdx ? { ...p, [field]: val } : p) };
    }));
    markDirty();
  };
  const addPrompt    = () => { setSpaces(prev => prev!.map((s, i) => i !== selectedIdx ? s : { ...s, prompts: [...s.prompts, newPrompt()] })); markDirty(); };
  const deletePrompt = (pIdx: number) => { setSpaces(prev => prev!.map((s, i) => i !== selectedIdx ? s : { ...s, prompts: s.prompts.filter((_, j) => j !== pIdx) })); markDirty(); };

  // ── Drag & drop via le conteneur parent ──────────────────────────────────────
  const onDragStart = (e: React.DragEvent, idx: number) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'BUTTON') { e.preventDefault(); return; }
    setDragIdx(idx);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(idx));
  };

  const onListDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const card = (e.target as HTMLElement).closest<HTMLElement>('[data-idx]');
    if (!card) return;
    const idx = parseInt(card.dataset.idx ?? '-1', 10);
    if (!isNaN(idx) && idx !== dragIdx) setDragOver(idx);
  };

  const onListDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
    if (e.currentTarget.contains(e.relatedTarget as Node)) return;
    setDragOver(null);
  };

  const onListDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const card = (e.target as HTMLElement).closest<HTMLElement>('[data-idx]');
    const toIdx = card ? parseInt(card.dataset.idx ?? '-1', 10) : -1;
    if (dragIdx === null || toIdx === -1 || dragIdx === toIdx) {
      setDragOver(null); setDragIdx(null); return;
    }
    setSpaces(prev => prev!.map((s, i) => {
      if (i !== selectedIdx) return s;
      const ps = [...s.prompts];
      const [moved] = ps.splice(dragIdx, 1);
      ps.splice(toIdx, 0, moved);
      return { ...s, prompts: ps };
    }));
    setDragOver(null); setDragIdx(null); markDirty();
  };

  // ── Drag & drop des espaces dans la sidebar ─────────────────────────────────
  const onSpaceDragStart = (e: React.DragEvent, idx: number) => {
    setDragSpaceIdx(idx);
    e.dataTransfer.effectAllowed = 'move';
    e.dataTransfer.setData('text/plain', String(idx));
  };

  const onSpaceListDragOver = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = 'move';
    const item = (e.target as HTMLElement).closest<HTMLElement>('[data-space-idx]');
    if (!item) return;
    const idx = parseInt(item.dataset.spaceIdx ?? '-1', 10);
    if (!isNaN(idx) && idx !== dragSpaceIdx) setDragSpaceOver(idx);
  };

  const onSpaceListDragLeave = (e: React.DragEvent<HTMLDivElement>) => {
    if (e.currentTarget.contains(e.relatedTarget as Node)) return;
    setDragSpaceOver(null);
  };

  const onSpaceListDrop = (e: React.DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    const item = (e.target as HTMLElement).closest<HTMLElement>('[data-space-idx]');
    const toIdx = item ? parseInt(item.dataset.spaceIdx ?? '-1', 10) : -1;
    if (dragSpaceIdx === null || toIdx === -1 || dragSpaceIdx === toIdx) {
      setDragSpaceOver(null); setDragSpaceIdx(null); return;
    }
    setSpaces(prev => {
      const next = [...prev!];
      const [moved] = next.splice(dragSpaceIdx, 1);
      next.splice(toIdx, 0, moved);
      return next;
    });
    // Maintenir la sélection sur le même espace après le déplacement
    setSelectedIdx(toIdx);
    setDragSpaceOver(null); setDragSpaceIdx(null); markDirty();
  };

  const handleSave = async () => {
    setSaving(true); setError('');
    try {
      const usedIds = new Set<string>();
      const spacesWithIds = spaces!.map(s => {
        let id = s.id && s.id.trim() ? s.id.trim() : slugify(s.label || 'espace');
        let candidate = id; let n = 2;
        while (usedIds.has(candidate)) { candidate = `${id}-${n++}`; }
        usedIds.add(candidate);
        return { ...s, id: candidate };
      });
      const payload = {
        spaces: spacesWithIds.map(({ _key, _autoId, prompts, ...s }) => ({
          ...s, prompts: prompts.map(({ _key: _k, ...p }) => p),
        })),
      };
      const r = await fetch(`${API_BASE}/api-proxy/api/spaces`, {
        method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(payload),
      });
      if (!r.ok) { const d = await r.json(); throw new Error(d.detail || 'Erreur serveur'); }
      setDirty(false);
      onSpacesUpdated();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <IconPickerCtx.Provider value={{ openId: openPickerId, setOpenId: setOpenPickerId }}>
    <div className="modal-overlay">
      <div className="modal pe-modal">

        <div className="modal-header">
          <div className="modal-header-left">
            <span className="modal-icon">✏️</span>
            <span className="modal-title">Éditeur d'espaces &amp; prompts</span>
            {dirty && <span className="pe-dirty-badge">● non sauvegardé</span>}
          </div>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>

        <div className="pe-body">

          {/* ── Sidebar ── */}
          <div className="pe-sidebar">
            <div className="pe-sidebar-title">Espaces <span className="pe-section-hint">— glissez ⠿ pour réordonner</span></div>
            <div
              className="pe-space-list"
              onDragOver={onSpaceListDragOver}
              onDragLeave={onSpaceListDragLeave}
              onDrop={onSpaceListDrop}
            >
              {spaces === null
                ? <div className="pe-loading">Chargement…</div>
                : spaces.map((s, i) => (
                  <div
                    key={s._key}
                    data-space-idx={i}
                    className={`pe-space-item${i === selectedIdx ? ' pe-space-item--active' : ''}${dragSpaceOver === i ? ' pe-space-item--dragover' : ''}${dragSpaceIdx === i ? ' pe-space-item--dragging' : ''}`}
                    draggable
                    onDragStart={e => onSpaceDragStart(e, i)}
                    onDragEnd={() => { setDragSpaceOver(null); setDragSpaceIdx(null); }}
                    onClick={() => setSelectedIdx(i)}
                  >
                    <span className="pe-drag-handle pe-space-drag-handle" title="Glisser pour réordonner">⠿</span>
                    <span className="pe-space-icon">{s.icon || '💬'}</span>
                    <div className="pe-space-meta">
                      <span className="pe-space-label">{s.label || <em style={{ opacity: .5 }}>Sans nom</em>}</span>
                      {s.id && <span className="pe-space-id">{s.id}</span>}
                    </div>
                    {s.dot && <span className={`dot ${s.dot}`} style={{ marginLeft: 'auto', flexShrink: 0 }} />}
                    <button
                      className="pe-del-btn"
                      title="Supprimer cet espace"
                      onClick={e => { e.stopPropagation(); deleteSpace(i); }}
                    >✕</button>
                  </div>
                ))
              }
            </div>
            <button className="pe-add-space-btn" onClick={addSpace}>
              <svg viewBox="0 0 16 16" fill="currentColor" width="12" height="12">
                <path d="M8 2a1 1 0 011 1v4h4a1 1 0 110 2H9v4a1 1 0 11-2 0V9H3a1 1 0 110-2h4V3a1 1 0 011-1z"/>
              </svg>
              Nouvel espace
            </button>
          </div>

          {/* ── Éditeur ── */}
          <div className="pe-editor">
            {space === null ? (
              <div className="pe-empty">
                <div style={{ fontSize: 32, marginBottom: 12 }}>✏️</div>
                Sélectionnez un espace dans la liste<br />ou créez-en un nouveau.
              </div>
            ) : (
              <>
                <section className="pe-section">
                  <div className="pe-section-title">Identité de l'espace</div>
                  <div className="pe-fields-row">
                    <div className="field-group pe-field-icon">
                      <label className="field-label">Icône</label>
                      <IconPicker value={space.icon} onChange={v => updateSpace('icon', v)} />
                    </div>
                    <div className="field-group" style={{ flex: 1 }}>
                      <label className="field-label">Nom affiché</label>
                      <input className="field-input" value={space.label} onChange={e => {
                        const newLabel = e.target.value;
                        updateSpace('label', newLabel);
                        if (!space.id || space._autoId) {
                          setSpaces(prev => prev!.map((s, i) => i !== selectedIdx ? s : { ...s, label: newLabel, id: slugify(newLabel), _autoId: true }));
                        }
                      }} placeholder="ex : Assistant RH" />
                    </div>
                    <div className="field-group pe-field-slug">
                      <label className="field-label">Identifiant <span className="pe-field-hint">clé interne</span></label>
                      <input className="field-input pe-id-input" value={space.id} onChange={e => {
                        setSpaces(prev => prev!.map((s, i) => i !== selectedIdx ? s : { ...s, id: e.target.value.replace(/\s+/g, '-').toLowerCase(), _autoId: false }));
                        markDirty();
                      }} placeholder="mon-espace" />
                    </div>
                  </div>
                  <div className="field-group" style={{ marginTop: 4 }}>
                    <label className="field-label">Badge couleur</label>
                    <div className="pe-dot-row">
                      {DOT_OPTIONS.map(opt => (
                        <button key={opt.value} className={`pe-dot-btn ${space.dot === opt.value ? 'pe-dot-btn--active' : ''}`} onClick={() => updateSpace('dot', opt.value)}>
                          <span className="pe-dot-swatch" style={{ background: opt.color, border: opt.border ? `1px dashed ${opt.border}` : 'none' }} />
                          {opt.label}
                        </button>
                      ))}
                    </div>
                  </div>
                </section>

                <section className="pe-section">
                  <div className="pe-section-title">
                    System prompt
                    <span className="pe-section-hint">— instructions envoyées au LLM à chaque échange</span>
                  </div>
                  <textarea className="pe-system-textarea" value={space.system} onChange={e => updateSpace('system', e.target.value)} placeholder="Tu es un assistant RH expert. Tu travailles pour…" rows={10} />
                  <div className="pe-char-count">{space.system?.length ?? 0} caractères</div>
                </section>

                <section className="pe-section">
                  <div className="pe-section-title">
                    Prompts rapides
                    <span className="pe-section-hint">— glissez ⠿ pour réordonner</span>
                  </div>

                  <div
                    className="pe-prompts-list"
                    onDragOver={onListDragOver}
                    onDragLeave={onListDragLeave}
                    onDrop={onListDrop}
                  >
                    {space.prompts.map((p, pi) => (
                      <div
                        key={p._key}
                        data-idx={pi}
                        className={`pe-prompt-card${dragOver === pi ? ' pe-prompt-card--dragover' : ''}${dragIdx === pi ? ' pe-prompt-card--dragging' : ''}`}
                        draggable
                        onDragStart={e => onDragStart(e, pi)}
                        onDragEnd={() => { setDragOver(null); setDragIdx(null); }}
                      >
                        <div className="pe-prompt-card-header">
                          <span className="pe-drag-handle" title="Glisser pour déplacer">⠿</span>
                          <span onMouseDown={e => e.stopPropagation()}>
                            <IconPicker value={p.icon} onChange={v => updatePrompt(pi, 'icon', v)} />
                          </span>
                          <input className="field-input pe-prompt-label" value={p.label} onChange={e => updatePrompt(pi, 'label', e.target.value)} placeholder="Titre affiché…" maxLength={40} onMouseDown={e => e.stopPropagation()} />
                          <button className="pe-del-btn" title="Supprimer" onClick={() => deletePrompt(pi)}>✕</button>
                        </div>
                        <div className="pe-prompt-text-row">
                          <input className="field-input pe-prompt-text-input" value={p.prompt} onChange={e => updatePrompt(pi, 'prompt', e.target.value)} placeholder="Texte envoyé au LLM lorsque l'utilisateur clique…" onMouseDown={e => e.stopPropagation()} />
                        </div>
                      </div>
                    ))}
                  </div>

                  <button className="pe-add-prompt-btn" onClick={addPrompt}>
                    <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11">
                      <path d="M8 2a1 1 0 011 1v4h4a1 1 0 110 2H9v4a1 1 0 11-2 0V9H3a1 1 0 110-2h4V3a1 1 0 011-1z"/>
                    </svg>
                    Ajouter un prompt
                  </button>
                </section>
              </>
            )}
          </div>
        </div>

        <div className="modal-footer">
          {error && <span className="pe-error">{error}</span>}
          <button className="btn-ghost" onClick={onClose}>Fermer</button>
          <button className="btn-primary" onClick={handleSave} disabled={saving || !dirty}>
            {saving ? 'Sauvegarde…' : 'Enregistrer'}
          </button>
        </div>
      </div>
    </div>
    </IconPickerCtx.Provider>
  );
}
