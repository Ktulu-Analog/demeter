import type { EChartsOption } from 'echarts';

/**
 * Normalise les balises LaTeX et HTML brutes en Markdown.
 */
export function normalizeLatex(text: string): string {
  if (!text) return text;
  text = text.replace(/<br\s*\/?>/gi, '\n');
  text = text.replace(/<b>(.*?)<\/b>/gi, '**$1**');
  text = text.replace(/<strong>(.*?)<\/strong>/gi, '**$1**');
  text = text.replace(/<i>(.*?)<\/i>/gi, '*$1*');
  text = text.replace(/<em>(.*?)<\/em>/gi, '*$1*');
  text = text.replace(/<code>(.*?)<\/code>/gi, '`$1`');
  text = text.replace(/\\\[([^]*?)\\\]/g, (_, m: string) => `$$${m}$$`);
  text = text.replace(/\\\(([^]*?)\\\)/g, (_, m: string) => `$${m}$`);
  text = text.replace(/^\s*\[([^\]]*\\[^\]]+)\]\s*$/gm, (_, m: string) => `$$${m}$$`);
  return text;
}

/**
 * Supprime les emoji-numéros des titres (1️⃣ 2️⃣ etc.)
 */
export function normalizeHeadings(text: string): string {
  if (!text) return text;
  return text.replace(/^(#{1,6}\s*)([0-9]️⃣|🔟|[①②③④⑤⑥⑦⑧⑨⑩]|[❶-❿])?\s*/gm, '$1');
}

/**
 * Nettoie le code ECharts généré par le LLM pour le rendre évaluable via new Function().
 */
export function cleanEChartsCode(src: string): string {
  src = (src || '').trim();
  let out = '';
  let i = 0;
  while (i < src.length) {
    if (src[i] === '"') {
      out += '"'; i++;
      while (i < src.length) {
        if (src[i] === '\\') { out += src[i] + (src[i + 1] || ''); i += 2; continue; }
        if (src[i] === '"')  { out += '"'; i++; break; }
        if (src[i] === '\n') { out += '\\n'; i++; continue; }
        if (src[i] === '\r') { i++; continue; }
        out += src[i++];
      }
      continue;
    }
    if (src[i] === "'") {
      out += '"'; i++;
      while (i < src.length) {
        if (src[i] === '\\') { out += src[i] + (src[i + 1] || ''); i += 2; continue; }
        if (src[i] === '"')  { out += '\\"'; i++; continue; }
        if (src[i] === '\n') { out += '\\n'; i++; continue; }
        if (src[i] === '\r') { i++; continue; }
        if (src[i] === "'")  { out += '"'; i++; break; }
        out += src[i++];
      }
      continue;
    }
    if (src[i] === '`') {
      out += '`'; i++;
      let depth = 0;
      while (i < src.length) {
        if (src[i] === '\\') { out += src[i] + (src[i + 1] || ''); i += 2; continue; }
        if (src[i] === '$' && src[i + 1] === '{') { out += '${'; i += 2; depth++; continue; }
        if (depth > 0 && src[i] === '}') { out += '}'; i++; depth--; continue; }
        if (depth === 0 && src[i] === '`') { out += '`'; i++; break; }
        out += src[i++];
      }
      continue;
    }
    if (src[i] === '/' && src[i + 1] === '/') { while (i < src.length && src[i] !== '\n') i++; continue; }
    if (src[i] === '/' && src[i + 1] === '*') {
      i += 2;
      while (i < src.length && !(src[i] === '*' && src[i + 1] === '/')) i++;
      i += 2;
      continue;
    }
    out += src[i++];
  }
  out = out.replace(/(?<=[[\],]\s*)\?(?=\s*[,\]])/g, 'null');
  return out.replace(/,(\s*[}\]])/g, '$1');
}

/**
 * Convertit du Markdown en HTML inline (pour copie Word).
 */
