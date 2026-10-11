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

// ── Transcription audio avec diarization (Albert API /v1/audio/transcriptions) ─

import React, { useState, useEffect, useCallback, useRef } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { API_BASE } from '../constants';
import { useDialog } from '../DialogContext';
import { chunkAudioFile } from '../utils/audioChunking';
import { reconcileChunks, mergeConsecutiveSameSpeaker } from '../utils/speakerReconciliation';
import type { ChunkResult, MergedSegment } from '../utils/speakerReconciliation';
import { generateDocx } from '../utils/docx';
import { DownloadIcon, SpinnerIcon } from './Icons';
import { makeMdComponents } from './MarkdownComponents';
import { normalizeHeadings, normalizeLatex } from '../utils/text';
import { DEFAULT_AUDIO_SUMMARY_PROMPTS } from '../constants';
import type { AudioSummaryPrompt } from '../constants';

// Rendu markdown final (non-streamé) — mêmes composants que dans le chat / les artefacts.
const SUMMARY_MD_COMPONENTS = makeMdComponents(false);

// ── Types ─────────────────────────────────────────────────────────────────────

interface AppSettings {
  endpoint: string;
  bearer: string;
  model?: string;
  audio_summary_prompts?: AudioSummaryPrompt[];
}

interface AlbertModel {
  id: string;
  type?: string | null;
}

interface Segment {
  id: number;
  text: string;
  start: number;
  end: number;
  speaker?: string | null;
}

interface AudioTranscriptionResult {
  id?: string;
  text: string;
  model?: string;
  segments?: Segment[] | null;
  usage?: { prompt_tokens?: number; total_tokens?: number; cost?: number } | null;
}

interface AudioTranscriptionModalProps {
  settings: AppSettings;
  onClose: () => void;
  /** Insère le texte formaté dans le champ de message et ferme la modale. */
  onInsert: (text: string) => void;
}

const ACCEPTED_AUDIO_EXTS = ['.mp3', '.wav'];

// Au-delà de cette taille, le fichier est découpé en tranches chevauchantes
// avant envoi (l'API Albert n'annonce pas de limite explicite — on reste
// prudent, un peu sous les ~25 Mo généralement constatés pour ce type d'API).
const CHUNK_SIZE_THRESHOLD_BYTES = 20 * 1024 * 1024; // 20 Mo
const CHUNK_DURATION_SEC = 8 * 60; // 8 min par tranche
const CHUNK_OVERLAP_SEC = 15; // recouvrement entre tranches, pour la réconciliation des locuteurs

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '00:00';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${String(m).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} Ko`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} Mo`;
}

// Couleurs cycliques pour distinguer visuellement les locuteurs
const SPEAKER_COLORS = ['#3b82f6', '#f59e0b', '#1a9e72', '#ec4899', '#8b5cf6', '#06b6d4', '#ef4444'];
function speakerColor(speaker: string, allSpeakers: string[]): string {
  const idx = Math.max(0, allSpeakers.indexOf(speaker));
  return SPEAKER_COLORS[idx % SPEAKER_COLORS.length];
}

/** Construit un texte au format markdown (locuteurs en gras) pour l'export .docx. */
function buildMarkdownTranscript(result: AudioTranscriptionResult, speakerNames: Record<string, string> = {}): string {
  if (result.segments && result.segments.length > 0) {
    return result.segments
      .map(seg => {
        const speaker = seg.speaker ? `**${resolveSpeakerName(seg.speaker, speakerNames)}** : ` : '';
        return `${speaker}${seg.text.trim()}`;
      })
      .join('\n\n');
  }
  return result.text || '';
}

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'transcription';
}
function resolveSpeakerName(speaker: string, speakerNames: Record<string, string>): string {
  const custom = speakerNames[speaker];
  return custom && custom.trim() ? custom.trim() : speaker;
}

