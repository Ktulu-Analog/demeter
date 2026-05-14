import React, { useState, useEffect, useRef, useCallback } from 'react';
import { API_BASE, ACCEPTED_DOC_EXTS } from '../constants';
import { useDialog } from '../DialogContext';

// ── Types ─────────────────────────────────────────────────────────────────────

interface Collection {
  id: number;
  name: string;
  description?: string;
  visibility: 'private' | 'public';
}

interface Document {
  id: number;
  name?: string;
  filename?: string;
  chunks_count?: number;
}

interface AppSettings {
  endpoint: string;
  bearer: string;
}

interface Space {
  id: string;
  label?: string;
  icon?: string;
}

// ── CopyButton & CodeSnippet ──────────────────────────────────────────────────

function CopyButton({ text }: { text: string }) {
  const [copied, setCopied] = useState(false);
  const handle = async () => {
    try { await navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 2000); } catch { /* ignore */ }
  };
  return (
    <button className="igcode-copy" onClick={handle} title="Copier">
      {copied
        ? <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z"/></svg>
        : <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M4 2a2 2 0 012-2h6a2 2 0 012 2v10a2 2 0 01-2 2H6a2 2 0 01-2-2V2zm2-1a1 1 0 00-1 1v10a1 1 0 001 1h6a1 1 0 001-1V2a1 1 0 00-1-1H6z"/><path d="M2 5a1 1 0 00-1 1v8a1 1 0 001 1h7v-1H2V6H1V5h1z"/></svg>
      }
    </button>
  );
}

function CodeSnippet({ code }: { code: string }) {
  return (
    <div className="igcode-wrap">
      <pre className="igcode-pre"><code>{code}</code></pre>
      <CopyButton text={code} />
    </div>
  );
}

// ── IngestionGuide ────────────────────────────────────────────────────────────

interface IngestionGuideProps {
  collection: Collection;
  endpoint: string;
  bearer: string;
  chunkSize: number;
  chunkOverlap: number;
}

