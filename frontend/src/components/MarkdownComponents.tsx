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


import React, { useState, useEffect, useRef, memo } from 'react';
import type { Components } from 'react-markdown';
import type { Element } from 'hast';
import { API_BASE } from '../constants';
import { mergeEChartsOption, cleanEChartsCode } from '../utils/text';
import { sanitizeMermaid } from '../utils/mermaid-sanitizer';
import * as echarts from 'echarts';
import mermaid from 'mermaid';

// ── Mermaid initialization (once) ────────────────────────────────────────────
// ── Mermaid theme — synchronisé avec les variables CSS de l'app ─────────────
// Light  : bg-primary=#ffffff  text-primary=#111111  border-medium=rgba(0,0,0,0.16)
// Dark   : bg-primary=#1a1a1a  text-primary=#f0f0f0  border-medium=rgba(255,255,255,0.14)
const _darkMq = window.matchMedia('(prefers-color-scheme: dark)');
function _getMermaidThemeVars(dark: boolean) {
  return dark
    ? {
        // ── Dark ──────────────────────────────────────────────────────────
        background:              '#1a1a1a',
        primaryColor:            '#2e2e2e',   // bg-hover
        primaryTextColor:        '#f0f0f0',   // text-primary
        primaryBorderColor:      'rgba(255,255,255,0.14)', // border-medium
        secondaryColor:          '#242424',   // bg-secondary
        secondaryTextColor:      '#aaaaaa',   // text-secondary
        secondaryBorderColor:    'rgba(255,255,255,0.07)', // border-light
        tertiaryColor:           '#111111',   // bg-tertiary
        tertiaryTextColor:       '#aaaaaa',
        tertiaryBorderColor:     'rgba(255,255,255,0.07)',
        lineColor:               '#aaaaaa',   // text-secondary
        edgeLabelBackground:     '#242424',
        mainBkg:                 '#2e2e2e',
        nodeBorder:              'rgba(255,255,255,0.14)',
        clusterBkg:              '#242424',
        clusterBorder:           'rgba(255,255,255,0.07)',
        titleColor:              '#f0f0f0',
        fontSize:                '13px',
        // Sequence diagrams
        actorBkg:                '#2e2e2e',
        actorBorder:             'rgba(255,255,255,0.14)',
        actorTextColor:          '#f0f0f0',
        actorLineColor:          '#aaaaaa',
        signalColor:             '#aaaaaa',
        signalTextColor:         '#f0f0f0',
        labelBoxBkgColor:        '#242424',
        labelBoxBorderColor:     'rgba(255,255,255,0.07)',
        labelTextColor:          '#aaaaaa',
        loopTextColor:           '#aaaaaa',
        noteBkgColor:            '#2e2e2e',
        noteBorderColor:         'rgba(255,255,255,0.14)',
        noteTextColor:           '#aaaaaa',
        activationBorderColor:   'rgba(255,255,255,0.14)',
        activationBkgColor:      '#242424',
        // Gantt
        gridColor:               'rgba(255,255,255,0.07)',
        section0:                '#2e2e2e',
        section1:                '#242424',
        section2:                '#1a1a1a',
        section3:                '#111111',
        taskBorderColor:         'rgba(255,255,255,0.14)',
        taskBkgColor:            '#2e2e2e',
        taskTextColor:           '#f0f0f0',
        taskTextOutsideColor:    '#aaaaaa',
        todayLineColor:          '#dddddd',  // green-dark (dark)
      }
    : {
        // ── Light ─────────────────────────────────────────────────────────
        background:              '#ffffff',
        primaryColor:            '#f0f0f0',   // bg-hover / green-light
        primaryTextColor:        '#111111',   // text-primary
        primaryBorderColor:      'rgba(0,0,0,0.16)', // border-medium
        secondaryColor:          '#f4f4f4',   // bg-secondary
        secondaryTextColor:      '#555555',   // text-secondary
        secondaryBorderColor:    'rgba(0,0,0,0.08)', // border-light
        tertiaryColor:           '#e8e8e8',   // bg-tertiary
        tertiaryTextColor:       '#555555',
        tertiaryBorderColor:     'rgba(0,0,0,0.08)',
        lineColor:               '#555555',   // text-secondary
        edgeLabelBackground:     '#ffffff',
        mainBkg:                 '#f0f0f0',
        nodeBorder:              'rgba(0,0,0,0.16)',
        clusterBkg:              '#f4f4f4',
        clusterBorder:           'rgba(0,0,0,0.08)',
        titleColor:              '#111111',
        fontSize:                '13px',
        // Sequence diagrams
        actorBkg:                '#f0f0f0',
        actorBorder:             'rgba(0,0,0,0.16)',
        actorTextColor:          '#111111',
        actorLineColor:          '#555555',
        signalColor:             '#555555',
        signalTextColor:         '#111111',
        labelBoxBkgColor:        '#f4f4f4',
        labelBoxBorderColor:     'rgba(0,0,0,0.08)',
        labelTextColor:          '#555555',
        loopTextColor:           '#555555',
        noteBkgColor:            '#f4f4f4',
        noteBorderColor:         'rgba(0,0,0,0.16)',
        noteTextColor:           '#555555',
        activationBorderColor:   'rgba(0,0,0,0.16)',
        activationBkgColor:      '#e8e8e8',
        // Gantt
        gridColor:               'rgba(0,0,0,0.08)',
        section0:                '#f0f0f0',
        section1:                '#e8e8e8',
        section2:                '#f4f4f4',
        section3:                '#ffffff',
        taskBorderColor:         'rgba(0,0,0,0.16)',
        taskBkgColor:            '#f0f0f0',
        taskTextColor:           '#111111',
        taskTextOutsideColor:    '#555555',
        todayLineColor:          '#111111',  // text-primary
      };
}
mermaid.initialize({
  startOnLoad: false,
  theme: 'base',
  securityLevel: 'loose',
  suppressErrorRendering: true,
  themeVariables: _getMermaidThemeVars(_darkMq.matches),
});

