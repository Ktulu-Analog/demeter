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


import React, { useState, useRef, useEffect, memo, useMemo } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';
import { EChartsBlock, MermaidBlock, SvgBlock, ARTIFACT_MD_COMPONENTS, makeMdComponents } from './MarkdownComponents';
import { CopyIcon, CheckIcon, DownloadIcon, SpinnerIcon } from './Icons';
import { markdownToWordHtml, copyHtmlToClipboard, normalizeHeadings, normalizeLatex } from '../utils/text';
import { generateDocx } from '../utils/docx';
import { useDialog } from '../DialogContext';
import type { Artifact } from '../utils/artifacts';

const WORD_MD_COMPONENTS = makeMdComponents(false);

// Mémoïsé : ne se re-rend que si le contenu du document change,
// pas à chaque frappe dans le ChatInput.
const WordDocBlock = memo(function WordDocBlock({ content, containerRef }: { content: string; containerRef: React.RefObject<HTMLDivElement> }) {
  return (
    <div className="artifact-word-preview bubble-markdown" ref={containerRef}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={WORD_MD_COMPONENTS}
      >{normalizeHeadings(normalizeLatex(content))}</ReactMarkdown>
    </div>
  );
});

// Mémoïsé : couvre les artefacts "réponse complète" et "tableau",
// qui peuvent contenir des blocs ECharts/Mermaid coûteux à re-rendre.
const FullContentBlock = memo(function FullContentBlock({ content }: { content: string }) {
  return (
    <div className="artifact-markdown bubble-markdown">
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[rehypeKatex]}
        components={ARTIFACT_MD_COMPONENTS}
      >{normalizeHeadings(normalizeLatex(content))}</ReactMarkdown>
    </div>
  );
});

// Mémoïsé : le graphique ECharts ne se re-rend pas sur frappe dans le ChatInput.
const EChartsArtifact = memo(function EChartsArtifact({
  content, getImageRef,
}: { content: string; getImageRef: React.MutableRefObject<(() => string) | null> }) {
  return (
    <div className="artifact-chart-wrap">
      <EChartsBlock code={content} streaming={false} compact getImageRef={getImageRef} />
    </div>
  );
});

// Mémoïsé : le diagramme Mermaid ne se re-rend pas sur frappe dans le ChatInput.
const MermaidArtifact = memo(function MermaidArtifact(
  { content, svgRef }: { content: string; svgRef: React.MutableRefObject<string | null> }
) {
  return (
    <div className="artifact-chart-wrap">
      <MermaidBlock code={content} streaming={false} svgRef={svgRef} />
    </div>
  );
});

// Mémoïsé : le schéma SVG ne se re-rend pas sur frappe dans le ChatInput.
const SvgArtifact = memo(function SvgArtifact({ content }: { content: string }) {
  return (
    <div className="artifact-chart-wrap artifact-svg-wrap">
      <SvgBlock code={content} streaming={false} />
    </div>
  );
});

interface ArtifactsPanelProps {
  artifacts: Artifact[];
  onClose: () => void;
}

