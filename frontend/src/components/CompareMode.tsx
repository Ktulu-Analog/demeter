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

// ============================================================================
// Demeter — Mode Comparaison
// Permet de lancer la même question vers deux modèles simultanément
// et de comparer les réponses côte à côte.
// ============================================================================

import React, {
  useState, useCallback, useRef, useEffect, memo, useMemo,
} from 'react';
import { createPortal } from 'react-dom';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { makeMdComponents } from './MarkdownComponents';
import { normalizeLatex, normalizeHeadings } from '../utils/text';
import { API_BASE } from '../constants';
import type { Settings } from './Modals';
import type { Attachment } from './Message';

// ── Types ─────────────────────────────────────────────────────────────────────

interface CompareSlot {
  model: string;
  content: string;
  streaming: boolean;
  error: string | null;
  promptTokens: number;
  completionTokens: number;
  durationMs: number | null;
}

export interface CompareTurn {
  userContent: string;
  displayContent: string;
  slots: [CompareSlot, CompareSlot];
}

export interface CompareModeProps {
  settings: Settings;
  availableModels: string[];
  modelMaxCtx: Record<string, number>;
  activeSpace: { id: string } | null;
  webSearch: boolean;
  onWebSearchChange: (v: boolean) => void;
  collectionId: string | null;
  attachments: Attachment[];
  onAttachmentsChange: (atts: Attachment[]) => void;
  onFilePickerClick: () => void;
  onMcpClick: () => void;
  extracting: boolean;
  onSelectResponse: (turns: CompareTurn[], chosenSlotIndex: 0 | 1) => void;
  onClose: () => void;
}

// ── Helpers ───────────────────────────────────────────────────────────────────

function emptySlot(model: string): CompareSlot {
  return { model, content: '', streaming: false, error: null, promptTokens: 0, completionTokens: 0, durationMs: null };
}

// Fix #4 : passe les mcp_servers des settings au lieu du tableau vide codé en dur
async function streamIntoSlot(
  slot: CompareSlot,
  history: { role: string; content: unknown }[],
  settings: Settings,
  spaceId: string | null,
  webSearch: boolean,
  collectionId: string | null,
  onDelta: (delta: string) => void,
  onUsage: (pt: number, ct: number) => void,
  onDone: () => void,
  onError: (msg: string) => void,
  signal: AbortSignal,
) {
  const body: Record<string, unknown> = {
    messages: history,
    model: slot.model,
    stream: true,
    space_id: spaceId,
    mcp_servers: [...(settings.mcp_servers || []).map(s => s.url), ...(webSearch && settings.web_search_mcp ? [settings.web_search_mcp] : [])],
  };
  if (collectionId != null) body.collection_id = collectionId;

  try {
    const res = await fetch(`${API_BASE}/api-proxy/api/chat`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
      signal,
    });
    if (!res.ok) {
      onError(`Erreur ${res.status} : ${await res.text()}`);
      onDone(); // Fix #2 : sans ça checkBothDone() ne se déclenche jamais → UI bloquée
      return;
    }
    const reader = res.body!.getReader();
    const decoder = new TextDecoder();
    let tail = '';
    let pendingEvent: string | null = null;

    outer: while (true) {
      const { done, value } = await reader.read();
      if (value) tail += decoder.decode(value, { stream: !done });
      // Fix #1 : parser tail AVANT de vérifier done, sinon le dernier chunk est abandonné
      const nl = tail.lastIndexOf('\n');
      if (nl !== -1) {
        const block = tail.slice(0, nl + 1); tail = tail.slice(nl + 1);
        for (const line of block.split('\n')) {
          const trimmed = line.trim();
          if (trimmed.startsWith('event:')) { pendingEvent = trimmed.slice(6).trim(); continue; }
          if (!trimmed.startsWith('data:')) { if (!trimmed) pendingEvent = null; continue; }
          const raw = trimmed.slice(5).trim();
          if (raw === '[DONE]') break outer;
          if (!raw) continue;
          if (pendingEvent === 'rag_sources') { pendingEvent = null; continue; }
          pendingEvent = null;
          try {
            const parsed = JSON.parse(raw);
            if (parsed.error) { onError(JSON.stringify(parsed.error)); return; }
            const delta = parsed.choices?.[0]?.delta?.content;
            if (delta) onDelta(delta);
            if (parsed.usage) onUsage(parsed.usage.prompt_tokens ?? 0, parsed.usage.completion_tokens ?? 0);
          } catch { /* ignore */ }
        }
      }
      if (done) break;
    }
    onDone();
  } catch (err) {
    if ((err as Error).name !== 'AbortError') onError((err as Error).message);
    else onDone();
  }
}