// Re-init Mermaid whenever the OS color-scheme flips
_darkMq.addEventListener('change', (e) => {
  mermaid.initialize({
    startOnLoad: false,
    theme: 'base',
    securityLevel: 'loose',
    suppressErrorRendering: true,
    themeVariables: _getMermaidThemeVars(e.matches),
  });
});

let _mermaidRenderCounter = 0;

function getMermaidSandbox(): HTMLElement {
  let el = document.getElementById('__mermaid_sandbox__');
  if (!el) {
    el = document.createElement('div');
    el.id = '__mermaid_sandbox__';
    el.style.cssText = 'position:fixed;top:-9999px;left:-9999px;width:1px;height:1px;overflow:hidden;visibility:hidden;';
    document.body.appendChild(el);
  }
  return el;
}

// ── Image proxy wikimedia ──────────────────────────────────────────────────
async function resolveImageUrl(rawSrc: string): Promise<string | null> {
  if (!rawSrc) return null;
  const wikiFileMatch = rawSrc.match(/commons\.wikimedia\.org\/wiki\/File:(.+)/i);
  if (wikiFileMatch) {
    const filename = decodeURIComponent(wikiFileMatch[1]);
    try {
      const apiUrl = `https://commons.wikimedia.org/w/api.php?action=query&titles=File:${encodeURIComponent(filename)}&prop=imageinfo&iiprop=url&format=json&origin=*`;
      const res = await fetch(apiUrl);
      const data = await res.json();
      const pages: Record<string, { imageinfo?: Array<{ url: string }> }> = data?.query?.pages || {};
      for (const page of Object.values(pages)) {
        const url = page?.imageinfo?.[0]?.url;
        if (url) return url;
      }
    } catch { /* ignore */ }
    return rawSrc;
  }
  return rawSrc;
}

