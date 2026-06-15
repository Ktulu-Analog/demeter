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

// ── Backend Qdrant — les collections sont identifiées par leur nom (string)  ──

import React, { useState, useEffect, useCallback } from 'react';
import { API_BASE } from '../constants';
import { useDialog } from '../DialogContext';

// ── Types ─────────────────────────────────────────────────────────────────────

interface Collection {
  id: string;
  name: string;
  visibility: 'private' | 'public' | 'shared';
  owner: string;
  is_owner: boolean;
  shared_with: string[];
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

// ── IngestionModal ────────────────────────────────────────────────────────────

interface IngestionModalProps {
  settings: AppSettings;
  spaces: Space[];
  onClose: () => void;
  onSave: (assignments: Record<string, string>) => void;
}

export function IngestionModal({ settings, spaces, onClose, onSave }: IngestionModalProps) {
  const { toast: toastFn } = useDialog();
  const [selectedSpaceId, setSelectedSpaceId]       = useState(spaces[0]?.id || '');
  const [collections, setCollections]               = useState<Collection[]>([]);
  const [loadingCollections, setLoadingCollections] = useState(false);
  const [qdrantStatus, setQdrantStatus]             = useState<'checking' | 'ok' | 'error'>('checking');

  // Ping Qdrant au montage et toutes les 30s
  useEffect(() => {
    const check = async () => {
      try {
        const res = await fetch(`${API_BASE}/api-proxy/api/rag/status`);
        const data = res.ok ? await res.json() : null;
        setQdrantStatus(data?.qdrant_ok === true ? 'ok' : 'error');
      } catch {
        setQdrantStatus('error');
      }
    };
    check();
    const id = setInterval(check, 30_000);
    return () => clearInterval(id);
  }, []);

  const showToast = useCallback((msg: string, type: 'success' | 'error' = 'success') => toastFn(msg, type), [toastFn]);

  // Draft local des assignments — lu depuis localStorage à l'ouverture,
  // jamais committé avant que l'utilisateur clique "Enregistrer".
  const [spaceAssignments, setSpaceAssignments] = useState<Record<string, string>>(() => {
    try {
      const raw: Record<string, string | null> = JSON.parse(localStorage.getItem('demeter_space_assignments_v2') || '{}');
      return Object.fromEntries(Object.entries(raw).filter(([, v]) => v !== null)) as Record<string, string>;
    } catch { return {}; }
  });

  // Modifie le draft local uniquement — pas de localStorage ici
  const saveAssignments = (next: Record<string, string | null>) => {
    const cleaned = Object.fromEntries(Object.entries(next).filter(([, v]) => v !== null)) as Record<string, string>;
    setSpaceAssignments(cleaned);
  };

  // Commit : persiste dans localStorage, notifie le parent, ne ferme PAS
  const handleSave = () => {
    localStorage.setItem('demeter_space_assignments_v2', JSON.stringify(spaceAssignments));
    onSave(spaceAssignments);
    showToast('Assignations enregistrées.');
  };

  const CACHE_TTL_MS = 180 * 60 * 1000;
  const cacheKey = `demeter_collections_qdrant_cache`;

  const [cacheAge, setCacheAge] = useState<number | null>(null);

  useEffect(() => {
    const tick = () => {
      try {
        const raw = localStorage.getItem(cacheKey);
        if (!raw) { setCacheAge(null); return; }
        const { ts } = JSON.parse(raw);
        setCacheAge(Math.floor((Date.now() - ts) / 1000));
      } catch { setCacheAge(null); }
    };
    tick();
    const id = setInterval(tick, 15_000);
    return () => clearInterval(id);
  }, [cacheKey]);

  const applyCollections = useCallback((sorted: Collection[]) => {
    setCollections(sorted);
    const nameCache: Record<string, string> = {};
    sorted.forEach(c => { nameCache[c.id] = c.name; });
    localStorage.setItem('demeter_collection_names', JSON.stringify(nameCache));
  }, []);

  const loadCollections = useCallback(async (force = false) => {
    if (!force) {
      try {
        const raw = localStorage.getItem(cacheKey);
        if (raw) {
          const { ts, data } = JSON.parse(raw);
          if (Date.now() - ts < CACHE_TTL_MS) {
            applyCollections(data);
            setCacheAge(Math.floor((Date.now() - ts) / 1000));
            return;
          }
        }
      } catch { /* cache corrompu */ }
    }

    setLoadingCollections(true);
    try {
      const res = await fetch(`${API_BASE}/api-proxy/api/ingestion/collections?endpoint=qdrant`);
      if (!res.ok) throw new Error(await res.text());
      const resp: { data?: Collection[] } = await res.json();
      const sorted = (resp.data || []).sort((a, b) => a.name.localeCompare(b.name));
      applyCollections(sorted);
      localStorage.setItem(cacheKey, JSON.stringify({ ts: Date.now(), data: sorted }));
      setCacheAge(0);
    } catch (e) {
      showToast(`Erreur chargement collections : ${(e as Error).message}`, 'error');
    } finally {
      setLoadingCollections(false);
    }
  }, [showToast, cacheKey, applyCollections]);

  useEffect(() => { loadCollections(); }, [loadCollections]);

  const assignedColName: string | null = spaceAssignments[selectedSpaceId] ?? null;
  const matchingCollection = assignedColName !== null
    ? collections.find(c => c.name === assignedColName) ?? null
    : null;

  return (
    <div className="modal-overlay">
      <div className="modal ingestion-modal">
        <div className="modal-header">
          <div className="modal-header-left">
            <span className="modal-icon">📚</span>
            <span className="modal-title">Collections RAG (Qdrant)</span>
            <span
              className={`qdrant-indicator qdrant-indicator--${qdrantStatus}`}
              title={qdrantStatus === 'checking' ? 'Vérification…' : qdrantStatus === 'ok' ? 'Qdrant connecté' : 'Qdrant inaccessible'}
            >
              <span className="qdrant-indicator__dot" />
              <span className="qdrant-indicator__label">
                {qdrantStatus === 'checking' ? 'Vérification…' : qdrantStatus === 'ok' ? 'Connecté' : 'Hors ligne'}
              </span>
            </span>
          </div>
          <button className="icon-btn" onClick={onClose} title="Fermer sans enregistrer">✕</button>
        </div>

        <div className="modal-body ingestion-body">

          {/* ── Sélecteur d'espace ── */}
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

          {/* ── État de l'assignation ── */}
          <div className="ingestion-section">
            <div className="ingestion-collection-status">
              {loadingCollections ? (
                <div className="ingestion-status-row"><span className="spinner-sm" /> Chargement…</div>
              ) : matchingCollection ? (
                <div className="ingestion-status-row ingestion-status-row--ok">
                  <span className="ingestion-status-dot ingestion-status-dot--green" />
                  <span>Collection <strong>"{matchingCollection.name}"</strong> assignée à cet espace</span>
                </div>
              ) : (
                <div className="ingestion-status-row ingestion-status-row--warn">
                  <span className="ingestion-status-dot ingestion-status-dot--amber" />
                  <span>Aucune collection assignée à <strong>"{selectedSpaceId}"</strong></span>
                </div>
              )}
            </div>
          </div>

          {/* ── Liste des collections ── */}
          <div className="ingestion-section">
            <div className="ingestion-collections-header">
              <label className="field-label" style={{ margin: 0 }}>Collections Qdrant</label>
              <div className="ingestion-cache-info">
                {cacheAge !== null && !loadingCollections && (
                  <span className="ingestion-cache-age" title="Données issues du cache local">
                    {cacheAge < 60
                      ? `il y a ${cacheAge}s`
                      : `il y a ${Math.floor(cacheAge / 60)}min`}
                  </span>
                )}
                <button className="ingestion-refresh-btn" onClick={() => loadCollections(true)} disabled={loadingCollections} title="Forcer le rechargement">
                  <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" width="14" height="14" style={{ animation: loadingCollections ? 'spin 1s linear infinite' : 'none' }}>
                    <path d="M16 3v4.5h-4.5"/>
                    <path d="M2 9a7 7 0 0 1 11.95-4.95L16 7.5"/>
                    <path d="M2 15v-4.5h4.5"/>
                    <path d="M16 9a7 7 0 0 1-11.95 4.95L2 10.5"/>
                  </svg>
                </button>
              </div>
            </div>

            {loadingCollections ? (
              <div className="ingestion-empty"><span className="spinner-sm" /> Chargement…</div>
            ) : collections.length === 0 ? (
              <div className="ingestion-empty">Aucune collection disponible.</div>
            ) : (
              <div className="ingestion-col-list">
                {collections.map(col => {
                  const isAssigned = matchingCollection?.name === col.name;
                  const visBadge   = col.visibility === 'public' ? { icon: '🌐', label: 'Publique',  cls: 'vis-public'  }
                                   : col.visibility === 'shared' ? { icon: '👥', label: 'Partagée', cls: 'vis-shared'  }
                                   :                               { icon: '🔒', label: 'Privée',   cls: 'vis-private' };
                  return (
                    <div key={col.name} className={`ingestion-col-row ${isAssigned ? 'ingestion-col-row--active' : ''}`}>
                      <div className="ingestion-col-header">
                        <div className="ingestion-col-info">
                          <span className="ingestion-col-name">{col.name}</span>
                          {isAssigned && <span className="ingestion-col-badge">espace actif</span>}
                          <span className={`ingestion-vis-badge ${visBadge.cls}`} title={visBadge.label}>{visBadge.icon}</span>
                          {col.owner && (
                            <span className="ingestion-col-owner" title="Propriétaire">
                              {col.is_owner ? '(vous)' : col.owner}
                            </span>
                          )}
                        </div>
                        <div className="ingestion-col-actions">
                          <button
                            className={`ingestion-col-action ingestion-col-assign${isAssigned ? ' ingestion-col-assign--active' : ''}`}
                            onClick={() => saveAssignments({ ...spaceAssignments, [selectedSpaceId]: isAssigned ? null : col.name })}
                            title={isAssigned ? `Désassigner de l'espace "${selectedSpaceId}"` : `Assigner à l'espace "${selectedSpaceId}"`}
                          >{isAssigned ? '📌' : '📎'}</button>
                        </div>
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          <div className="field-hint">
            <span className="hint-icon">💡</span>
            <span>Le RAG se déclenche automatiquement à chaque question dans l'espace auquel la collection est assignée.</span>
          </div>

        </div>

        <div className="modal-footer">
          <button className="btn-ghost" onClick={onClose}>
            Fermer
          </button>
          <button className="btn-primary" onClick={handleSave}>
            ✓ Enregistrer
          </button>
        </div>
      </div>
    </div>
  );
}
