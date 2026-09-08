// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Ingestion de documents dans une collection RAG (Qdrant).
//  • Dépose/choisit des documents (PDF/DOCX/TXT…) et/ou des archives .zip/.7z.
//  • Les archives sont décompressées côté serveur → batch de staging (VFS
//    éphémère, fichiers spoolés sur disque). On peut lister / SUPPRIMER des
//    documents du staging avant ingestion.
//  • « Ingérer » vectorise chaque document restant dans la collection Qdrant,
//    puis le batch est purgé. On peut aussi supprimer durablement un document
//    déjà ingéré (depuis la liste de la collection).
// ============================================================================

import React, { useState, useCallback, useEffect } from 'react';
import { API_BASE } from '../constants';
import { useDialog } from '../DialogContext';

interface Collection { id: string; name: string; is_owner: boolean; }

interface StagedFile { id: string; name: string; ext: string; size: number; }

interface DocRow { id: string; name: string; filename?: string; chunks_count?: number; }

interface IngestFileResult { name: string; ok: boolean; id?: string; chunks?: number; error?: string; }

const UPLOAD_ACCEPT = '.zip,.7z,.pdf,.docx,.doc,.txt,.md,.markdown,.rst,.csv';
const pretty = (n: number) => n > 1024 * 1024
  ? `${(n / (1024 * 1024)).toFixed(1)} Mo`
  : n > 1024 ? `${(n / 1024).toFixed(0)} Ko` : `${n} o`;

