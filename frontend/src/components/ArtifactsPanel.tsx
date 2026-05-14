import React, { useState, useRef, useEffect } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { EChartsBlock, MermaidBlock, ARTIFACT_MD_COMPONENTS, makeMdComponents } from './MarkdownComponents';
import { CopyIcon, CheckIcon, DownloadIcon, SpinnerIcon } from './Icons';
import { markdownToWordHtml, copyHtmlToClipboard, normalizeHeadings, normalizeLatex } from '../utils/text';
import { generateDocx } from '../utils/docx';
import { useDialog } from '../DialogContext';
import type { Artifact } from '../utils/artifacts';

const WORD_MD_COMPONENTS = makeMdComponents(false);

function WordDocBlock({ content, containerRef }: { content: string; containerRef: React.RefObject<HTMLDivElement> }) {
  return (
    <div className="artifact-word-preview bubble-markdown" ref={containerRef}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={WORD_MD_COMPONENTS}
      >{normalizeHeadings(normalizeLatex(content))}</ReactMarkdown>
    </div>
  );
}

interface ArtifactsPanelProps {
  artifacts: Artifact[];
  onClose: () => void;
}

export function ArtifactsPanel({ artifacts, onClose }: ArtifactsPanelProps) {
  const [activeId, setActiveId]       = useState(artifacts[0]?.id);
  const [listOpen, setListOpen]       = useState(false);
  const [copied, setCopied]           = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [width, setWidth]             = useState(440);
  const getImageRef    = useRef<(() => string) | null>(null);
  const wordPreviewRef = useRef<HTMLDivElement>(null);
  const { toast }      = useDialog();

  const active = artifacts.find(a => a.id === activeId) || artifacts[0];

  useEffect(() => {
    const stillExists = artifacts.some(a => a.id === activeId);
    if (!stillExists) setActiveId(artifacts[0]?.id);
  }, [artifacts, activeId]);

  /* ── Copy ── */
  const handleCopy = async () => {
    if (!active) return;
    let ok = false;
    if (active.type === 'chart' && getImageRef.current) {
      try {
        const dataUrl  = getImageRef.current();
        const base64   = dataUrl.split(',')[1];
        const bytes    = Uint8Array.from(atob(base64), c => c.charCodeAt(0));
        const imgBlob  = new Blob([bytes], { type: 'image/png' });
        const imgHtml  = `<img src="${dataUrl}" style="max-width:100%">`;
        const htmlBlob = new Blob([imgHtml], { type: 'text/html' });
        await navigator.clipboard.write([new ClipboardItem({ 'image/png': imgBlob, 'text/html': htmlBlob })]);
        ok = true;
      } catch {
        try { await navigator.clipboard.writeText(active.content); ok = true; } catch { /* ignore */ }
      }
    } else {
      const html = markdownToWordHtml(active.content);
      ok = await copyHtmlToClipboard(html, active.content);
    }
    if (ok) { setCopied(true); setTimeout(() => setCopied(false), 2000); }
  };

  /* ── Download Word ── */
  const handleDownloadWord = async () => {
    if (!active || active.type !== 'word') return;
    setDownloading(true);
    try {
      const allWordArtifacts = artifacts.filter(a => a.type === 'word');
      const mergedContent    = allWordArtifacts.map(a => a.content).join('\n\n');
      const label            = allWordArtifacts[0]?.label || active.label;
      const slug = label.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '') || 'document';
      await generateDocx(mergedContent, `${slug}.docx`);
      toast(`"${label}.docx" téléchargé`, 'success');
    } catch (e) {
      console.error('Erreur génération docx:', e);
      toast('Erreur lors de la génération : ' + (e as Error).message, 'error');
    } finally {
      setDownloading(false);
    }
  };

  /* ── Resize drag ── */
  const isDragging = useRef(false);
  const startX     = useRef(0);
  const startW     = useRef(0);

  const onResizeMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    isDragging.current = true;
    startX.current = e.clientX;
    startW.current = width;
    document.body.style.cursor    = 'col-resize';
    document.body.style.userSelect = 'none';
  };

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!isDragging.current) return;
      const delta = startX.current - e.clientX;
      setWidth(Math.min(900, Math.max(280, startW.current + delta)));
    };
    const onUp = () => {
      isDragging.current = false;
      document.body.style.cursor    = '';
      document.body.style.userSelect = '';
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => { window.removeEventListener('mousemove', onMove); window.removeEventListener('mouseup', onUp); };
  }, []);

  if (!active) return null;

  return (
    <aside className="artifacts-panel" style={{ width }}>
      <div className="artifacts-resize-handle" onMouseDown={onResizeMouseDown} title="Glisser pour redimensionner" />

      <div className="artifacts-header">
        <div className="artifacts-header-left">
          <span className="artifacts-title">✦ Artéfacts</span>
          {artifacts.length > 1 && (
            <div className="artifacts-pill-wrap">
              <button
                className={`artifacts-pill ${listOpen ? 'artifacts-pill--open' : ''}`}
                onClick={() => setListOpen(o => !o)}
              >
                <span className="artifacts-pill-icon">{active.icon}</span>
                <span className="artifacts-pill-label">{active.label}</span>
                <span className="artifacts-pill-count">{artifacts.length}</span>
                <span className="artifacts-pill-chevron">{listOpen ? '▲' : '▼'}</span>
              </button>
              {listOpen && (
                <div className="artifacts-dropdown">
                  {artifacts.map(a => (
                    <button
                      key={a.id}
                      className={`artifacts-dropdown-item ${a.id === active.id ? 'artifacts-dropdown-item--active' : ''}`}
                      onClick={() => { setActiveId(a.id); setListOpen(false); }}
                    >
                      <span>{a.icon}</span>
                      <span>{a.label}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
        <div className="artifacts-header-actions">
          {active.type === 'word' && (
            <button
              className={`artifact-download-btn ${downloading ? 'artifact-download-btn--busy' : ''}`}
              onClick={handleDownloadWord}
              disabled={downloading}
              title="Télécharger en .docx"
            >
              {downloading ? <><SpinnerIcon /> Génération…</> : <><DownloadIcon /> Télécharger .docx</>}
            </button>
          )}
          <button className={`artifact-copy-btn ${copied ? 'artifact-copy-btn--done' : ''}`} onClick={handleCopy}>
            {copied
              ? <><CheckIcon /> Copié !</>
              : active.type === 'chart'
                ? <><CopyIcon /> Copier l'image</>
                : <><CopyIcon /> Copier le texte</>
            }
          </button>
          <button className="artifacts-close" onClick={onClose}>✕</button>
        </div>
      </div>

      <div className="artifacts-content" key={active.id}>
        {active.type === 'chart' ? (
          <div className="artifact-chart-wrap">
            <EChartsBlock code={active.content} streaming={false} compact getImageRef={getImageRef} />
          </div>
        ) : active.type === 'mermaid' ? (
          <div className="artifact-chart-wrap">
            <MermaidBlock code={active.content} streaming={false} />
          </div>
        ) : active.type === 'word' ? (
          <WordDocBlock content={active.content} containerRef={wordPreviewRef} />
        ) : (
          <div className="artifact-markdown bubble-markdown">
            <ReactMarkdown
              remarkPlugins={[remarkGfm, remarkMath]}
              rehypePlugins={[rehypeKatex]}
              components={ARTIFACT_MD_COMPONENTS}
            >{normalizeHeadings(normalizeLatex(active.content))}</ReactMarkdown>
          </div>
        )}
      </div>
    </aside>
  );
}