// Mémoïsé : ne se re-rend que si artifacts ou onClose changent,
// pas à chaque frappe dans le ChatInput.
export const ArtifactsPanel = memo(function ArtifactsPanel({ artifacts, onClose }: ArtifactsPanelProps) {
  const [activeId, setActiveId]       = useState(artifacts[0]?.id);
  const [listOpen, setListOpen]       = useState(false);
  const [copied, setCopied]           = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [width, setWidth]             = useState(440);
  const getImageRef    = useRef<(() => string) | null>(null);
  const mermaidSvgRef  = useRef<string | null>(null);
  const wordPreviewRef = useRef<HTMLDivElement>(null);
  const { toast }      = useDialog();

  const active = useMemo(
    () => artifacts.find(a => a.id === activeId) || artifacts[0],
    [artifacts, activeId],
  );

  useEffect(() => {
    const stillExists = artifacts.some(a => a.id === activeId);
    if (!stillExists) setActiveId(artifacts[0]?.id);
  }, [artifacts, activeId]);

  /* ── Copie ── */
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

  /* ── Télécharger Word ── */
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

  /* ── Export SVG Mermaid ── */
  const handleDownloadMermaidSvg = () => {
    const svgStr = mermaidSvgRef.current;
    if (!svgStr) return;
    const blob = new Blob([svgStr], { type: 'image/svg+xml' });
    const url  = URL.createObjectURL(blob);
    const a    = document.createElement('a');
    a.href     = url;
    a.download = (active?.label || 'diagramme').toLowerCase().replace(/[^a-z0-9]+/g, '-') + '.svg';
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleDownloadMermaidPng = () => {
    // Récupérer le SVG déjà rendu dans le DOM — il contient les styles Mermaid inline
    const domSvg = document.querySelector<SVGSVGElement>('.artifact-chart-wrap .mermaid-wrapper svg');
    if (!domSvg) return;

    const scale  = 2;
    const W      = domSvg.getBoundingClientRect().width  || domSvg.viewBox.baseVal.width  || 1200;
    const H      = domSvg.getBoundingClientRect().height || domSvg.viewBox.baseVal.height || 800;

    // Cloner le SVG et forcer width/height absolus + fond blanc
    const clone  = domSvg.cloneNode(true) as SVGSVGElement;
    clone.setAttribute('width',  String(W * scale));
    clone.setAttribute('height', String(H * scale));
    // Fond blanc explicite
    const bg = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
    bg.setAttribute('width', '100%');
    bg.setAttribute('height', '100%');
    bg.setAttribute('fill', '#ffffff');
    clone.insertBefore(bg, clone.firstChild);

    // Sérialiser en string avec XMLSerializer (préserve les styles inline Mermaid)
    const serializer = new XMLSerializer();
    const svgStr     = serializer.serializeToString(clone);
    const svgBlob    = new Blob([svgStr], { type: 'image/svg+xml;charset=utf-8' });
    const url        = URL.createObjectURL(svgBlob);

    const canvas     = document.createElement('canvas');
    canvas.width     = W * scale;
    canvas.height    = H * scale;
    const ctx        = canvas.getContext('2d')!;
    ctx.fillStyle    = '#ffffff';
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    const img        = new Image();
    img.onload = () => {
      ctx.drawImage(img, 0, 0);
      URL.revokeObjectURL(url);
      canvas.toBlob(pngBlob => {
        if (!pngBlob) return;
        const a    = document.createElement('a');
        a.href     = URL.createObjectURL(pngBlob);
        a.download = (active?.label || 'diagramme').toLowerCase().replace(/[^a-z0-9]+/g, '-') + '.png';
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
      }, 'image/png');
    };
    img.onerror = (e) => {
      console.error('[PNG export] échec chargement SVG dans Image:', e);
      URL.revokeObjectURL(url);
    };
    img.src = url;
  };

  /* ── Redimensionnement ── */
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
          {active.type === 'mermaid' && (
            <>
              <button
                className="artifact-download-btn"
                onClick={handleDownloadMermaidSvg}
                title="Télécharger en .svg"
              >
                <DownloadIcon /> .svg
              </button>
              <button
                className="artifact-download-btn"
                onClick={handleDownloadMermaidPng}
                title="Télécharger en .png"
              >
                <DownloadIcon /> .png
              </button>
            </>
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

      <div className="artifacts-content">
        {active.type === 'chart' ? (
          <EChartsArtifact content={active.content} getImageRef={getImageRef} />
        ) : active.type === 'mermaid' ? (
          <MermaidArtifact content={active.content} svgRef={mermaidSvgRef} />
        ) : active.type === 'excalidraw' || active.type === 'svg' ? (
          <SvgArtifact content={active.content} />
        ) : active.type === 'word' ? (
          <WordDocBlock content={active.content} containerRef={wordPreviewRef} />
        ) : (
          <FullContentBlock content={active.content} />
        )}
      </div>
    </aside>
  );
});