interface MdImageProps { src?: string; alt?: string; }

export function MdImage({ src: rawSrc, alt }: MdImageProps) {
  const [resolvedSrc, setResolvedSrc] = useState<string | null>(null);
  const [errored, setErrored]         = useState(false);
  const [loading, setLoading]         = useState(true);

  useEffect(() => {
    if (!rawSrc) return;
    setErrored(false);
    setLoading(true);

    // Les data-URLs (base64 inline) sont auto-contenues — pas de proxy
    if (rawSrc.startsWith('data:')) {
      setResolvedSrc(rawSrc);
      setLoading(false);
      return;
    }

    resolveImageUrl(rawSrc).then(url => {
      const proxied = url ? `${API_BASE}/api-proxy/api/image-proxy?url=${encodeURIComponent(url)}` : null;
      setResolvedSrc(proxied);
      setLoading(false);
    });
  }, [rawSrc]);

  if (loading) return <span className="md-img-wrap md-img-loading">⏳ Chargement de l'image…</span>;
  if (errored || !resolvedSrc) {
    return (
      <a className="md-img-fallback md-link" href={rawSrc} target="_blank" rel="noopener noreferrer">
        🖼 {alt || rawSrc}
      </a>
    );
  }
  return (
    <span className="md-img-wrap">
      <img className="md-img" src={resolvedSrc} alt={alt || ''} onError={() => setErrored(true)} loading="lazy" />
      {alt && <span className="md-img-caption">{alt}</span>}
    </span>
  );
}


// ── ECharts ─────────────────────────────────────────────────────────────
interface EChartsBlockProps {
  code: string;
  streaming?: boolean;
  compact?: boolean;
  getImageRef?: React.MutableRefObject<(() => string) | null>;
}

export const EChartsBlock = memo(function EChartsBlock({ code, streaming, compact, getImageRef }: EChartsBlockProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const chartRef     = useRef<echarts.ECharts | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (streaming || !containerRef.current) return;
    setError(null);
    let option: unknown;
    let cleaned = '';
    try {
      cleaned = cleanEChartsCode(code);
      // eslint-disable-next-line no-new-func
      option = (new Function(`return (${cleaned})`))();
    } catch (e) {
      setError('JSON invalide : ' + (e as Error).message + '\n\n--- Source nettoyée ---\n' + cleaned);
      return;
    }
    if (chartRef.current) chartRef.current.dispose();
    const chart = echarts.init(containerRef.current, null, { renderer: 'canvas' });
    chartRef.current = chart;
    chart.setOption(mergeEChartsOption(option as Record<string, unknown>));
    if (getImageRef) getImageRef.current = () => chart.getDataURL({ type: 'png', pixelRatio: 2, backgroundColor: '#ffffff' });
    const ro = new ResizeObserver(() => chart.resize());
    ro.observe(containerRef.current!);
    return () => {
      ro.disconnect(); chart.dispose(); chartRef.current = null;
      if (getImageRef) getImageRef.current = null;
    };
  }, [code, streaming]);

  if (streaming) return (
    <div className="echarts-wrapper echarts-loading">
      <span className="echarts-spinner" /><span>Génération du graphique…</span>
    </div>
  );
  if (error) return <div className="echarts-error"><span>⚠ Erreur ECharts</span><pre>{error}</pre></div>;
  return <div className="echarts-wrapper" style={compact ? { height: 320 } : {}}><div ref={containerRef} className="echarts-canvas" /></div>;
});

// ── Mermaid  ─────────────────────────────────────────────────────────────
interface MermaidBlockProps { code: string; streaming?: boolean; }