// ── ComparePanelLive ──────────────────────────────────────────────────────────

const ComparePanelLive = memo(function ComparePanelLive({
  slot, panelIndex, isWinner, canSelect, onSelect,
}: {
  slot: CompareSlot;
  panelIndex: 0 | 1;
  isWinner: boolean | null;
  canSelect: boolean;     // Fix #3 : activé sur tous les turns terminés
  onSelect: () => void;
}) {
  const components = useMemo(() => makeMdComponents(slot.streaming), [slot.streaming]);
  const accentColor = panelIndex === 0 ? 'var(--compare-accent-a)' : 'var(--compare-accent-b)';
  const label       = panelIndex === 0 ? 'Modèle A' : 'Modèle B';

  const speedLabel = slot.durationMs != null
    ? slot.durationMs < 1000
      ? `${slot.durationMs}ms`
      : `${(slot.durationMs / 1000).toFixed(1)}s`
    : null;

  const tps = (slot.durationMs && slot.completionTokens)
    ? Math.round((slot.completionTokens / slot.durationMs) * 1000)
    : null;

  const showBtn = canSelect && !slot.streaming && !!slot.content && !slot.error;

  return (
    <div
      className={`cmp-panel${isWinner ? ' cmp-panel--winner' : ''}`}
      style={{ '--cmp-accent': accentColor } as React.CSSProperties}
    >
      {/* En-tête */}
      <div className="cmp-panel-header">
        <div className="cmp-panel-label">
          <span className="cmp-panel-badge" style={{ background: accentColor }}>{label}</span>
          <span className="cmp-panel-model-name" title={slot.model}>{slot.model}</span>
        </div>
        <div className="cmp-metrics">
          {slot.promptTokens > 0 && (
            <span className="cmp-metric" title="Tokens prompt → completion">
              🔢 {slot.promptTokens.toLocaleString()} → {slot.completionTokens.toLocaleString()}
            </span>
          )}
          {speedLabel && (
            <span className="cmp-metric" title="Durée totale">⏱ {speedLabel}</span>
          )}
          {tps != null && (
            <span className="cmp-metric" title="Tokens par seconde">⚡ {tps} t/s</span>
          )}
        </div>
        {isWinner && <span className="cmp-winner-badge">🏆 Plus rapide</span>}
      </div>

      {/* Corps */}
      <div className="cmp-panel-body">
        {slot.error ? (
          <div className="cmp-error">❌ {slot.error}</div>
        ) : slot.content === '' && slot.streaming ? (
          <span className="typing-dots"><span /><span /><span /></span>
        ) : slot.content === '' ? (
          <span className="cmp-placeholder">En attente…</span>
        ) : (
          <div className="bubble-markdown cmp-markdown">
            <ReactMarkdown
              remarkPlugins={slot.streaming ? [remarkGfm] : [remarkGfm, remarkMath]}
              rehypePlugins={slot.streaming ? [] : [rehypeKatex]}
              components={components}
            >
              {normalizeHeadings(normalizeLatex(slot.content))}
            </ReactMarkdown>
          </div>
        )}
      </div>

      {/* Pied : bouton de sélection */}
      {showBtn && (
        <div className="cmp-panel-footer">
          <button
            className="cmp-select-btn"
            style={{ '--cmp-accent': accentColor } as React.CSSProperties}
            onClick={onSelect}
            title="Utiliser cette réponse dans le chat principal"
          >
            <svg viewBox="0 0 16 16" fill="currentColor" width="13" height="13">
              <path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z"/>
            </svg>
            Utiliser cette réponse
          </button>
        </div>
      )}
    </div>
  );
});

// ── CompareTurnLive ───────────────────────────────────────────────────────────

