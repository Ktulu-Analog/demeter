// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Auteur  : Pierre COUGET
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
//           https://www.gnu.org/licenses/agpl-3.0.html
// Année   : 2026
// ----------------------------------------------------------------------------
// Boîte de dialogue d'autorisation unique des serveurs MCP (constat F-01).
// ============================================================================

import { useState } from 'react';
import type { ConsentEntry } from '../utils/mcpConsent';

interface Props {
  entries: ConsentEntry[];
  /** Vrai si au moins un binaire local a changé depuis son autorisation (F-03). */
  modified?: string[];
  onConfirm: (keys: string[]) => void;
  onCancel: () => void;
}

export function McpConsentDialog({ entries, modified = [], onConfirm, onCancel }: Props) {
  const [selected, setSelected] = useState<Set<string>>(() => new Set(entries.map(e => e.key)));
  const toggle = (key: string) =>
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key); else next.add(key);
      return next;
    });
  const hasLocal = entries.some(e => e.kind === 'stdio');

  return (
    <div className="modal-overlay" style={{ zIndex: 1200 }}>
      <div className="modal mcp-consent-modal" role="dialog" aria-modal="true" aria-labelledby="mcp-consent-title">
        <div className="modal-header">
          <div className="modal-header-left">
            <span className="modal-icon">🛡️</span>
            <span className="modal-title" id="mcp-consent-title">Autoriser les outils MCP ?</span>
          </div>
        </div>

        <div className="modal-body">
          <p className="mcp-consent-text">
            Ces serveurs fournissent des outils que l'assistant peut appeler <strong>lui-même</strong>,
            sans vous redemander à chaque fois. Votre choix est enregistré une seule fois et reste
            valable d'une session à l'autre ; vous pouvez le retirer à tout moment dans
            <em> Serveurs MCP</em>.
          </p>

          <div className="mcp-consent-warning">
            <strong>À savoir.</strong> L'assistant lit aussi des contenus externes (documents, pages web).
            Un contenu piégé peut tenter de lui faire utiliser ces outils. N'autorisez que des serveurs de
            confiance, et redoublez d'attention pour ceux qui écrivent ou accèdent à vos fichiers.
          </div>

          {modified.length > 0 && (
            <div className="mcp-consent-warning mcp-consent-warning--danger">
              <strong>Binaire modifié.</strong> Le programme de {modified.join(', ')} a changé depuis votre
              dernière autorisation (mise à jour ou altération). Ne le réautorisez que si vous en connaissez la cause.
            </div>
          )}

          <div className="mcp-consent-list">
            {entries.map(e => (
              <label key={e.key} className="mcp-consent-item">
                <input type="checkbox" checked={selected.has(e.key)} onChange={() => toggle(e.key)} />
                <span className="mcp-consent-item__icon">{e.kind === 'stdio' ? '🖥️' : '⚡'}</span>
                <span className="mcp-consent-item__text">
                  <span className="mcp-consent-item__name">{e.label}</span>
                  {e.detail && <span className="mcp-consent-item__detail">{e.detail}</span>}
                </span>
              </label>
            ))}
          </div>

          {hasLocal && (
            <div className="field-hint">
              <span className="hint-icon">🔐</span>
              <span>
                Pour un programme local, Demeter mémorise son empreinte (SHA-256) : s'il est modifié,
                il ne sera plus lancé tant que vous ne l'aurez pas autorisé de nouveau.
              </span>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn-ghost" onClick={onCancel}>Pas maintenant</button>
          <button className="btn-primary" disabled={selected.size === 0} onClick={() => onConfirm(Array.from(selected))}>
            Autoriser{selected.size > 1 ? ` (${selected.size})` : ''}
          </button>
        </div>
      </div>
    </div>
  );
}