export const MermaidBlock = memo(function MermaidBlock({ code, streaming }: MermaidBlockProps) {
  const [svg, setSvg]         = useState<string | null>(null);
  const [error, setError]     = useState<boolean>(false);
  const [rawCode, setRawCode] = useState<string | null>(null);
  const activeRender          = useRef(0);

  useEffect(() => {
    if (streaming || !code) return;
    const thisRender = ++activeRender.current;
    setSvg(null); setError(false);

    const sanitized = sanitizeMermaid(code);
    setRawCode(sanitized);

    const id = 'mr' + (++_mermaidRenderCounter);
    const sandbox = getMermaidSandbox();

    mermaid.render(id, sanitized, sandbox)
      .then(({ svg: rendered }) => { if (activeRender.current === thisRender) setSvg(rendered); })
      .catch((err: unknown) => {
        if (activeRender.current === thisRender) {
          console.warn('[Mermaid] render failed:', err, '\ncode:\n', sanitized);
          setError(true);
        }
      })
      .finally(() => {
        const el = document.getElementById(id);
        if (el && sandbox.contains(el)) el.remove();
      });

    return () => { activeRender.current++; };
  }, [code, streaming]);

  if (streaming) return (
    <div className="echarts-wrapper echarts-loading">
      <span className="echarts-spinner" /><span>Génération du diagramme…</span>
    </div>
  );
  if (error) return (
    <div className="mermaid-error">
      <span>⚠</span> Diagramme invalide — syntaxe Mermaid incorrecte
      {rawCode && (
        <details style={{ marginTop: 8 }}>
          <summary style={{ cursor: 'pointer', fontSize: '0.8em', opacity: 0.7 }}>Voir le code source</summary>
          <pre style={{ fontSize: '0.75em', marginTop: 4, whiteSpace: 'pre-wrap', wordBreak: 'break-all' }}>{rawCode}</pre>
        </details>
      )}
    </div>
  );
  if (!svg) return <div className="echarts-wrapper echarts-loading"><span className="echarts-spinner" /></div>;
  return <div className="mermaid-wrapper" dangerouslySetInnerHTML={{ __html: svg }} />;
});

// ── WordChatCard — placeholder inline dans le chat ────────────────────────────
interface WordChatCardProps { raw: string; }