export function markdownToWordHtml(md: string): string {
  if (!md) return '';

  const TEXT_PRIMARY   = '#111111';
  const TEXT_SECONDARY = '#555555';
  const BG_SECONDARY   = '#f4f4f4';

  let text = md.replace(/^(#{1,6}\s*)([0-9]️⃣|🔟|[①-⑩]|[❶-❿])?\s*/gm, '$1');

  // Fenced code blocks
  text = text.replace(/```[\w]*\n([\s\S]*?)```/g,
    `<pre style="background:#f4f4f4;border:1px solid rgba(0,0,0,0.1);border-radius:4px;padding:10px 14px;font-family:Consolas,monospace;font-size:10pt;margin:10px 0;overflow-x:auto">$1</pre>`);

  // Tables
  text = text.replace(/((?:\|.+\|\n)+)/g, (block) => {
    const rows = block.trim().split('\n');
    const dataRows = rows.filter(r => !/^\|[-:\s|]+\|$/.test(r.trim()));
    if (dataRows.length < 1) return block;
    const [head, ...body] = dataRows;
    const thStyle = `border-bottom:2px solid rgba(0,0,0,0.2);border-right:1px solid rgba(0,0,0,0.08);padding:8px 12px;text-align:left;font-weight:600;font-size:10pt;color:${TEXT_PRIMARY};background:${BG_SECONDARY};text-transform:uppercase;letter-spacing:0.3px;white-space:nowrap`;
    const tdStyle = (even: boolean) => `border-bottom:1px solid rgba(0,0,0,0.07);border-right:1px solid rgba(0,0,0,0.07);padding:8px 12px;text-align:left;font-size:10.5pt;vertical-align:top;${even ? `background:${BG_SECONDARY}` : ''}`;
    const toTh = (row: string) => '<tr>' + row.split('|').filter((_, i, a) => i > 0 && i < a.length - 1).map(c => `<th style="${thStyle}">${c.trim()}</th>`).join('') + '</tr>';
    const toTd = (row: string, even: boolean) => '<tr>' + row.split('|').filter((_, i, a) => i > 0 && i < a.length - 1).map(c => `<td style="${tdStyle(even)}">${c.trim()}</td>`).join('') + '</tr>';
    return `<table style="border-collapse:collapse;width:100%;font-family:Calibri,sans-serif;margin:14px 0;border:1px solid rgba(0,0,0,0.16);border-radius:4px"><thead style="background:${BG_SECONDARY}">${toTh(head)}</thead><tbody>${body.map((r, i) => toTd(r, i % 2 === 1)).join('')}</tbody></table>`;
  });

  // Headings
  text = text
    .replace(/^#### (.+)$/gm, `<h4 style="font-family:'Sora',Calibri,sans-serif;font-size:11pt;font-weight:600;color:${TEXT_SECONDARY};margin:12px 0 4px">$1</h4>`)
    .replace(/^### (.+)$/gm,  `<h3 style="font-family:'Sora',Calibri,sans-serif;font-size:10.5pt;font-weight:600;color:${TEXT_SECONDARY};text-transform:uppercase;letter-spacing:0.6px;margin:16px 0 6px">$1</h3>`)
    .replace(/^## (.+)$/gm,   `<h2 style="font-family:'Sora',Calibri,sans-serif;font-size:12.5pt;font-weight:600;color:${TEXT_PRIMARY};background:${BG_SECONDARY};border-left:3px solid ${TEXT_PRIMARY};padding:5px 10px;margin:20px 0 8px;border-radius:0 4px 4px 0">$1</h2>`)
    .replace(/^# (.+)$/gm,    `<h1 style="font-family:'Sora',Calibri,sans-serif;font-size:14pt;font-weight:700;color:${TEXT_PRIMARY};border-bottom:1px solid rgba(0,0,0,0.2);padding-bottom:6px;margin:24px 0 10px">$1</h1>`);

  // Inline formatting
  text = text
    .replace(/\*\*(.+?)\*\*/g, `<strong style="font-weight:600">$1</strong>`)
    .replace(/\*(.+?)\*/g,     `<em style="color:${TEXT_SECONDARY}">$1</em>`)
    .replace(/`([^`]+)`/g,     `<code style="background:#ebebeb;border:1px solid rgba(0,0,0,0.1);border-radius:3px;padding:1px 5px;font-family:Consolas,monospace;font-size:9.5pt">$1</code>`);

  // Blockquotes
  text = text.replace(/^> (.+)$/gm,
    `<blockquote style="border-left:3px solid rgba(0,0,0,0.2);margin:10px 0;padding:8px 14px;background:${BG_SECONDARY};color:${TEXT_SECONDARY};font-style:italic;border-radius:0 4px 4px 0">$1</blockquote>`);

  // Lists
  text = text.replace(/^[*\-] (.+)$/gm, `<li style="margin-bottom:4px;padding-left:4px;line-height:1.65">$1</li>`);
  text = text.replace(/(<li[^>]*>.*<\/li>\n?)+/g, m => `<ul style="margin:6px 0 12px;padding-left:20px;list-style:disc">${m}</ul>`);
  text = text.replace(/^\d+\. (.+)$/gm, `<li style="margin-bottom:4px;line-height:1.65">$1</li>`);

  // HR
  text = text.replace(/^---+$/gm, `<hr style="border:none;border-top:1px solid rgba(0,0,0,0.12);margin:16px 0">`);

  // Paragraphs
  text = text
    .replace(/\n\n+/g, `</p><p style="margin:0 0 10px;line-height:1.75">`)
    .replace(/\n/g, '<br>');

  return `<div style="font-family:Calibri,'DM Sans',sans-serif;font-size:11pt;line-height:1.75;color:${TEXT_PRIMARY};max-width:800px"><p style="margin:0 0 10px;line-height:1.75">${text}</p></div>`;
}

export async function copyHtmlToClipboard(html: string, plainText: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.write) {
      const htmlBlob = new Blob([html], { type: 'text/html' });
      const textBlob = new Blob([plainText], { type: 'text/plain' });
      await navigator.clipboard.write([new ClipboardItem({ 'text/html': htmlBlob, 'text/plain': textBlob })]);
      return true;
    }
  } catch { /* fall through */ }
  try { await navigator.clipboard.writeText(plainText); return true; } catch { /* fall through */ }
  return false;
}

export function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 7);
}

export function slugify(str: string): string {
  return str
    .toLowerCase()
    .normalize('NFD').replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    || 'espace';
}

// ── ECharts theme defaults (shared between MarkdownComponents and docx) ──────
const CHART_BG      = '#ffffff';
const CHART_FG      = '#374151';
const CHART_SUBTLE  = '#9ca3af';
const CHART_BORDER  = '#e5e7eb';
const CHART_PALETTE = [
  '#7ec8e3', '#a8d5a2', '#f4a7b9', '#f7c59f', '#c3aed6',
  '#fde68a', '#a0c4d8', '#f9b8a0', '#b5e0d0', '#d4b5f0',
];

export const ECHARTS_DEFAULTS: EChartsOption = {
  backgroundColor: CHART_BG,
  textStyle: { fontFamily: 'inherit', color: CHART_FG, fontSize: 12 },
  color: CHART_PALETTE,
  grid: { containLabel: true, left: 16, right: 16, top: 40, bottom: 48, borderColor: CHART_BORDER },
  title: { textStyle: { color: CHART_FG, fontWeight: 600, fontSize: 14 }, subtextStyle: { color: CHART_SUBTLE } },
  legend: { orient: 'horizontal', bottom: 0, left: 'center', textStyle: { color: CHART_FG }, pageTextStyle: { color: CHART_SUBTLE }, inactiveColor: CHART_SUBTLE },
  tooltip: {
    backgroundColor: '#1f2937', borderColor: '#374151', textStyle: { color: '#f9fafb' },
    axisPointer: {
      lineStyle: { color: CHART_BORDER, type: 'dashed' },
      crossStyle: { color: CHART_SUBTLE },
      shadowStyle: { color: 'rgba(0,0,0,0.04)' },
    },
  },
  xAxis: {
    axisLine: { lineStyle: { color: CHART_BORDER } }, axisTick: { lineStyle: { color: CHART_BORDER } },
    axisLabel: { color: CHART_SUBTLE }, splitLine: { lineStyle: { color: CHART_BORDER, type: 'dashed' } },
  },
  yAxis: {
    axisLine: { lineStyle: { color: CHART_BORDER } }, axisTick: { lineStyle: { color: CHART_BORDER } },
    axisLabel: { color: CHART_SUBTLE }, splitLine: { lineStyle: { color: CHART_BORDER, type: 'dashed' } },
  },
};

/**
 * Fusionne les defaults ECharts avec l'option générée par le LLM,
 * en s'assurant que `legend.bottom` n'est jamais écrasé.
 */
export function mergeEChartsOption(option: EChartsOption): EChartsOption {
  const merged: EChartsOption = { ...ECHARTS_DEFAULTS, ...option };
  const baseLegend = ECHARTS_DEFAULTS.legend as Record<string, unknown>;
  const userLegend = (Array.isArray(option.legend) ? option.legend[0] : option.legend) as Record<string, unknown> | undefined;
  merged.legend = { ...baseLegend, ...userLegend };
  return merged;
}