const CompareTurnLive = memo(function CompareTurnLive({
  turn, turnIndex, totalTurns, onSelectA, onSelectB,
}: {
  turn: CompareTurn;
  turnIndex: number;
  totalTurns: number;
  onSelectA: () => void;
  onSelectB: () => void;
}) {
  const aMs = turn.slots[0].durationMs;
  const bMs = turn.slots[1].durationMs;
  const winnerA: boolean | null = (aMs != null && bMs != null) ? (aMs < bMs) : null;

  const bothDone = !turn.slots[0].streaming && !turn.slots[1].streaming;
  const isLast   = turnIndex === totalTurns - 1;

  return (
    <div className="cmp-turn">
      {!isLast && bothDone && (
        <div className="cmp-turn-label">Tour {turnIndex + 1} sur {totalTurns}</div>
      )}

      <div className="cmp-user-row">
        <div className="avatar user-avatar">👤</div>
        <div className="bubble bubble-user">
          <span className="user-text">{turn.displayContent}</span>
        </div>
      </div>

      <div className="cmp-panels-row">
        <ComparePanelLive
          slot={turn.slots[0]}
          panelIndex={0}
          isWinner={winnerA === true}
          canSelect={bothDone}
          onSelect={onSelectA}
        />
        <div className="cmp-divider" />
        <ComparePanelLive
          slot={turn.slots[1]}
          panelIndex={1}
          isWinner={winnerA === false}
          canSelect={bothDone}
          onSelect={onSelectB}
        />
      </div>
    </div>
  );
});

// ── CompareModeView principal ──────────────────────────────────────────────────

// ── Jauge de contexte ─────────────────────────────────────────────────────────

function ContextGauge({ used, max, accentColor }: { used: number; max: number | null; accentColor: string }) {
  if (!max) return null;
  const pct = Math.min(100, Math.round((used / max) * 100));
  const warn = pct >= 80;
  const crit = pct >= 95;
  const color = crit ? 'var(--red-mid, #e53e3e)' : warn ? 'var(--orange-mid, #dd6b20)' : accentColor;
  return (
    <div className="cmp-ctx-gauge" title={`Contexte : ${used.toLocaleString()} / ${max.toLocaleString()} tokens (${pct}%)`}>
      <div className="cmp-ctx-bar">
        <div className="cmp-ctx-fill" style={{ width: `${pct}%`, background: color }} />
      </div>
      <span className="cmp-ctx-label" style={{ color: warn ? color : undefined }}>
        {used > 0 ? `${pct}%` : `/ ${(max / 1000).toFixed(0)}k`}
      </span>
    </div>
  );
}