export function IngestionGuide({ collection, endpoint, bearer, chunkSize, chunkOverlap }: IngestionGuideProps) {
  const [open, setOpen]           = useState(false);
  const [activeTab, setActiveTab] = useState('fichier');

  const base  = endpoint.replace(/\/(chat\/completions|rerank|embeddings|search)$/, '').replace(/\/+$/, '');
  const colId = collection.id;

  const curlFichier = `curl -X POST ${base}/v1/documents \\
  -H "Authorization: Bearer ${bearer}" \\
  -F "file=@mon_document.pdf" \\
  -F "collection_id=${colId}" \\
  -F "chunk_size=${chunkSize}" \\
  -F "chunk_overlap=${chunkOverlap}" \\
  -F "preset_separators=markdown"`;

  const bashRepertoire = `#!/bin/bash
ENDPOINT="${base}"
TOKEN="${bearer}"
COLLECTION_ID=${colId}
CHUNK_SIZE=${chunkSize}
CHUNK_OVERLAP=${chunkOverlap}
DOSSIER="./mes_documents"   # ← chemin à adapter

find "$DOSSIER" -type f \\( -name "*.pdf" -o -name "*.docx" \\
  -o -name "*.txt" -o -name "*.md" \\) | while read -r fichier; do
  echo "→ $fichier"
  curl -s -X POST "$ENDPOINT/v1/documents" \\
    -H "Authorization: Bearer $TOKEN" \\
    -F "file=@$fichier" \\
    -F "collection_id=$COLLECTION_ID" \\
    -F "chunk_size=$CHUNK_SIZE" \\
    -F "chunk_overlap=$CHUNK_OVERLAP" \\
    -F "preset_separators=markdown" | python3 -m json.tool
  sleep 0.5
done
echo "✓ Terminé"`;

  const pyFichier = `import httpx, pathlib

BASE     = "${base}"
TOKEN    = "${bearer}"
COL_ID   = ${colId}

def upload(path: str):
    with open(path, "rb") as f:
        r = httpx.post(
            f"{base}/v1/documents",
            headers={"Authorization": f"Bearer {TOKEN}"},
            files={"file": (pathlib.Path(path).name, f)},
            data={
                "collection_id": str(COL_ID),
                "chunk_size":    "${chunkSize}",
                "chunk_overlap": "${chunkOverlap}",
                "preset_separators": "markdown",
            },
            timeout=120,
        )
    r.raise_for_status()
    return r.json()

print(upload("mon_document.pdf"))`;

  const tabs = [
    { id: 'fichier',    label: '📄 Fichier (curl)',    code: curlFichier },
    { id: 'repertoire', label: '📁 Répertoire (bash)', code: bashRepertoire },
    { id: 'python',     label: '🐍 Python',            code: pyFichier },
  ];

  return (
    <div className="ingestion-section">
      <button className="igguide-toggle" onClick={() => setOpen(o => !o)}>
        <span className="igguide-toggle-icon">{open ? '▾' : '▸'}</span>
        <span>Ingestion par ligne de commande</span>
        <span className="igguide-toggle-hint">curl · bash · Python</span>
        <span style={{ marginLeft: 'auto', fontSize: 10, color: 'var(--text-tertiary)' }}>
          collection #{colId} · {chunkSize} tok
        </span>
      </button>
      {open && (
        <div className="igguide-body">
          <div className="igguide-info">
            Les valeurs de <strong>chunk_size</strong> ({chunkSize}) et <strong>chunk_overlap</strong> ({chunkOverlap}) correspondent à votre réglage actuel.
          </div>
          <div className="igguide-tabs">
            {tabs.map(t => (
              <button key={t.id} className={`igguide-tab ${activeTab === t.id ? 'active' : ''}`} onClick={() => setActiveTab(t.id)}>{t.label}</button>
            ))}
          </div>
          {tabs.filter(t => t.id === activeTab).map(t => <CodeSnippet key={t.id} code={t.code} />)}
          <div className="igguide-tips">
            <div className="igguide-tip"><span className="igguide-tip-icon">💡</span><span>Pour un répertoire volumineux, ajoutez un <code>sleep</code> entre les appels pour éviter le rate-limiting.</span></div>
            <div className="igguide-tip"><span className="igguide-tip-icon">📖</span><span>Albert chunke automatiquement avec <code>preset_separators=markdown</code> — idéal pour PDF et Word.</span></div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── IngestionModal ────────────────────────────────────────────────────────────

interface IngestionModalProps {
  settings: AppSettings;
  spaces: Space[];
  onClose: () => void;
}

export function IngestionModal({ settings, spaces, onClose }: IngestionModalProps) {
  const { toast: toastFn, confirm: confirmFn } = useDialog();
  const [selectedSpaceId, setSelectedSpaceId]                 = useState(spaces[0]?.id || '');
  const [newCollectionVisibility, setNewCollectionVisibility] = useState<'private' | 'public'>('private');
  const [collections, setCollections]                         = useState<Collection[]>([]);
  const [loadingCollections, setLoadingCollections]           = useState(false);
  const [uploading, setUploading]                             = useState(false);
  const [creating, setCreating]                               = useState(false);
  const [deleting, setDeleting]                               = useState<number | null>(null);
  const [deletingDoc, setDeletingDoc]                         = useState<number | null>(null);
  const [expandedCollection, setExpandedCollection]           = useState<number | null>(null);
  const [documents, setDocuments]                             = useState<Record<number, Document[]>>({});
  const [loadingDocs, setLoadingDocs]                         = useState<number | null>(null);
  const [chunkSize, setChunkSize]                             = useState(1024);
  const [chunkOverlap, setChunkOverlap]                       = useState(100);
  const [isDragging, setIsDragging]                           = useState(false);
  const [pendingFiles, setPendingFiles]                       = useState<File[]>([]);
  const [uploadProgress, setUploadProgress]                   = useState<{ current: number; total: number; currentName: string; ok: number; errors: number } | null>(null);
  const fileInputRef   = useRef<HTMLInputElement>(null);
  const folderInputRef = useRef<HTMLInputElement>(null);
  const dropRef        = useRef<HTMLDivElement>(null);

  const showToast = useCallback((msg: string, type: 'success' | 'error' = 'success') => toastFn(msg, type), [toastFn]);

  const [spaceAssignments, setSpaceAssignments] = useState<Record<string, number | null>>(() => {
    try { return JSON.parse(localStorage.getItem('demeter_space_assignments') || '{}'); } catch { return {}; }
  });

  const saveAssignments = (next: Record<string, number | null>) => {
    setSpaceAssignments(next);
    localStorage.setItem('demeter_space_assignments', JSON.stringify(next));
  };

  const loadCollections = useCallback(async () => {
    if (!settings.endpoint || !settings.bearer) return;
    setLoadingCollections(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections?endpoint=${encodeURIComponent(settings.endpoint)}&bearer=${encodeURIComponent(settings.bearer)}`);
      if (!res.ok) throw new Error(await res.text());
      const data: { data?: Collection[] } = await res.json();
      const sorted = (data.data || []).sort((a, b) => (a.visibility === 'private' ? 0 : 1) - (b.visibility === 'private' ? 0 : 1));
      setCollections(sorted);
      const nameCache: Record<number, string> = {};
      sorted.forEach(c => { nameCache[c.id] = c.name; });
      localStorage.setItem('demeter_collection_names', JSON.stringify(nameCache));
    } catch (e) {
      showToast(`Erreur chargement collections : ${(e as Error).message}`, 'error');
    } finally {
      setLoadingCollections(false);
    }
  }, [settings, showToast]);

  useEffect(() => { loadCollections(); }, [loadCollections]);

  const selectedSpace      = spaces.find(s => s.id === selectedSpaceId);
  const assignedColId      = spaceAssignments[selectedSpaceId];
  const matchingCollection = assignedColId != null
    ? collections.find(c => c.id === assignedColId)
    : assignedColId === null
      ? undefined
      : collections.find(c => c.name === selectedSpaceId);

  const handleCreateCollection = async () => {
    if (!selectedSpaceId) return;
    setCreating(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections`, {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: selectedSpaceId, description: `Base documentaire — ${selectedSpace?.label || selectedSpaceId}`, visibility: newCollectionVisibility, endpoint: settings.endpoint, bearer: settings.bearer }),
      });
      if (!res.ok) { const e = await res.json().catch(() => ({ detail: res.statusText })); throw new Error(e.detail || res.statusText); }
      showToast(`Collection "${selectedSpaceId}" créée avec succès !`);
      await loadCollections();
    } catch (e) {
      showToast(`Erreur : ${(e as Error).message}`, 'error');
    } finally {
      setCreating(false);
    }
  };

  const handleDeleteCollection = async (colId: number, colName: string) => {
    const ok = await confirmFn(`Supprimer la collection "${colName}" et tous ses documents ?`, { type: 'danger', confirmLabel: 'Supprimer' });
    if (!ok) return;
    setDeleting(colId);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections/${colId}?endpoint=${encodeURIComponent(settings.endpoint)}&bearer=${encodeURIComponent(settings.bearer)}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(await res.text());
      showToast(`Collection "${colName}" supprimée.`);
      setDocuments(d => { const nd = { ...d }; delete nd[colId]; return nd; });
      setExpandedCollection(p => p === colId ? null : p);
      await loadCollections();
    } catch (e) {
      showToast(`Erreur : ${(e as Error).message}`, 'error');
    } finally {
      setDeleting(null);
    }
  };

  const handleRenameCollection = async (colId: number, currentName: string) => {
    const newName = prompt(`Renommer la collection "${currentName}" :`, currentName);
    if (!newName || newName.trim() === currentName) return;
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections/${colId}`, {
        method: 'PATCH', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ name: newName.trim(), endpoint: settings.endpoint, bearer: settings.bearer }),
      });
      if (!res.ok) { const err = await res.json().catch(() => ({ detail: res.statusText })); throw new Error(err.detail || res.statusText); }
      showToast(`Collection renommée en "${newName.trim()}".`);
      await loadCollections();
    } catch (e) {
      showToast(`Erreur : ${(e as Error).message}`, 'error');
    }
  };

  const loadDocuments = async (colId: number) => {
    if (expandedCollection === colId) { setExpandedCollection(null); return; }
    setExpandedCollection(colId); setLoadingDocs(colId);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections/${colId}/documents?endpoint=${encodeURIComponent(settings.endpoint)}&bearer=${encodeURIComponent(settings.bearer)}`);
      if (!res.ok) throw new Error(await res.text());
      const data: { data?: Document[] } = await res.json();
      setDocuments(d => ({ ...d, [colId]: data.data || [] }));
    } catch (e) {
      showToast(`Erreur chargement documents : ${(e as Error).message}`, 'error');
    } finally {
      setLoadingDocs(null);
    }
  };

  const handleDeleteDocument = async (docId: number, docName: string, colId: number) => {
    const ok = await confirmFn(`Supprimer le document "${docName}" ?`, { type: 'danger', confirmLabel: 'Supprimer' });
    if (!ok) return;
    setDeletingDoc(docId);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/documents/${docId}?endpoint=${encodeURIComponent(settings.endpoint)}&bearer=${encodeURIComponent(settings.bearer)}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(await res.text());
      showToast('Document supprimé.');
      setDocuments(d => ({ ...d, [colId]: (d[colId] || []).filter(doc => doc.id !== docId) }));
    } catch (e) {
      showToast(`Erreur : ${(e as Error).message}`, 'error');
    } finally {
      setDeletingDoc(null);
    }
  };

  const uploadFiles = async (files: File[]) => {
    if (!matchingCollection) { showToast("Créez d'abord la collection pour cet espace.", 'error'); return; }
    setUploading(true);
    const total = files.length; let ok = 0; const errors: string[] = [];
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      setUploadProgress({ current: i + 1, total, currentName: file.name, ok, errors: errors.length });
      const fd = new FormData();
      fd.append('file', file); fd.append('collection_id', String(matchingCollection.id));
      fd.append('endpoint', settings.endpoint); fd.append('bearer', settings.bearer);
      fd.append('chunk_size', String(chunkSize)); fd.append('chunk_overlap', String(chunkOverlap));
      try {
        const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/upload`, { method: 'POST', body: fd });
        if (!res.ok) { const e = await res.json().catch(() => ({ detail: res.statusText })); errors.push(`${file.name} : ${e.detail || res.statusText}`); }
        else ok++;
      } catch (e) { errors.push(`${file.name} : ${(e as Error).message}`); }
    }
    setUploadProgress(null); setPendingFiles([]);
    if (ok > 0) showToast(`${ok} fichier(s) indexé(s) avec succès !`);
    if (errors.length) showToast(errors.join('\n'), 'error');
    if (expandedCollection === matchingCollection.id) await loadDocuments(matchingCollection.id);
    setUploading(false);
  };

  const isAccepted = (f: File) => ACCEPTED_DOC_EXTS.has('.' + f.name.split('.').pop()!.toLowerCase());
  const addFiles = (incoming: FileList | null) => {
    if (!incoming) return;
    const valid = Array.from(incoming).filter(isAccepted);
    if (!valid.length) return;
    setPendingFiles(prev => { const names = new Set(prev.map(p => p.name)); return [...prev, ...valid.filter(f => !names.has(f.name))]; });
  };
  const handleFileChange   = (e: React.ChangeEvent<HTMLInputElement>) => { addFiles(e.target.files); if (fileInputRef.current) fileInputRef.current.value = ''; };
  const handleFolderChange = (e: React.ChangeEvent<HTMLInputElement>) => { addFiles(e.target.files); if (folderInputRef.current) folderInputRef.current.value = ''; };
  const handleDrop         = (e: React.DragEvent) => { e.preventDefault(); setIsDragging(false); addFiles(e.dataTransfer.files); };

  return (
    <div className="modal-overlay">
      <div className="modal ingestion-modal">
        <div className="modal-header">
          <div className="modal-header-left"><span className="modal-icon">📚</span><span className="modal-title">Ingestion documentaire — RAG</span></div>
          <button className="icon-btn" onClick={onClose}>✕</button>
        </div>

        <div className="modal-body ingestion-body">
          {/* Sélecteur d'espace */}
          <div className="ingestion-section">
            <label className="field-label">Espace cible</label>
            <div className="ingestion-space-tabs">
              {spaces.map(s => (
                <button key={s.id} className={`ingestion-space-tab ${selectedSpaceId === s.id ? 'active' : ''}`} onClick={() => setSelectedSpaceId(s.id)}>
                  <span>{s.icon}</span> {s.label}
                </button>
              ))}
            </div>
          </div>

          {/* État de la collection */}
          <div className="ingestion-section">
            <div className="ingestion-collection-status">
              {loadingCollections ? (
                <div className="ingestion-status-row"><span className="spinner-sm" /> Chargement…</div>
              ) : matchingCollection ? (
                <div className="ingestion-status-row ingestion-status-row--ok">
                  <span className="ingestion-status-dot ingestion-status-dot--green" />
                  <span>Collection <strong>"{matchingCollection.name}"</strong> prête</span>
                  <span className="ingestion-status-id">id={matchingCollection.id}</span>
                </div>
              ) : (
                <div className="ingestion-status-row ingestion-status-row--warn">
                  <span className="ingestion-status-dot ingestion-status-dot--amber" />
                  <span>Aucune collection pour l'espace <strong>"{selectedSpaceId}"</strong></span>
                  <div className="ingestion-visibility-toggle">
                    <button className={`ingestion-vis-btn ${newCollectionVisibility === 'private' ? 'active' : ''}`} onClick={() => setNewCollectionVisibility('private')} title="Visible uniquement par vous">🔒 Privée</button>
                    <button className={`ingestion-vis-btn ${newCollectionVisibility === 'public' ? 'active' : ''}`} onClick={() => setNewCollectionVisibility('public')} title="Visible par tous">🌐 Publique</button>
                  </div>
                  <button className="btn-primary ingestion-create-btn" onClick={handleCreateCollection} disabled={creating}>
                    {creating ? <span className="spinner-sm" /> : '＋'} Créer
                  </button>
                </div>
              )}
            </div>
          </div>

          {/* Zone upload */}
          {matchingCollection && matchingCollection.visibility === 'private' && (
            <div className="ingestion-section">
              <label className="field-label">Ajouter des documents</label>
              <div className="ingestion-source-btns">
                <button className="ingestion-source-btn" onClick={() => !uploading && fileInputRef.current?.click()} disabled={uploading}>
                  <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path fillRule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4zm2 6a1 1 0 011-1h6a1 1 0 110 2H7a1 1 0 01-1-1zm1 3a1 1 0 100 2h6a1 1 0 100-2H7z" clipRule="evenodd"/></svg>
                  Fichiers
                </button>
                <button className="ingestion-source-btn" onClick={() => !uploading && folderInputRef.current?.click()} disabled={uploading}>
                  <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/></svg>
                  Dossier entier
                </button>
              </div>
              <input ref={fileInputRef} type="file" accept=".pdf,.docx,.doc,.txt,.md" multiple style={{ display: 'none' }} onChange={handleFileChange} />
              <input ref={folderInputRef} type="file" multiple style={{ display: 'none' }} onChange={handleFolderChange} />

              <div
                ref={dropRef}
                className={`ingestion-dropzone ${isDragging ? 'ingestion-dropzone--active' : ''}`}
                onDragOver={e => { e.preventDefault(); setIsDragging(true); }}
                onDragLeave={() => setIsDragging(false)}
                onDrop={handleDrop}
              >
                <div className="ingestion-drop-icon">{isDragging ? '⬇' : '📂'}</div>
                <div className="ingestion-drop-label">{isDragging ? 'Déposez ici' : 'ou glissez-déposez vos fichiers'}</div>
                <div className="ingestion-drop-hint">PDF · Word · TXT · Markdown</div>
              </div>

              <div className="ingestion-chunk-params">
                <div className="ingestion-chunk-field">
                  <label className="field-label">Taille des chunks (tokens)</label>
                  <input className="field-input" type="number" value={chunkSize} min={256} max={4096} step={128} onChange={e => setChunkSize(Number(e.target.value))} disabled={uploading} />
                </div>
                <div className="ingestion-chunk-field">
                  <label className="field-label">Chevauchement (tokens)</label>
                  <input className="field-input" type="number" value={chunkOverlap} min={0} max={512} step={16} onChange={e => setChunkOverlap(Number(e.target.value))} disabled={uploading} />
                </div>
              </div>

              {pendingFiles.length > 0 && !uploading && (
                <div className="ingestion-pending">
                  <div className="ingestion-pending-header">
                    <span>{pendingFiles.length} fichier{pendingFiles.length > 1 ? 's' : ''} en attente</span>
                    <button className="ingestion-pending-clear" onClick={() => setPendingFiles([])}>Tout retirer</button>
                  </div>
                  <div className="ingestion-pending-list">
                    {pendingFiles.map((f, i) => (
                      <div key={i} className="ingestion-pending-file">
                        <span className="ingestion-file-icon">{f.name.endsWith('.pdf') ? '📄' : f.name.match(/\.docx?$/) ? '📝' : '📃'}</span>
                        <span className="ingestion-file-name">{f.name}</span>
                        <span className="ingestion-file-size">{f.size < 1024 * 1024 ? (f.size / 1024).toFixed(0) + ' Ko' : (f.size / 1024 / 1024).toFixed(1) + ' Mo'}</span>
                        <button className="ingestion-file-remove" onClick={() => setPendingFiles(p => p.filter((_, j) => j !== i))}>✕</button>
                      </div>
                    ))}
                  </div>
                  <button className="btn-primary ingestion-upload-btn" onClick={() => uploadFiles(pendingFiles)}>
                    ⬆ Indexer {pendingFiles.length} fichier{pendingFiles.length > 1 ? 's' : ''}
                  </button>
                </div>
              )}

              {uploading && uploadProgress && (
                <div className="ingestion-progress-wrap">
                  <div className="ingestion-progress-header">
                    <span className="ingestion-progress-label"><span className="spinner-sm" /> {uploadProgress.current}/{uploadProgress.total} — <em>{uploadProgress.currentName}</em></span>
                    <span className="ingestion-progress-stats">
                      {uploadProgress.ok > 0 && <span className="ingestion-prog-ok">✓ {uploadProgress.ok}</span>}
                      {uploadProgress.errors > 0 && <span className="ingestion-prog-err">✗ {uploadProgress.errors}</span>}
                    </span>
                  </div>
                  <div className="ingestion-progress-bar-track">
                    <div className="ingestion-progress-bar-fill" style={{ width: `${Math.round((uploadProgress.current - 1) / uploadProgress.total * 100)}%` }} />
                  </div>
                </div>
              )}
            </div>
          )}

          {matchingCollection && matchingCollection.visibility === 'public' && (
            <div className="ingestion-section">
              <div className="field-hint"><span className="hint-icon">🔒</span><span>La collection <strong>"{matchingCollection.name}"</strong> est publique — elle est en lecture seule.</span></div>
            </div>
          )}

          {matchingCollection && matchingCollection.visibility === 'private' && (
            <IngestionGuide collection={matchingCollection} endpoint={settings.endpoint} bearer={settings.bearer} chunkSize={chunkSize} chunkOverlap={chunkOverlap} />
          )}

          {/* Liste des collections */}
          <div className="ingestion-section">
            <div className="ingestion-collections-header">
              <label className="field-label" style={{ margin: 0 }}>Collections Albert</label>
              <button className="ingestion-refresh-btn" onClick={loadCollections} disabled={loadingCollections} title="Rafraîchir">
                <svg viewBox="0 0 16 16" fill="currentColor" width="13" height="13" style={{ animation: loadingCollections ? 'spin 1s linear infinite' : 'none' }}>
                  <path d="M13.65 2.35A8 8 0 1014 8h-2a6 6 0 10-.54 2.36l1.45 1.45A8 8 0 0113.65 2.35z"/>
                </svg>
              </button>
            </div>

            {loadingCollections ? (
              <div className="ingestion-empty"><span className="spinner-sm" /> Chargement…</div>
            ) : collections.length === 0 ? (
              <div className="ingestion-empty">Aucune collection. Créez-en une via le sélecteur ci-dessus.</div>
            ) : (
              <div className="ingestion-col-list">
                {collections.map(col => {
                  const isMatching = col.name === selectedSpaceId;
                  const isExpanded = expandedCollection === col.id;
                  const docs       = documents[col.id] || [];
                  const isAssigned = matchingCollection?.id === col.id;
                  return (
                    <div key={col.id} className={`ingestion-col-row ${isMatching ? 'ingestion-col-row--active' : ''}`}>
                      <div className="ingestion-col-header">
                        <div className="ingestion-col-info" onClick={() => loadDocuments(col.id)}>
                          <span className="ingestion-col-chevron">{isExpanded ? '▾' : '▸'}</span>
                          <span className="ingestion-col-name">{col.name}</span>
                          {isMatching && <span className="ingestion-col-badge">espace actif</span>}
                        </div>
                        <div className="ingestion-col-actions">
                          <button
                            className={`ingestion-col-action ingestion-col-assign${isAssigned ? ' ingestion-col-assign--active' : ''}`}
                            onClick={e => { e.stopPropagation(); saveAssignments({ ...spaceAssignments, [selectedSpaceId]: isAssigned ? null : col.id }); }}
                            title={isAssigned ? `Désassigner de l'espace "${selectedSpaceId}"` : `Assigner à l'espace "${selectedSpaceId}"`}
                          >{isAssigned ? '📌' : '📎'}</button>
                          {col.visibility === 'private' && (<>
                            <button className="ingestion-col-action ingestion-col-rename" onClick={e => { e.stopPropagation(); handleRenameCollection(col.id, col.name); }} disabled={deleting === col.id} title="Renommer">✏️</button>
                            <button className="ingestion-col-action ingestion-col-delete" onClick={e => { e.stopPropagation(); handleDeleteCollection(col.id, col.name); }} disabled={deleting === col.id} title="Supprimer">{deleting === col.id ? <span className="spinner-sm" /> : '🗑️'}</button>
                          </>)}
                        </div>
                        <span className="ingestion-col-id">#{col.id}</span>
                        <span className={`ingestion-vis-badge ingestion-vis-badge--${col.visibility === 'public' ? 'public' : 'private'}`}>{col.visibility === 'public' ? '🌐 publique' : '🔒 privée'}</span>
                      </div>

                      {isExpanded && (
                        <div className="ingestion-doc-list">
                          {loadingDocs === col.id ? (
                            <div className="ingestion-doc-loading"><span className="spinner-sm" /> Chargement…</div>
                          ) : docs.length === 0 ? (
                            <div className="ingestion-doc-empty">Aucun document dans cette collection.</div>
                          ) : docs.map(doc => {
                            const rawName   = doc.name || doc.filename || `Document #${doc.id}`;
                            const shortName = rawName.split('/').pop()!.split('\\').pop()!;
                            return (
                              <div key={doc.id} className="ingestion-doc-row">
                                <span className="ingestion-doc-icon">📄</span>
                                <span className="ingestion-doc-name" title={rawName}>{shortName}</span>
                                {doc.chunks_count != null && <span className="ingestion-doc-chunks">{doc.chunks_count} chunks</span>}
                                <button className="ingestion-doc-delete" onClick={() => handleDeleteDocument(doc.id, shortName, col.id)} disabled={deletingDoc === doc.id} title="Supprimer le document">
                                  {deletingDoc === doc.id ? <span className="spinner-sm" /> : '✕'}
                                </button>
                              </div>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          <div className="field-hint">
            <span className="hint-icon">💡</span>
            <span>Les collections sont liées à un espace par leur nom. Le RAG se déclenche automatiquement à chaque question dans l'espace correspondant.</span>
          </div>
        </div>
      </div>
    </div>
  );
}
