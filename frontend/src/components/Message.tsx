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

import React, { useState, memo, useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { makeMdComponents } from './MarkdownComponents';
import { RagSourcesBadge } from './RagSourcesBadge';
import type { RagSource } from './RagSourcesBadge';
import { normalizeLatex, normalizeHeadings } from '../utils/text';
import { hasArtifacts } from '../utils/artifacts';

export interface Attachment {
  filename: string;
  ext: string;
  type: 'image' | 'doc';
  chars?: number;
  dataUrl?: string;
  text?: string;
}

export interface ChatMessage {
  role: 'user' | 'assistant';
  content: string | Array<{ type: string; text?: string; image_url?: { url: string } }>;
  displayContent?: string;
  streaming?: boolean;
  attachments?: Attachment[] | null;
  ragSources?: RagSource[];
}

interface MessageProps {
  msg: ChatMessage;
  msgIndex: number;
  onOpenArtifacts: (index: number) => void;
  onRegenerate: (index: number) => void;
  onEditResend: (index: number, newText: string) => void;
  onStop: () => void;
  isLast: boolean;
  loading: boolean;
}

/**
 * Extrait le titre d'un bloc ```word``` :
 * 1. Texte inline après ```word sur la même ligne
 * 2. Premier titre # dans le corps du bloc
 * 3. Retourne null si aucun bloc word trouvé
 */
function extractWordTitle(content: string): string | null {
  const wordStart = content.indexOf('```word');
  if (wordStart === -1) return null;

  const titleStart = wordStart + 7;
  const titleEnd = content.indexOf('\n', titleStart);
  if (titleEnd === -1) return null;

  const inlineTitle = content.slice(titleStart, titleEnd).trim();
  if (inlineTitle) return inlineTitle;

  const body = content.slice(titleEnd + 1);
  const h1Match = body.match(/^#\s+(.+)/m);
  if (h1Match) return h1Match[1].trim();

  return null;
}

export const Message = memo(function Message({
  msg, msgIndex, onOpenArtifacts, onRegenerate, onEditResend, onStop, isLast, loading,
}: MessageProps) {
  const streamingComponents = useMemo(() => makeMdComponents(true),  []);
  const finalComponents     = useMemo(() => makeMdComponents(false), []);

  const isUser = msg.role === 'user';
  const contentStr = typeof msg.content === 'string' ? msg.content : '';
  const showArtifactsBtn = !isUser && !msg.streaming && hasArtifacts(contentStr);

  // Titre du document Word pour le texte d'accompagnement (null si pas de bloc word)
  // Mémoïsé pour éviter de re-parser le contenu (potentiellement long) à chaque frappe dans le ChatInput
  const wordTitle = useMemo(
    () => !isUser && !msg.streaming ? extractWordTitle(contentStr) : null,
    [isUser, msg.streaming, contentStr],
  );

  const [editing, setEditing]   = useState(false);
  const [editText, setEditText] = useState('');

  const startEdit   = () => { setEditText(msg.displayContent ?? contentStr); setEditing(true); };
  const cancelEdit  = () => setEditing(false);
  const confirmEdit = () => { if (editText.trim()) { onEditResend(msgIndex, editText); setEditing(false); } };
  const onEditKey   = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); confirmEdit(); }
    if (e.key === 'Escape') cancelEdit();
  };

  return (
    <div className={`message-row ${isUser ? 'user' : 'assistant'}`}>
      {!isUser && (
        <div className="avatar assistant-avatar"><img src="/ico-demeter.png" alt="Demeter" className="assistant-avatar-img" /></div>
      )}
      <div className={`bubble ${isUser ? 'bubble-user' : 'bubble-assistant'}`}>

        {/* Attachments */}
        {msg.attachments && msg.attachments.length > 0 && (
          <div className="bubble-attachments">
            {msg.attachments.map((a, i) => (
              <span key={i} className="bubble-attach-tag">
                {a.ext === 'pdf' ? '📄' : '📝'} {a.filename}
                {a.chars != null && <span className="bubble-attach-size"> · {(a.chars / 1000).toFixed(1)}k car.</span>}
              </span>
            ))}
          </div>
        )}

        {/* Content */}
        {editing ? (
          <div className="edit-area">
            <textarea
              className="edit-textarea"
              value={editText}
              onChange={e => setEditText(e.target.value)}
              onKeyDown={onEditKey}
              autoFocus
              rows={Math.max(2, editText.split('\n').length)}
            />
            <div className="edit-actions">
              <button className="edit-confirm-btn" onClick={confirmEdit}>Envoyer</button>
              <button className="edit-cancel-btn"  onClick={cancelEdit}>Annuler</button>
            </div>
          </div>
        ) : contentStr === '' && msg.role === 'assistant'
          ? <span className="typing-dots"><span /><span /><span /></span>
          : isUser
            ? <span className="user-text">{msg.displayContent ?? contentStr}</span>
            : msg.streaming
              ? (
                <div className="bubble-markdown">
                  <ReactMarkdown remarkPlugins={[remarkGfm]} components={streamingComponents}>
                    {normalizeHeadings(normalizeLatex(contentStr))}
                  </ReactMarkdown>
                </div>
              )
              : (
                <div className="bubble-markdown">
                  <ReactMarkdown remarkPlugins={[remarkGfm, remarkMath]} rehypePlugins={[rehypeKatex]} components={finalComponents}>
                    {normalizeHeadings(normalizeLatex(contentStr))}
                  </ReactMarkdown>
                </div>
              )
        }

        {/* Texte d'accompagnement pour les documents Word */}
        {wordTitle && !msg.streaming && (
          <p className="bubble-word-intro">
            Le document <strong>« {wordTitle} »</strong> a été généré et est disponible dans le panneau Artefacts. Vous pouvez le prévisualiser ou le télécharger en .docx.
          </p>
        )}

        {/* RAG Sources */}
        {!isUser && !msg.streaming && msg.ragSources && msg.ragSources.length > 0 && (
          <RagSourcesBadge sources={msg.ragSources} />
        )}

        {/* Footer actions */}
        {(showArtifactsBtn || (!editing && !loading && isUser) || (!editing && !msg.streaming && !isUser && isLast)) && (
          <div className="bubble-footer">
            {showArtifactsBtn && (
              <button className="artifacts-trigger" onClick={() => onOpenArtifacts(msgIndex)}>
                <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z"/></svg>
                Artéfacts
              </button>
            )}
            {!editing && !loading && isUser && (
              <button className="msg-action-btn" title="Modifier et renvoyer" onClick={startEdit}>
                <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M11.5 1.5a1.5 1.5 0 012.12 2.12l-9 9a1 1 0 01-.38.24l-3 1a.5.5 0 01-.63-.63l1-3a1 1 0 01.24-.38l9-9z"/></svg>
                Modifier
              </button>
            )}
            {!editing && !msg.streaming && !isUser && isLast && !loading && (
              <button className="msg-action-btn" title="Régénérer la réponse" onClick={() => onRegenerate(msgIndex)}>
                <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor"><path d="M13.5 2.5A6.5 6.5 0 012.07 9H1a.5.5 0 01-.35-.85l2-2a.5.5 0 01.7 0l2 2A.5.5 0 015 9H3.93A5.5 5.5 0 1013 8a.5.5 0 011 0 6.5 6.5 0 01-.5 2.5z"/></svg>
                Régénérer
              </button>
            )}
          </div>
        )}

        {/* Stop while streaming */}
        {msg.streaming && isLast && (
          <div className="bubble-footer">
            <button className="stop-btn" onClick={onStop} title="Arrêter la génération">
              <svg viewBox="0 0 16 16" width="11" height="11" fill="currentColor"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
              Stop
            </button>
          </div>
        )}
      </div>
      {isUser && <div className="avatar user-avatar">👤</div>}
    </div>
  );
});