export function IngestionDocsModal({ onClose }: { onClose: () => void }) {
  const { toast: toastFn } = useDialog();
  const showToast = useCallback((m: string, t: 'success' | 'error' = 'success') => toastFn(m, t), [toastFn]);

  const [collections, setCollections] = useState<Collection[]>([]);
  const [colName, setColName] = useState('');
  const [loadingCols, setLoadingCols] = useState(false);

  const [docs, setDocs] = useState<DocRow[]>([]);
  const [loadingDocs, setLoadingDocs] = useState(false);
  const [confirmDelId, setConfirmDelId] = useState<string | null>(null);

  const [staging, setStaging] = useState<{ batch: string; files: StagedFile[]; errors: string[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [fileBusy, setFileBusy] = useState(false);
  const fileInputRef = React.useRef<HTMLInputElement>(null);

  const loadCollections = useCallback(async () => {
    setLoadingCols(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections?endpoint=qdrant`);
      if (!res.ok) throw new Error(await res.text());
      const data: { data?: Collection[] } = await res.json();
      const list = (data.data || []).sort((a, b) => a.name.localeCompare(b.name));
      setCollections(list);
      const mine = list.find(c => c.is_owner) ?? list[0];
      setColName(prev => (prev && list.some(c => c.name === prev) ? prev : mine?.name ?? ''));
    } catch (e) {
      showToast(`Erreur chargement collections : ${(e as Error).message}`, 'error');
    } finally {
      setLoadingCols(false);
    }
  }, [showToast]);

  useEffect(() => { loadCollections(); }, [loadCollections]);

  const loadDocs = useCallback(async (name: string) => {
    if (!name) return;
    setLoadingDocs(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections/${encodeURIComponent(name)}/documents`);
      if (!res.ok) throw new Error(await res.text());
      const data: { data?: DocRow[] } = await res.json();
      setDocs(data.data || []);
    } catch (e) {
      showToast(`Erreur chargement documents : ${(e as Error).message}`, 'error');
    } finally {
      setLoadingDocs(false);
    }
  }, [showToast]);

  useEffect(() => {
    if (colName) { setDocs([]); loadDocs(colName); }
  }, [colName, loadDocs]);

  const deleteDoc = useCallback(async (doc: DocRow) => {
    if (!colName) return;
    try {
      const res = await fetch(
        `${API_BASE}/api-proxy/api/ingestion/documents/${encodeURIComponent(doc.id)}?endpoint=${encodeURIComponent(colName)}`,
        { method: 'DELETE' },
      );
      if (!res.ok) throw new Error(await res.text());
      showToast(`Document supprimé : ${doc.name || doc.filename || doc.id}`);
      loadDocs(colName);
    } catch (e) {
      showToast(`Erreur suppression : ${(e as Error).message}`, 'error');
    } finally {
      setConfirmDelId(null);
    }
  }, [colName, loadDocs, showToast]);

  const handleFiles = useCallback(async (files: File[]) => {
    if (!files.length) return;
    if (!colName) { showToast('Choisissez une collection cible.', 'error'); return; }
    setFileBusy(true);
    setStaging(null);
    try {
      const form = new FormData();
      files.forEach(f => form.append('files', f));
      const res = await fetch(`${API_BASE}/api-proxy/api/uploads/stage`, { method: 'POST', body: form });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(body.detail || body.error || res.statusText);
      const errs: string[] = (body.errors || []).map((e: { error?: string }) => e?.error || 'Erreur');
      setStaging({
        batch: body.batch_id,
        files: body.files || [],
        errors: errs,
      });
      if ((body.files || []).length === 0 && errs.length) {
        showToast(`Aucun document exploitable :\n${errs.join('\n')}`, 'error');
      }
    } catch (e) {
      showToast(`Erreur décompression : ${(e as Error).message}`, 'error');
    } finally {
      setFileBusy(false);
    }
  }, [colName, showToast]);

  const removeStaged = useCallback(async (f: StagedFile) => {
    if (!staging) return;
    try {
      const res = await fetch(
        `${API_BASE}/api-proxy/api/uploads/batches/${staging.batch}/files/${f.id}`,
        { method: 'DELETE' },
      );
      if (!res.ok) throw new Error(await res.text());
      const body = await res.json();
      setStaging(prev => prev ? { ...prev, files: body.files || [] } : prev);
    } catch (e) {
      showToast(`Erreur suppression staging : ${(e as Error).message}`, 'error');
    }
  }, [staging, showToast]);

  const cancelBatch = useCallback(async () => {
    if (!staging) return;
    try {
      await fetch(`${API_BASE}/api-proxy/api/uploads/batches/${staging.batch}`, { method: 'DELETE' });
    } catch { /* best effort */ }
    setStaging(null);
  }, [staging]);

  const ingestBatch = useCallback(async () => {
    if (!staging || !colName) return;
    if (!staging.files.length) { showToast('Le staging est vide.', 'error'); return; }
    setBusy(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/uploads/batches/${staging.batch}/ingest`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ collection_id: colName, chunk_size: 2048, chunk_overlap: 0 }),
      });
      const body = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(body.detail || body.error || res.statusText);
      const results: IngestFileResult[] = body.files || [];
      const ok = results.filter(r => r.ok);
      const ko = results.filter(r => !r.ok);
      if (ko.length) {
        showToast(
          `Ingérés : ${ok.length}/${results.length}\nErreurs :\n${ko.map(r => `• ${r.name} : ${r.error || '?'}`).join('\n')}`,
          'error',
        );
      } else {
        showToast(`${ok.length} document(s) ingéré(s) dans « ${colName} ».`);
      }
      setStaging(null);
      loadDocs(colName);
    } catch (e) {
      showToast(`Erreur ingestion : ${(e as Error).message}`, 'error');
    } finally {
      setBusy(false);
    }
  }, [staging, colName, loadDocs, showToast]);

  return (
    <div className="modal-overlay">
      <div className="modal ingestion-modal ingestion-docs-modal">
        <div className="modal-header">
          <div className="modal-header-left">
            <span className="modal-icon">📥</span>
            <span className="modal-title">Ingérer des documents</span>
            <span className="ingestion-cache-info">
              <button className="ingestion-refresh-btn" onClick={() => { loadCollections(); if (colName) loadDocs(colName); }} title="Recharger" style={{ border: 'none', background: 'transparent' }}>🔄</button>
            </span>
          </div>
          <button className="icon-btn" onClick={onClose} title="Fermer">✕</button>
        </div>

        <div className="modal-body ingestion-body">
          {/* ── Collection cible ── */}
          <div className="ingestion-section">
            <label className="field-label">Collection cible</label>
            {loadingCols ? (
              <div className="ingestion-empty"><span className="spinner-sm" /> Chargement…</div>
            ) : collections.length === 0 ? (
              <div className="ingestion-empty">Aucune collection Qdrant. Créez-la d'abord.</div>
            ) : (
              <select
                className="im-col-select"
                value={colName}
                onChange={e => setColName(e.target.value)}
              >
                {collections.map(c => (
                  <option key={c.id} value={c.name}>
                    {c.name}{c.is_owner ? '' : ' (lecture seule ?)'}
                  </option>
                ))}
              </select>
            )}
          </div>

          {/* ── Upload / staging ── */}
          <div className="ingestion-section">
            <label className="field-label">Ajouter des documents (PDF, DOCX, TXT…) ou archives (.zip / .7z)</label>
            <div
              className="im-picker"
              onClick={() => fileInputRef.current?.click()}
              onDragOver={e => { if (Array.from(e.dataTransfer.types).includes('Files')) { e.preventDefault(); } }}
              onDrop={async e => {
                e.preventDefault();
                const files = Array.from(e.dataTransfer.files || []);
                if (files.length) await handleFiles(files);
              }}
            >
              {fileBusy ? (
                <><span className="spinner-sm" /> Décompression en cours…</>
              ) : (
                <>📁 Cliquez ou glissez-déposez — les .zip/.7z sont décompressés en documents</>
              )}
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept={UPLOAD_ACCEPT}
              multiple
              style={{ display: 'none' }}
              onChange={e => {
                const files = Array.from(e.target.files || []);
                if (fileInputRef.current) fileInputRef.current.value = '';
                if (files.length) handleFiles(files);
              }}
            />

            {staging && (
              <div className="im-staging">
                <div className="im-staging-head">
                  <strong>En attente d'ingestion ({staging.files.length})</strong>
                  <div>
                    <button className="im-mini-btn" onClick={cancelBatch} disabled={busy} title="Annuler ce lot">🗑 Annuler</button>
                    <button className="im-mini-btn im-mini-btn--primary" onClick={ingestBatch} disabled={busy || !staging.files.length}>
                      {busy ? 'Ingestion…' : '✓ Ingérer dans la collection'}
                    </button>
                  </div>
                </div>
                <div className="im-staging-list">
                  {staging.files.map(f => (
                    <div key={f.id} className="im-staged-row">
                      <span className="im-staged-name" title={f.name}>{f.name}</span>
                      <span className="im-staged-meta">.{f.ext} · {pretty(f.size)}</span>
                      <button className="im-staged-del" onClick={() => removeStaged(f)} title="Retirer de l'envoi">✕</button>
                    </div>
                  ))}
                </div>
                {staging.errors.length > 0 && (
                  <div className="im-staging-errors">
                    {staging.errors.map((e, i) => <div key={i}>⚠ {e}</div>)}
                  </div>
                )}
              </div>
            )}
          </div>

          {/* ── Documents déjà ingérés (suppression durable) ── */}
          <div className="ingestion-section">
            <div className="ingestion-collections-header">
              <label className="field-label" style={{ margin: 0 }}>
                Documents de la collection
                {docs.length > 0 ? ` (${docs.length})` : ''}
              </label>
              <button className="ingestion-refresh-btn" onClick={() => loadDocs(colName)} disabled={loadingDocs} title="Recharger">
                <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" width="14" height="14" style={{ animation: loadingDocs ? 'spin 1s linear infinite' : 'none' }}>
                  <path d="M16 3v4.5h-4.5" />
                  <path d="M2 9a7 7 0 0 1 11.95-4.95L16 7.5" />
                  <path d="M2 15v-4.5h4.5" />
                  <path d="M16 9a7 7 0 0 1-11.95 4.95L2 10.5" />
                </svg>
              </button>
            </div>
            {loadingDocs ? (
              <div className="ingestion-empty"><span className="spinner-sm" /> Chargement…</div>
            ) : docs.length === 0 ? (
              <div className="ingestion-empty">Aucun document indexé dans cette collection.</div>
            ) : (
              <div className="ingestion-col-list">
                {docs.map(d => (
                  <div key={d.id} className="ingestion-col-row">
                    <div className="ingestion-col-header">
                      <div className="ingestion-col-info">
                        <span className="im-doc-name" title={d.name}>{d.name || d.filename}</span>
                        {typeof d.chunks_count === 'number' && (
                          <span className="im-doc-meta">{d.chunks_count} chunk(s)</span>
                        )}
                      </div>
                      <div className="ingestion-col-actions">
                        {confirmDelId === d.id ? (
                          <button className="im-mini-btn im-mini-btn--danger" onClick={() => deleteDoc(d)}>
                            Confirmer ?
                          </button>
                        ) : (
                          <button
                            className="icon-btn"
                            onClick={() => setConfirmDelId(d.id)}
                            title="Supprimer ce document de la collection"
                          >🗑</button>
                        )}
                        {confirmDelId === d.id && (
                          <button className="icon-btn" onClick={() => setConfirmDelId(null)} title="Annuler">✕</button>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="modal-footer">
          <button className="btn-ghost" onClick={onClose}>Fermer</button>
        </div>
      </div>
    </div>
  );
}
