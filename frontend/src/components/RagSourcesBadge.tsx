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



import React, { useState } from 'react';

export interface RagSource {
  index: number;
  source?: string;
  page?: number | string;
  rerank_score?: number;
}

export function RagSourcesBadge({ sources }: { sources: RagSource[] }) {
  const [open, setOpen] = useState(false);
  if (!sources || sources.length === 0) return null;

  const unique = sources.reduce<RagSource[]>((acc, s) => {
    const key = s.source || '(inconnu)';
    if (!acc.find(x => x.source === key)) acc.push(s);
    return acc;
  }, []);

  return (
    <div className="rag-badge-wrapper">
      <button
        className={`rag-badge-btn ${open ? 'rag-badge-btn--open' : ''}`}
        onClick={() => setOpen(o => !o)}
        title="Voir les documents utilisés pour cette réponse"
      >
        <svg viewBox="0 0 16 16" width="11" height="11" fill="currentColor">
          <path d="M2 2a1 1 0 011-1h6.5l4.5 4.5V14a1 1 0 01-1 1H3a1 1 0 01-1-1V2zm8 0v3.5H13L10 2zM4 7h8v1H4V7zm0 2h8v1H4V9zm0 2h5v1H4v-1z"/>
        </svg>
        {unique.length} source{unique.length > 1 ? 's' : ''} RAG
        <svg
          viewBox="0 0 16 16" width="9" height="9" fill="currentColor"
          style={{ transform: open ? 'rotate(180deg)' : 'rotate(0deg)', transition: 'transform .2s' }}
        >
          <path d="M3 5l5 5 5-5z"/>
        </svg>
      </button>
      {open && (
        <div className="rag-badge-panel">
          <div className="rag-badge-title">Documents consultés</div>
          <ul className="rag-badge-list">
            {sources.map((s, i) => (
              <li key={i} className="rag-badge-item">
                <span className="rag-badge-idx">[{s.index}]</span>
                <span className="rag-badge-source">
                  {s.source ? s.source.split('/').pop() : '(source inconnue)'}
                  {s.page ? <span className="rag-badge-page">, p.{s.page}</span> : null}
                </span>
                {s.rerank_score != null && (
                  <span className="rag-badge-score" title="Score de pertinence">
                    {(s.rerank_score * 100).toFixed(0)}%
                  </span>
                )}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
