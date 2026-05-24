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
  score?: number;
  rerank_score?: number;
}

// Groupe les chunks par fichier source pour l'affichage, tout en conservant
// tous les index cités par le LLM (pas de déduplication).
interface SourceGroup {
  filename: string;
  chunks: RagSource[];
}

function groupBySource(sources: RagSource[]): SourceGroup[] {
  const map = new Map<string, RagSource[]>();
  for (const s of sources) {
    const key = s.source ? s.source.split('/').pop()! : '(source inconnue)';
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(s);
  }
  return Array.from(map.entries()).map(([filename, chunks]) => ({ filename, chunks }));
}

export function RagSourcesBadge({ sources }: { sources: RagSource[] }) {
  const [open, setOpen] = useState(false);
  if (!sources || sources.length === 0) return null;

  const groups = groupBySource(sources);
  const total  = sources.length;

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
        {total} extrait{total > 1 ? 's' : ''} · {groups.length} fichier{groups.length > 1 ? 's' : ''}
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
            {groups.map((g, gi) => (
              <li key={gi} className="rag-badge-item rag-badge-item--group">
                <span className="rag-badge-source">{g.filename}</span>
                <span className="rag-badge-indexes">
                  {g.chunks.map((c, ci) => {
                    const parts: string[] = [];
                    if (c.page)         parts.push(`Page ${c.page}`);
                    if (c.rerank_score != null)
                      parts.push(`Pertinence (rerank) : ${(c.rerank_score * 100).toFixed(1)} %`);
                    if (c.score != null)
                      parts.push(`Score vectoriel : ${(c.score * 100).toFixed(1)} %`);
                    const tip = parts.join(' · ');
                    return (
                      <span key={ci} className="rag-badge-idx" title={tip || undefined}>
                        [{c.index}]{c.page ? <span className="rag-badge-page"> p.{c.page}</span> : null}
                      </span>
                    );
                  })}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