/** Construit le texte formaté (utilisé pour la copie et l'insertion dans le message). */
function buildPlainTranscript(result: AudioTranscriptionResult, speakerNames: Record<string, string> = {}): string {
  if (result.segments && result.segments.length > 0) {
    return result.segments
      .map(seg => {
        const speaker = seg.speaker ? `${resolveSpeakerName(seg.speaker, speakerNames)} : ` : '';
        return `${speaker}${seg.text.trim()}`;
      })
      .join('\n');
  }
  return result.text || '';
}

// ── Composant ─────────────────────────────────────────────────────────────────

export function AudioTranscriptionModal({ settings, onClose, onInsert }: AudioTranscriptionModalProps) {
  const { toast: toastFn } = useDialog();
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [file, setFile] = useState<File | null>(null);
  const [dragOver, setDragOver] = useState(false);

  const [audioModels, setAudioModels] = useState<string[]>([]);
  const [loadingModels, setLoadingModels] = useState(false);
  const [model, setModel] = useState('');

  const [language, setLanguage] = useState(''); // vide = auto-détection
  const [diarization, setDiarization] = useState(true);

  const [loading, setLoading] = useState(false);
  const [progressMsg, setProgressMsg] = useState('');
  const [result, setResult] = useState<AudioTranscriptionResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [exportingDocx, setExportingDocx] = useState(false);

  // ── Résumé automatique (envoi du transcript à un modèle de chat) ────────────
  // Chaque génération est ajoutée à la liste (ne remplace pas la précédente) :
  // l'utilisateur peut ainsi générer plusieurs synthèses (Synthèse, Analyse…)
  // et les conserver/exporter séparément.
  const summaryPrompts: AudioSummaryPrompt[] = settings.audio_summary_prompts?.length
    ? settings.audio_summary_prompts
    : DEFAULT_AUDIO_SUMMARY_PROMPTS;
  const [summaryPromptId, setSummaryPromptId] = useState(summaryPrompts[0]?.id || '');
  const [summarizing, setSummarizing] = useState(false);
  const [summaries, setSummaries] = useState<{ id: string; label: string; text: string }[]>([]);
  const [summaryError, setSummaryError] = useState<string | null>(null);
  const [exportingSummaryId, setExportingSummaryId] = useState<string | null>(null);

  useEffect(() => {
    if (!summaryPromptId && summaryPrompts.length) setSummaryPromptId(summaryPrompts[0].id);
  }, [summaryPrompts, summaryPromptId]);

  const resetSummaryState = () => {
    setSummaries([]);
    setSummaryError(null);
    setSummarizing(false);
  };

  // ── Renommage des locuteurs (ex : "Intervenant 1" → "Alice") ────────────────
  // Clé = libellé original renvoyé par l'API, valeur = nom personnalisé saisi.
  const [speakerNames, setSpeakerNames] = useState<Record<string, string>>({});
  const [editingSpeaker, setEditingSpeaker] = useState<string | null>(null);
  const [editingValue, setEditingValue] = useState('');
  const renameInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editingSpeaker !== null) renameInputRef.current?.focus();
  }, [editingSpeaker]);

  const startEditingSpeaker = (speaker: string) => {
    setEditingSpeaker(speaker);
    setEditingValue(resolveSpeakerName(speaker, speakerNames));
  };

  const commitEditingSpeaker = () => {
    if (editingSpeaker === null) return;
    const original = editingSpeaker;
    const trimmed = editingValue.trim();
    setSpeakerNames(prev => {
      const next = { ...prev };
      if (!trimmed || trimmed === original) {
        delete next[original];
      } else {
        next[original] = trimmed;
      }
      return next;
    });
    setEditingSpeaker(null);
    setEditingValue('');
  };

  const cancelEditingSpeaker = () => {
    setEditingSpeaker(null);
    setEditingValue('');
  };

  // ── Charge la liste des modèles et filtre ceux de type ASR ──────────────────
  useEffect(() => {
    if (!settings.endpoint || !settings.bearer) return;
    setLoadingModels(true);
    fetch(`${API_BASE}/api-proxy/api/models`)
      .then(r => (r.ok ? r.json() : null))
      .then((data: { data?: AlbertModel[] } | null) => {
        const asr = (data?.data || []).filter(m => m.type === 'automatic-speech-recognition').map(m => m.id);
        setAudioModels(asr);
        if (asr.length && !model) setModel(asr[0]);
      })
      .catch(() => { /* silencieux — l'utilisateur pourra saisir le modèle manuellement */ })
      .finally(() => setLoadingModels(false));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.endpoint, settings.bearer]);

  const validateAndSetFile = useCallback((f: File | null) => {
    if (!f) return;
    const ext = '.' + (f.name.split('.').pop() || '').toLowerCase();
    if (!ACCEPTED_AUDIO_EXTS.includes(ext)) {
      toastFn(`Format non supporté : ${ext}. Utilisez un fichier .mp3 ou .wav.`, 'error');
      return;
    }
    setFile(f);
    setResult(null);
    setErrorMsg(null);
    setSpeakerNames({});
    cancelEditingSpeaker();
    resetSummaryState();
  }, [toastFn]);

  const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    validateAndSetFile(e.target.files?.[0] || null);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setDragOver(false);
    validateAndSetFile(e.dataTransfer.files?.[0] || null);
  };

  /** Envoie un unique fichier (ou une tranche) à l'endpoint de transcription. */
  const postTranscription = async (fileArg: File, responseFormat: string): Promise<AudioTranscriptionResult> => {
    const form = new FormData();
    form.append('file', fileArg);
    form.append('model', model.trim());
    form.append('response_format', responseFormat);
    if (language.trim()) form.append('language', language.trim());

    const res = await fetch(
      `${API_BASE}/api-proxy/api/audio/transcriptions`,
      { method: 'POST', body: form },
    );

    const data = await res.json().catch(() => ({}));

    if (!res.ok) {
      const detail = typeof data?.detail === 'string' ? data.detail : (data?.detail ? JSON.stringify(data.detail) : res.statusText);
      throw new Error(detail || `Erreur ${res.status}`);
    }
    return data as AudioTranscriptionResult;
  };

  const handleTranscribe = async () => {
    if (!file) { toastFn('Choisissez un fichier audio (.mp3 ou .wav).', 'error'); return; }
    if (!model.trim()) { toastFn('Choisissez ou saisissez un modèle de transcription.', 'error'); return; }

    setLoading(true);
    setErrorMsg(null);
    setResult(null);
    setSpeakerNames({});
    cancelEditingSpeaker();
    resetSummaryState();

    try {
      if (file.size <= CHUNK_SIZE_THRESHOLD_BYTES) {
        // ── Fichier de taille raisonnable : envoi direct en un seul appel ──
        setProgressMsg('Envoi du fichier et transcription en cours… (cela peut prendre plusieurs minutes selon la durée de l\'audio)');
        const data = await postTranscription(file, diarization ? 'diarized_json' : 'json');
        if (data.segments && data.segments.length > 0) {
          data.segments = mergeConsecutiveSameSpeaker(data.segments as MergedSegment[]);
        }
        setResult(data);
        toastFn('Transcription terminée.');
        return;
      }

      // ── Fichier volumineux : découpage en tranches chevauchantes ──────────
      toastFn(`Fichier volumineux (${formatBytes(file.size)}) : découpage automatique en tranches de ${CHUNK_DURATION_SEC / 60} min avant envoi.`, 'info');
      setProgressMsg('Découpage du fichier audio…');
      const chunks = await chunkAudioFile(file, { chunkSec: CHUNK_DURATION_SEC, overlapSec: CHUNK_OVERLAP_SEC });

      const chunkResults: ChunkResult[] = [];
      let aggTokens = 0;
      let hasTokens = false;
      for (let i = 0; i < chunks.length; i++) {
        setProgressMsg(`Transcription de la tranche ${i + 1}/${chunks.length}…`);
        // On demande toujours diarized_json pour disposer des timestamps nécessaires
        // à la réconciliation entre tranches, même si la diarization est désactivée
        // à l'affichage (dans ce cas, les labels de locuteurs seront simplement masqués).
        const data = await postTranscription(chunks[i].file, 'diarized_json');
        const segments = data.segments && data.segments.length > 0
          ? data.segments
          : [{ id: 0, text: data.text || '', start: 0, end: chunks[i].endSec - chunks[i].startSec, speaker: null }];
        chunkResults.push({ startSec: chunks[i].startSec, segments });
        if (data.usage?.total_tokens != null) { aggTokens += data.usage.total_tokens; hasTokens = true; }
      }

      setProgressMsg('Fusion et réconciliation des locuteurs entre les tranches…');
      let merged = reconcileChunks(chunkResults, CHUNK_OVERLAP_SEC);
      if (!diarization) merged = merged.map(s => ({ ...s, speaker: null }));
      merged = mergeConsecutiveSameSpeaker(merged);

      setResult({
        text: merged.map(s => s.text.trim()).join(' '),
        model: model.trim(),
        segments: merged,
        usage: hasTokens ? { total_tokens: aggTokens } : null,
      });
      toastFn(`Transcription terminée (${chunks.length} tranches fusionnées).`);
    } catch (e) {
      const msg = (e as Error).message || 'Erreur inconnue';
      setErrorMsg(msg);
      toastFn(`Erreur de transcription : ${msg}`, 'error');
    } finally {
      setLoading(false);
      setProgressMsg('');
    }
  };

  const handleCopy = async () => {
    if (!result) return;
    try {
      await navigator.clipboard.writeText(buildPlainTranscript(result, speakerNames));
      toastFn('Transcript copié dans le presse-papiers.');
    } catch {
      toastFn('Impossible de copier.', 'error');
    }
  };

  const handleInsert = () => {
    if (!result) return;
    onInsert(buildPlainTranscript(result, speakerNames));
    onClose();
  };

  const handleExportDocx = async () => {
    if (!result) return;
    setExportingDocx(true);
    try {
      const markdown = buildMarkdownTranscript(result, speakerNames);
      const baseName = file?.name.replace(/\.[^.]+$/, '') || 'transcription';
      await generateDocx(markdown, `${slugify(baseName)}.docx`);
      toastFn('Transcript exporté en .docx.');
    } catch (e) {
      toastFn('Erreur lors de la génération du .docx : ' + (e as Error).message, 'error');
    } finally {
      setExportingDocx(false);
    }
  };

  const handleSummarize = async () => {
    if (!result) return;
    const template = summaryPrompts.find(p => p.id === summaryPromptId) || summaryPrompts[0];
    if (!template) return;
    setSummarizing(true);
    setSummaryError(null);
    try {
      const transcript = buildPlainTranscript(result, speakerNames);
      const res = await fetch(`${API_BASE}/api-proxy/api/chat`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          messages: [
            {
              role: 'system',
              content: "Tu es un assistant qui synthétise des transcriptions audio en français, de façon claire, structurée et fidèle aux propos tenus.",
            },
            {
              role: 'user',
              content: `${template.prompt}\n\n---\n\nTranscription :\n${transcript}`,
            },
          ],
          model: settings.model,
          stream: false,
          space_id: null,
        }),
      });
      if (!res.ok) throw new Error(`Erreur ${res.status} : ${await res.text()}`);
      const data = await res.json();
      const content = data.choices?.[0]?.message?.content;
      if (!content) throw new Error('Réponse vide du modèle.');
      setSummaries(prev => [{ id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`, label: template.label || 'Résumé', text: content }, ...prev]);
    } catch (e) {
      setSummaryError((e as Error).message);
    } finally {
      setSummarizing(false);
    }
  };

  const removeSummary = (id: string) => setSummaries(prev => prev.filter(s => s.id !== id));

  const handleCopySummary = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      toastFn('Résumé copié dans le presse-papiers.');
    } catch {
      toastFn('Impossible de copier.', 'error');
    }
  };

  const handleInsertSummary = (text: string) => {
    onInsert(text);
    onClose();
  };

  const handleExportSummaryDocx = async (summary: { id: string; label: string; text: string }) => {
    setExportingSummaryId(summary.id);
    try {
      const baseName = file?.name.replace(/\.[^.]+$/, '') || 'transcription';
      await generateDocx(summary.text, `${slugify(baseName)}-${slugify(summary.label)}.docx`);
      toastFn('Résumé exporté en .docx.');
    } catch (e) {
      toastFn('Erreur lors de la génération du .docx : ' + (e as Error).message, 'error');
    } finally {
      setExportingSummaryId(null);
    }
  };

  const allSpeakers = Array.from(new Set((result?.segments || []).map(s => s.speaker).filter(Boolean) as string[]));

  return (
    <div className="modal-overlay">
      <div className="modal ingestion-modal audio-transcription-modal">
        <div className="modal-header">
          <div className="modal-header-left">
            <span className="modal-icon">🎙️</span>
            <span className="modal-title">Transcription audio</span>
          </div>
          <button className="icon-btn" onClick={onClose} title="Fermer">✕</button>
        </div>

        <div className="modal-body ingestion-body">

          {/* ── Zone de dépôt de fichier ── */}
          <div className="ingestion-section">
            <label className="field-label">Fichier audio</label>
            <div
              className={`audio-drop-zone ${dragOver ? 'audio-drop-zone--over' : ''} ${file ? 'audio-drop-zone--filled' : ''}`}
              onClick={() => fileInputRef.current?.click()}
              onDragOver={e => { e.preventDefault(); setDragOver(true); }}
              onDragLeave={() => setDragOver(false)}
              onDrop={handleDrop}
            >
              <input ref={fileInputRef} type="file" accept=".mp3,.wav,audio/mpeg,audio/wav" style={{ display: 'none' }} onChange={handleFileChange} />
              {file ? (
                <div className="audio-file-info">
                  <span className="attach-icon">🎵</span>
                  <div className="audio-file-meta">
                    <span className="attach-name">{file.name}</span>
                    <span className="attach-chars">
                      {formatBytes(file.size)}
                      {file.size > CHUNK_SIZE_THRESHOLD_BYTES && ' · sera découpé en tranches avant envoi'}
                    </span>
                  </div>
                  <button className="attach-remove" onClick={e => { e.stopPropagation(); setFile(null); setResult(null); }}>✕</button>
                </div>
              ) : (
                <span className="audio-drop-hint">📎 Glissez un fichier .mp3 / .wav ici, ou cliquez pour parcourir…</span>
              )}
            </div>
          </div>

          {/* ── Options ── */}
          <div className="ingestion-section audio-options-row">
            <div className="field-group">
              <label className="field-label">
                Modèle ASR
                {loadingModels && <span className="field-label-hint">chargement…</span>}
              </label>
              {audioModels.length > 0 ? (
                <select className="field-input" value={model} onChange={e => setModel(e.target.value)} disabled={loading}>
                  {audioModels.map(m => <option key={m} value={m}>{m}</option>)}
                </select>
              ) : (
                <input
                  className="field-input"
                  value={model}
                  onChange={e => setModel(e.target.value)}
                  placeholder="ex : openai/whisper-large-v3"
                  disabled={loading}
                  spellCheck={false}
                />
              )}
              {audioModels.length === 0 && !loadingModels && (
                <div className="field-hint">Aucun modèle `automatic-speech-recognition` détecté — saisissez son identifiant manuellement.</div>
              )}
            </div>

            <div className="field-group">
              <label className="field-label">Langue <span className="field-label-hint">optionnel</span></label>
              <input
                className="field-input"
                value={language}
                onChange={e => setLanguage(e.target.value)}
                placeholder="auto (ex : fr, en…)"
                disabled={loading}
                spellCheck={false}
              />
            </div>

            <div className="field-group audio-diarization-toggle">
              <label className="field-label">Diarization</label>
              <button
                className={`toggle-switch ${diarization ? 'toggle-switch--on' : ''}`}
                onClick={() => setDiarization(v => !v)}
                disabled={loading}
                title="Identifier les différents locuteurs"
              >
                <span className="toggle-switch-knob" />
              </button>
            </div>
          </div>

          <div className="modal-footer" style={{ padding: '4px 0 12px' }}>
            <button className="btn-primary" onClick={handleTranscribe} disabled={loading || !file}>
              {loading ? <><span className="spinner-sm" /> Transcription…</> : '🎙️ Transcrire'}
            </button>
          </div>

          {loading && (
            <div className="ingestion-status-row">
              <span className="spinner-sm" /> {progressMsg}
            </div>
          )}

          {errorMsg && (
            <div className="ingestion-status-row ingestion-status-row--warn">
              <span className="ingestion-status-dot ingestion-status-dot--amber" />
              <span>{errorMsg}</span>
            </div>
          )}

          {/* ── Résultat ── */}
          {result && (
            <div className="ingestion-section audio-result-section">
              <div className="audio-result-columns">

                {/* ── Colonne gauche : transcript ── */}
                <div className="audio-result-col">
                  <div className="ingestion-collections-header">
                    <label className="field-label" style={{ margin: 0 }}>Transcript</label>
                    <div className="ingestion-cache-info">
                      {result.usage?.total_tokens != null && (
                        <span className="ingestion-cache-age">{result.usage.total_tokens} tokens</span>
                      )}
                      <button
                        className="ingestion-refresh-btn"
                        onClick={handleExportDocx}
                        disabled={exportingDocx}
                        title="Exporter en .docx"
                      >
                        {exportingDocx ? <SpinnerIcon /> : <DownloadIcon />}
                      </button>
                      <button className="ingestion-refresh-btn" onClick={handleCopy} title="Copier le transcript">
                        <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path d="M8 3a2 2 0 00-2 2v9a2 2 0 002 2h7a2 2 0 002-2V8.414A2 2 0 0016.414 7L13 3.586A2 2 0 0011.586 3H8z"/><path d="M4 7a2 2 0 00-2 2v9a2 2 0 002 2h7a2 2 0 002-2v-1H6a3 3 0 01-3-3V7H4z"/></svg>
                      </button>
                    </div>
                  </div>

                  <div className="audio-transcript-panel">
                    {result.segments && result.segments.length > 0 ? (
                      result.segments.map(seg => (
                        <div key={seg.id} className="audio-segment-row">
                          {seg.speaker && (
                            editingSpeaker === seg.speaker ? (
                              <input
                                ref={renameInputRef}
                                className="audio-segment-speaker-input"
                                style={{ background: speakerColor(seg.speaker, allSpeakers) }}
                                value={editingValue}
                                onChange={e => setEditingValue(e.target.value)}
                                onBlur={commitEditingSpeaker}
                                onKeyDown={e => {
                                  if (e.key === 'Enter') { e.preventDefault(); commitEditingSpeaker(); }
                                  if (e.key === 'Escape') { e.preventDefault(); cancelEditingSpeaker(); }
                                }}
                              />
                            ) : (
                              <span
                                className="audio-segment-speaker"
                                style={{ background: speakerColor(seg.speaker, allSpeakers) }}
                                onDoubleClick={() => startEditingSpeaker(seg.speaker as string)}
                                title="Double-cliquez pour renommer ce locuteur"
                              >
                                {resolveSpeakerName(seg.speaker, speakerNames)}
                              </span>
                            )
                          )}
                          <span className="audio-segment-text">{seg.text.trim()}</span>
                        </div>
                      ))
                    ) : (
                      <p className="audio-transcript-plain">{result.text}</p>
                    )}
                  </div>
                </div>

                {/* ── Colonne droite : résumé automatique ── */}
                <div className="audio-result-col">
                  <div className="ingestion-collections-header">
                    <label className="field-label" style={{ margin: 0 }}>Résumé automatique</label>
                  </div>
                  <div className="audio-summary-section">
                    <div className="audio-summary-controls">
                      <select
                        className="field-input audio-summary-select"
                        value={summaryPromptId}
                        onChange={e => setSummaryPromptId(e.target.value)}
                        disabled={summarizing || summaryPrompts.length === 0}
                      >
                        {summaryPrompts.map(p => (
                          <option key={p.id} value={p.id}>{p.label || 'Sans nom'}</option>
                        ))}
                      </select>
                      <button
                        className="btn-primary"
                        onClick={handleSummarize}
                        disabled={summarizing || summaryPrompts.length === 0}
                      >
                        {summarizing ? (<><SpinnerIcon /> Génération…</>) : '✨ Résumer'}
                      </button>
                    </div>

                    {summaryError && (
                      <div className="ingestion-status ingestion-status--warning" style={{ marginTop: 8 }}>
                        <span className="ingestion-status-dot ingestion-status-dot--amber" />
                        <span>{summaryError}</span>
                      </div>
                    )}

                    {summaries.length === 0 && !summarizing && (
                      <p className="audio-summary-empty">Choisissez un type de synthèse et cliquez sur « Résumer ». Vous pouvez générer plusieurs synthèses (ex : Synthèse, puis Analyse) : elles s'accumulent ci-dessous.</p>
                    )}

                    <div className="audio-summary-list">
                      {summaries.map(summary => (
                        <div key={summary.id} className="audio-summary-result">
                          <div className="audio-summary-result-header">
                            <span className="audio-summary-result-label">{summary.label}</span>
                            <div className="ingestion-cache-info">
                              <button
                                className="ingestion-refresh-btn"
                                onClick={() => handleExportSummaryDocx(summary)}
                                disabled={exportingSummaryId === summary.id}
                                title="Exporter cette synthèse en .docx"
                              >
                                {exportingSummaryId === summary.id ? <SpinnerIcon /> : <DownloadIcon />}
                              </button>
                              <button className="ingestion-refresh-btn" onClick={() => handleCopySummary(summary.text)} title="Copier">
                                <svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path d="M8 3a2 2 0 00-2 2v9a2 2 0 002 2h7a2 2 0 002-2V8.414A2 2 0 0016.414 7L13 3.586A2 2 0 0011.586 3H8z"/><path d="M4 7a2 2 0 00-2 2v9a2 2 0 002 2h7a2 2 0 002-2v-1H6a3 3 0 01-3-3V7H4z"/></svg>
                              </button>
                              <button className="ingestion-refresh-btn" onClick={() => removeSummary(summary.id)} title="Supprimer cette synthèse">✕</button>
                            </div>
                          </div>
                          <div className="audio-summary-markdown bubble-markdown">
                            <ReactMarkdown
                              remarkPlugins={[remarkGfm, remarkMath]}
                              rehypePlugins={[rehypeKatex]}
                              components={SUMMARY_MD_COMPONENTS}
                            >{normalizeHeadings(normalizeLatex(summary.text))}</ReactMarkdown>
                          </div>
                          <button className="btn-ghost audio-summary-insert-btn" onClick={() => handleInsertSummary(summary.text)}>
                            ↩ Insérer dans le message
                          </button>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>

              </div>
            </div>
          )}
        </div>

        <div className="modal-footer">
          <button className="btn-ghost" onClick={onClose}>Fermer</button>
          <button className="btn-primary" onClick={handleInsert} disabled={!result}>
            ↩ Insérer dans le message
          </button>
        </div>
      </div>
    </div>
  );
}