export function CompareModeView({
  settings, availableModels, modelMaxCtx, activeSpace, webSearch, onWebSearchChange,
  collectionId, attachments, onAttachmentsChange, onFilePickerClick, onMcpClick, extracting,
  onSelectResponse, onClose,
}: CompareModeProps) {
  const [modelA, setModelA] = useState<string>(() => availableModels[0] || settings.model || '');
  const [modelB, setModelB] = useState<string>(() => availableModels[1] || availableModels[0] || settings.model || '');

  const maxCtxA = modelMaxCtx[modelA] ?? null;
  const maxCtxB = modelMaxCtx[modelB] ?? null;

  const [turns,    setTurns]    = useState<CompareTurn[]>([]);
  const [loading,  setLoading]  = useState(false);
  const [inputVal, setInputVal] = useState('');

  // Ref miroir de turns pour que les callbacks des boutons "Utiliser" lisent
  // toujours la liste complète courante, même depuis un composant mémoïsé.
  const turnsRef = useRef<CompareTurn[]>([]);
  useEffect(() => { turnsRef.current = turns; }, [turns]);

  // Fix #1 : historiques séparés par modèle pour éviter la contamination croisée
  // Stockés en ref (pas en state) car leur mutation ne doit pas déclencher de rendu.
  const historyA = useRef<{ role: string; content: unknown }[]>([]);
  const historyB = useRef<{ role: string; content: unknown }[]>([]);

  const abortA = useRef<AbortController | null>(null);
  const abortB = useRef<AbortController | null>(null);
  const textareaRef  = useRef<HTMLTextAreaElement>(null);
  const [dragOver, setDragOver] = useState(false);

  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setDragOver(true); };
  const handleDragLeave = () => setDragOver(false);
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault(); setDragOver(false);
    if (!e.dataTransfer.files.length) return;
    onFilePickerClick();
  };

  // Fix #5 : scroll automatique qui respecte le scroll manuel de l'utilisateur
  const userScrolledRef = useRef(false);
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const onScroll = () => {
      // Si l'utilisateur est à moins de 80px du bas → il "suit" le stream
      const nearBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80;
      userScrolledRef.current = !nearBottom;
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  useEffect(() => {
    const isStreaming = turns.some(t => t.slots[0].streaming || t.slots[1].streaming);
    if (!isStreaming) return;
    if (userScrolledRef.current) return;                // l'utilisateur a scrollé manuellement → ne pas forcer
    // scrollIntoView() peut involontairement scroller window/overlay et corrompre
    // le layout position:fixed après plusieurs turns. On scrolle directement bodyRef.
    const el = bodyRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [turns]);

  const stopAll = useCallback(() => {
    abortA.current?.abort(); abortA.current = null;
    abortB.current?.abort(); abortB.current = null;
    setLoading(false);
    setTurns(prev => prev.map(t => ({
      ...t,
      slots: [
        { ...t.slots[0], streaming: false },
        { ...t.slots[1], streaming: false },
      ],
    })));
  }, []);

  const sendMessage = useCallback(async () => {
    const text = inputVal.trim();
    if ((!text && !attachments.length) || loading) return;
    if (!modelA || !modelB) return;

    setInputVal('');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';

    // Construire le contenu utilisateur (texte + pièces jointes éventuelles)
    let userContent: unknown = text;
    let displayContent = text;

    if (attachments.length) {
      const images = attachments.filter(a => a.type === 'image');
      const docs   = attachments.filter(a => a.type !== 'image');
      displayContent = text || `Analyse de : ${attachments.map(a => a.filename).join(', ')}`;
      const parts: unknown[] = [];
      for (const img of images) {
        const [, mediaType, data] = img.dataUrl!.match(/^data:([^;]+);base64,(.+)$/) || [];
        if (data) parts.push({ type: 'image', source: { type: 'base64', media_type: mediaType, data } });
      }
      for (const doc of docs) {
        if (doc.text) parts.push({ type: 'text', text: `[${doc.filename}]\n${doc.text}` });
      }
      if (text) parts.push({ type: 'text', text });
      userContent = parts.length === 1 && (parts[0] as { type: string }).type === 'text'
        ? (parts[0] as { text: string }).text
        : parts;
    }

    onAttachmentsChange([]);

    // Fix #1 : ajouter le message utilisateur dans les DEUX historiques séparés
    historyA.current.push({ role: 'user', content: userContent });
    historyB.current.push({ role: 'user', content: userContent });

    // Snapshots immuables pour cet appel (les refs peuvent muter entre-temps)
    const snapshotA = [...historyA.current];
    const snapshotB = [...historyB.current];

    const turnIdx = turns.length;
    const newTurn: CompareTurn = {
      userContent: typeof userContent === 'string' ? userContent : JSON.stringify(userContent),
      displayContent,
      slots: [emptySlot(modelA), emptySlot(modelB)],
    };

    setTurns(prev => [...prev, newTurn]);
    setLoading(true);
    userScrolledRef.current = false;   // nouveau tour → retour au scroll automatique

    const startTurn = Date.now();

    const ctrlA = new AbortController();
    const ctrlB = new AbortController();
    abortA.current = ctrlA;
    abortB.current = ctrlB;

    const slotUpdate = (slotIdx: 0 | 1, patch: Partial<CompareSlot> | ((prev: CompareSlot) => Partial<CompareSlot>)) => {
      setTurns(prev => {
        const next = [...prev];
        const turn = next[turnIdx];
        if (!turn) return prev; // Fix #3 : garde défensive contre re-render concurrent
        const updatedTurn = { ...turn };
        const slots: [CompareSlot, CompareSlot] = [...updatedTurn.slots] as [CompareSlot, CompareSlot];
        const resolved = typeof patch === 'function' ? patch(slots[slotIdx]) : patch;
        slots[slotIdx] = { ...slots[slotIdx], ...resolved };
        updatedTurn.slots = slots;
        next[turnIdx] = updatedTurn;
        return next;
      });
    };

    slotUpdate(0, { streaming: true });
    slotUpdate(1, { streaming: true });

    let doneA = false;
    let doneB = false;
    // Accumule la réponse de chaque modèle pour l'injecter dans son historique propre
    let contentA = '';
    let contentB = '';

    const checkBothDone = () => {
      if (!doneA || !doneB) return;

      // Fix : ne jamais pousser un message assistant vide dans l'historique.
      // Un content vide (erreur, abort) provoquerait un 400 "Assistant message
      // must have either content or tool_calls" au tour suivant.
      if (contentA.trim()) {
        historyA.current.push({ role: 'assistant', content: contentA });
      } else {
        // Erreur côté A : retirer aussi le message user pour garder l'historique cohérent
        historyA.current.pop();
      }
      if (contentB.trim()) {
        historyB.current.push({ role: 'assistant', content: contentB });
      } else {
        historyB.current.pop();
      }

      // Troncature glissante : on garde au maximum les N derniers messages
      // (hors system prompt) pour éviter les 502 par dépassement de contexte.
      const MAX_HISTORY = 20;
      if (historyA.current.length > MAX_HISTORY)
        historyA.current = historyA.current.slice(-MAX_HISTORY);
      if (historyB.current.length > MAX_HISTORY)
        historyB.current = historyB.current.slice(-MAX_HISTORY);

      setLoading(false);
    };

    streamIntoSlot(
      { ...newTurn.slots[0] }, snapshotA, settings,
      activeSpace?.id ?? null, webSearch, collectionId,
      delta => { contentA += delta; slotUpdate(0, prev => ({ content: prev.content + delta })); },
      (pt, ct) => slotUpdate(0, { promptTokens: pt, completionTokens: ct }),
      () => { doneA = true; slotUpdate(0, { streaming: false, durationMs: Date.now() - startTurn }); checkBothDone(); },
      msg => { doneA = true; slotUpdate(0, { streaming: false, error: msg, durationMs: Date.now() - startTurn }); checkBothDone(); },
      ctrlA.signal,
    );

    streamIntoSlot(
      { ...newTurn.slots[1] }, snapshotB, settings,
      activeSpace?.id ?? null, webSearch, collectionId,
      delta => { contentB += delta; slotUpdate(1, prev => ({ content: prev.content + delta })); },
      (pt, ct) => slotUpdate(1, { promptTokens: pt, completionTokens: ct }),
      () => { doneB = true; slotUpdate(1, { streaming: false, durationMs: Date.now() - startTurn }); checkBothDone(); },
      msg => { doneB = true; slotUpdate(1, { streaming: false, error: msg, durationMs: Date.now() - startTurn }); checkBothDone(); },
      ctrlB.signal,
    );
  }, [inputVal, loading, modelA, modelB, turns, settings, activeSpace, webSearch, collectionId, attachments, onAttachmentsChange]);

  const handleKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
  };

  const autoResize = () => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = 'auto';
    el.style.height = Math.min(el.scrollHeight, 120) + 'px';
  };

  const isStreaming = turns.some(t => t.slots[0].streaming || t.slots[1].streaming);

  return createPortal(
    <div className="cmp-overlay">
      {/* Header */}
      <div className="cmp-header">
        <div className="cmp-header-left">
          <div className="cmp-icon">⚖️</div>
          <div>
            <div className="cmp-title">Mode Comparaison</div>
            <div className="cmp-subtitle">Comparez deux modèles côte à côte sur la même question</div>
          </div>
        </div>
        <button className="cmp-close-btn" onClick={onClose} title="Fermer">
          <svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14">
            <path d="M3.72 3.72a.75.75 0 011.06 0L8 6.94l3.22-3.22a.75.75 0 111.06 1.06L9.06 8l3.22 3.22a.75.75 0 11-1.06 1.06L8 9.06l-3.22 3.22a.75.75 0 01-1.06-1.06L6.94 8 3.72 4.78a.75.75 0 010-1.06z" />
          </svg>
          Fermer
        </button>
      </div>

      {/* Sélecteurs de modèles */}
      <div className="cmp-model-bar">
        <div className="cmp-model-slot cmp-model-slot--a">
          <div className="cmp-model-slot-row">
            <span className="cmp-model-badge" style={{ background: 'var(--compare-accent-a)' }}>A</span>
            <select
              className="cmp-model-select cmp-model-select--bar"
              value={modelA}
              onChange={e => setModelA(e.target.value)}
              disabled={isStreaming}
            >
              {availableModels.map(m => <option key={m} value={m}>{m}</option>)}
              {!availableModels.includes(modelA) && <option value={modelA}>{modelA}</option>}
            </select>
          </div>
          <ContextGauge
            used={turns.findLast(t => t.slots[0].promptTokens > 0)?.slots[0].promptTokens ?? 0}
            max={maxCtxA}
            accentColor="var(--compare-accent-a)"
          />
        </div>

        <div className="cmp-model-vs">VS</div>

        <div className="cmp-model-slot cmp-model-slot--b">
          <div className="cmp-model-slot-row">
            <span className="cmp-model-badge" style={{ background: 'var(--compare-accent-b)' }}>B</span>
            <select
              className="cmp-model-select cmp-model-select--bar"
              value={modelB}
              onChange={e => setModelB(e.target.value)}
              disabled={isStreaming}
            >
              {availableModels.map(m => <option key={m} value={m}>{m}</option>)}
              {!availableModels.includes(modelB) && <option value={modelB}>{modelB}</option>}
            </select>
          </div>
          <ContextGauge
            used={turns.findLast(t => t.slots[1].promptTokens > 0)?.slots[1].promptTokens ?? 0}
            max={maxCtxB}
            accentColor="var(--compare-accent-b)"
          />
        </div>
      </div>

      {/* Zone de conversation */}
      <div className="cmp-body" ref={bodyRef}>
        {turns.length === 0 && (
          <div className="cmp-empty">
            <div className="cmp-empty-icon">⚖️</div>
            <div className="cmp-empty-title">Prêt à comparer</div>
            <div className="cmp-empty-sub">
              Saisissez votre question ci-dessous. Les deux modèles répondront simultanément.
            </div>
            {activeSpace && (
              <div className="cmp-empty-space">
                Espace actif : <strong>{(activeSpace as { icon?: string; label?: string }).icon} {(activeSpace as { label?: string }).label}</strong>
              </div>
            )}
          </div>
        )}

        {turns.map((turn, i) => (
          <CompareTurnLive
            key={i}
            turn={turn}
            turnIndex={i}
            totalTurns={turns.length}
            onSelectA={() => onSelectResponse(turnsRef.current.slice(0, i + 1), 0)}
            onSelectB={() => onSelectResponse(turnsRef.current.slice(0, i + 1), 1)}
          />
        ))}


      </div>

      {/* Zone de saisie */}
      <div className="cmp-input-zone">
        <div
          className={`cmp-input-box${extracting ? ' cmp-input-box--loading' : ''}${dragOver ? ' cmp-input-box--dragover' : ''}`}
          onDragOver={handleDragOver}
          onDragLeave={handleDragLeave}
          onDrop={handleDrop}
        >
          {dragOver && (
            <div className="cmp-input-drop-overlay">
              <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.5"><path strokeLinecap="round" strokeLinejoin="round" d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"/></svg>
              Déposer l'image ici
            </div>
          )}

          {/* Pièces jointes */}
          {attachments.length > 0 && (
            <div className="input-attachments">
              {attachments.map((att, i) => (
                <div key={i} className={`input-attach-chip${att.type === 'image' ? ' input-attach-chip--image' : ''}`}>
                  {att.type === 'image' ? (
                    <div className="attach-img-thumb-wrap">
                      <img src={att.dataUrl} alt={att.filename} className="attach-img-thumb" />
                      <div className="attach-img-preview"><img src={att.dataUrl} alt={att.filename} /></div>
                    </div>
                  ) : <span className="attach-icon">{att.ext === 'pdf' ? '📄' : '📝'}</span>}
                  <span className="attach-name">{att.filename}</span>
                  {att.type !== 'image' && att.chars != null && (
                    <span className="attach-chars">{(att.chars / 1000).toFixed(1)}k car.</span>
                  )}
                  <button className="attach-remove" onClick={() => onAttachmentsChange(attachments.filter((_, j) => j !== i))}>✕</button>
                </div>
              ))}
            </div>
          )}

          <textarea
            ref={textareaRef}
            className="cmp-textarea"
            placeholder={
              extracting
                ? 'Extraction en cours…'
                : attachments.length === 1
                  ? `Message à propos de "${attachments[0].filename}"…`
                  : attachments.length > 1
                    ? `Message à propos de ${attachments.length} documents…`
                    : !modelA || !modelB
                      ? 'Sélectionnez deux modèles…'
                      : isStreaming
                        ? 'Génération en cours…'
                        : 'Posez votre question — les deux modèles répondront simultanément…'
            }
            value={inputVal}
            onChange={e => { setInputVal(e.target.value); autoResize(); }}
            onKeyDown={handleKey}
            disabled={isStreaming || !modelA || !modelB || extracting}
            rows={2}
          />

          <div className="cmp-input-toolbar">
            {/* Pièce jointe */}
            <button
              className={`attach-btn${extracting ? ' attach-btn--loading' : ''}${attachments.length ? ' attach-btn--active' : ''}`}
              onClick={onFilePickerClick}
              disabled={isStreaming || extracting}
              title="Joindre un fichier"
            >
              {extracting
                ? <span className="spinner-sm" />
                : <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16"><path fillRule="evenodd" d="M8 4a3 3 0 00-3 3v4a5 5 0 0010 0V7a1 1 0 112 0v4a7 7 0 11-14 0V7a5 5 0 0110 0v4a3 3 0 11-6 0V7a1 1 0 012 0v4a1 1 0 102 0V7a3 3 0 00-3-3z" clipRule="evenodd"/></svg>
              }
            </button>

            {/* Web search toggle */}
            <button
              className={`websearch-pill${webSearch ? ' websearch-pill--on' : ''}`}
              onClick={() => onWebSearchChange(!webSearch)}
              type="button"
              title={webSearch ? 'Désactiver la recherche web' : 'Activer la recherche web'}
            >
              <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11"><path d="M8 1a7 7 0 100 14A7 7 0 008 1zM2.05 8.5h2.01c.06.93.22 1.8.46 2.55A5.01 5.01 0 012.05 8.5zm0-1h2.01A9.2 9.2 0 014.52 4.95 5.01 5.01 0 012.05 7.5zm5.45 5.97V11h-1.7c.22.75.52 1.36.85 1.78.27.35.57.55.85.55v.14zm0-3.47H5.07c-.06-.8-.1-1.62-.1-2.5h2.53v2.5zm0-3.5H4.97c0-.88.04-1.7.1-2.5H7.5V6.5zm0-3.47V1.14c-.28 0-.58.2-.85.55-.33.42-.63 1.03-.85 1.78h1.7v.14zM8.5 13.97V11h1.7c-.22.75-.52 1.36-.85 1.78-.27.35-.57.55-.85.55v.14zm0-3.47V8h2.53c0 .88-.04 1.7-.1 2.5H8.5zm0-3.5V4.5h2.43c.06.8.1 1.62.1 2.5H8.5zm0-3.47V1.14c.28 0 .58.2.85.55.33.42.63 1.03.85 1.78H8.5v.14z"/></svg>
              Web
            </button>

            {/* MCP servers actifs — cliquable comme dans le chat normal */}
            {(settings.mcp_servers || []).length > 0 && (
              <button
                className="websearch-pill websearch-pill--on mcp-active-pill"
                onClick={onMcpClick}
                type="button"
                title={(settings.mcp_servers || []).map(s => s.alias || s.url).join(', ')}
              >
                🔌 {(settings.mcp_servers || []).length === 1
                  ? (settings.mcp_servers![0].alias || '1 MCP')
                  : `${settings.mcp_servers!.length} MCP`}
              </button>
            )}

            <span className="input-hint">Shift+Entrée pour sauter une ligne</span>

            {isStreaming ? (
              <button className="send-btn" onClick={stopAll} title="Arrêter">
                <svg viewBox="0 0 16 16" fill="currentColor" width="11" height="11"><rect x="3" y="3" width="10" height="10" rx="1"/></svg>
              </button>
            ) : (
              <button
                className="send-btn"
                onClick={sendMessage}
                disabled={(!inputVal.trim() && !attachments.length) || !modelA || !modelB || extracting}
              >
                <svg viewBox="0 0 16 16"><path d="M2 8l12-6-4 6 4 6z"/></svg>
              </button>
            )}
          </div>
        </div>
      </div>
    </div>,
    document.body
  );
}