export function WordChatCard({ raw }: WordChatCardProps) {
  const firstNewline = raw.indexOf('\n');
  const firstLine    = firstNewline !== -1 ? raw.slice(0, firstNewline).trim() : '';
  const bodyContent  = firstNewline !== -1 ? raw.slice(firstNewline + 1).trim() : raw.trim();
  const h1Match      = bodyContent.split('\n')[0].match(/^#\s+(.+)/);
  const label        = (firstLine || (h1Match && h1Match[1]) || 'Document').slice(0, 50);
  return (
    <div className="word-block-placeholder">
      <span className="word-block-placeholder__icon">📄</span>
      <span className="word-block-placeholder__label">{label}</span>
      <span className="word-block-placeholder__hint">⬇ disponible dans le panneau Artefacts</span>
    </div>
  );
}

// ── CodeBlock ──────────────────────────────────────────────────────
interface CodeBlockProps {
  className?: string;
  children?: React.ReactNode;
  node?: Element;
  streaming?: boolean;
}

// ── ScreenshotBlock — affiche un screenshot base64 passé dans un bloc de code ─
interface ScreenshotBlockProps { raw: string; }

function ScreenshotBlock({ raw }: ScreenshotBlockProps) {
  // Format attendu (première ligne = métadonnées JSON optionnelles, reste = base64)
  let mime = 'image/jpeg';
  let alt  = 'Screenshot';
  let b64  = raw.trim();

  const newline = raw.indexOf('\n');
  if (newline !== -1) {
    const firstLine = raw.slice(0, newline).trim();
    try {
      const meta = JSON.parse(firstLine) as { mime?: string; alt?: string };
      if (meta.mime) mime = meta.mime;
      if (meta.alt)  alt  = meta.alt;
      b64 = raw.slice(newline + 1).trim();
    } catch { /* première ligne pas du JSON → tout est base64 */ }
  }

  const src = `data:${mime};base64,${b64}`;
  return (
    <span className="md-img-wrap">
      <img className="md-img" src={src} alt={alt} style={{ maxWidth: '100%', borderRadius: 6 }} />
      <span className="md-img-caption">{alt}</span>
    </span>
  );
}

export function CodeBlock({ className, children, node, streaming }: CodeBlockProps) {
  const lang = (className || '').replace('language-', '');
  // Extract raw text from hast node or fallback to children string
  const raw = node?.children
    ?.filter((c): c is { type: 'text'; value: string } => c.type === 'text')
    .map(c => c.value)
    .join('') ?? String(children ?? '');
  const code = raw.replace(/\n$/, '');
  if (lang === 'echarts')     return <EChartsBlock code={code} streaming={streaming} />;
  if (lang === 'mermaid')     return <MermaidBlock code={code} streaming={streaming} />;
  if (lang === 'word')        return <WordChatCard raw={raw} />;
  if (lang === 'screenshot')  return <ScreenshotBlock raw={code} />;
  return <pre className="code-block"><code className={className}>{children}</code></pre>;
}

// ── Markdown ───────────────────────────────────────────
export function makeMdComponents(streaming: boolean): Components {
  const CodeBlockWrapper = (props: Omit<CodeBlockProps, 'streaming'>) =>
    <CodeBlock {...props} streaming={streaming} />;
  return {
    code: CodeBlockWrapper as Components['code'],
    img:        ({ src, alt }: { src?: string; alt?: string })                      => <MdImage src={src} alt={alt} />,
    h1:         ({ children }: { children?: React.ReactNode })                      => <h1 className="md-h1">{children}</h1>,
    h2:         ({ children }: { children?: React.ReactNode })                      => <h2 className="md-h2">{children}</h2>,
    h3:         ({ children }: { children?: React.ReactNode })                      => <h3 className="md-h3">{children}</h3>,
    h4:         ({ children }: { children?: React.ReactNode })                      => <h4 className="md-h4">{children}</h4>,
    p:          ({ children }: { children?: React.ReactNode })                      => <p className="md-p">{children}</p>,
    ul:         ({ children }: { children?: React.ReactNode })                      => <ul className="md-ul">{children}</ul>,
    ol:         ({ children }: { children?: React.ReactNode })                      => <ol className="md-ol">{children}</ol>,
    li:         ({ children }: { children?: React.ReactNode })                      => <li className="md-li">{children}</li>,
    strong:     ({ children }: { children?: React.ReactNode })                      => <strong className="md-strong">{children}</strong>,
    em:         ({ children }: { children?: React.ReactNode })                      => <em className="md-em">{children}</em>,
    blockquote: ({ children }: { children?: React.ReactNode })                      => <blockquote className="md-blockquote">{children}</blockquote>,
    hr:         ()                                                                   => <hr className="md-hr" />,
    a:          ({ href, children }: { href?: string; children?: React.ReactNode }) => <a className="md-link" href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
    table:      ({ children }: { children?: React.ReactNode })                      => <div className="md-table-wrap"><table className="md-table">{children}</table></div>,
    thead:      ({ children }: { children?: React.ReactNode })                      => <thead className="md-thead">{children}</thead>,
    tbody:      ({ children }: { children?: React.ReactNode })                      => <tbody>{children}</tbody>,
    tr:         ({ children }: { children?: React.ReactNode })                      => <tr className="md-tr">{children}</tr>,
    th:         ({ children }: { children?: React.ReactNode })                      => <th className="md-th">{children}</th>,
    td:         ({ children }: { children?: React.ReactNode })                      => <td className="md-td">{children}</td>,
  } as Components;
}

export const ARTIFACT_MD_COMPONENTS = makeMdComponents(false);
