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


import * as echarts from 'echarts';
import {
  Document, Packer, Paragraph, TextRun, HeadingLevel,
  Table, TableRow, TableCell, WidthType, BorderStyle,
  AlignmentType, ShadingType, ImageRun,
} from 'docx';
import { cleanEChartsCode, mergeEChartsOption } from './text';

const COLOR_H1      = '1a1a2e';
const COLOR_H2      = '16213e';
const COLOR_H3      = '0f3460';
const COLOR_TEXT    = '2d2d2d';
const COLOR_SUBTLE  = '666666';
const COLOR_CODE    = '3b3b3b';
const COLOR_CODE_BG = 'f0f0f0';

function inlineRuns(text: string, baseColor?: string, size?: number, forceBold?: boolean): TextRun[] {
  const runs: TextRun[] = [];
  const sz   = size || 22;
  const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*)/);
  for (const part of parts) {
    if (!part) continue;
    if (part.startsWith('**') && part.endsWith('**')) {
      runs.push(new TextRun({ text: part.slice(2, -2), bold: true, color: baseColor || COLOR_TEXT, size: sz }));
    } else if (part.startsWith('*') && part.endsWith('*')) {
      runs.push(new TextRun({ text: part.slice(1, -1), italics: true, color: COLOR_SUBTLE, size: sz }));
    } else if (part.startsWith('`') && part.endsWith('`')) {
      runs.push(new TextRun({ text: part.slice(1, -1), font: 'Courier New', color: COLOR_CODE, size: Math.min(sz, 20), shading: { type: ShadingType.CLEAR, fill: COLOR_CODE_BG } }));
    } else {
      runs.push(new TextRun({ text: part, bold: forceBold || false, color: baseColor || COLOR_TEXT, size: sz }));
    }
  }
  return runs.length ? runs : [new TextRun({ text, bold: forceBold || false, color: baseColor || COLOR_TEXT, size: sz })];
}

async function renderEChartsToImage(codeContent: string): Promise<Uint8Array> {
  const wrapper = document.createElement('div');
  wrapper.style.cssText = 'position:fixed;left:-9999px;top:-9999px;width:900px;height:450px;opacity:0;pointer-events:none;';
  document.body.appendChild(wrapper);
  try {
    const cleaned = cleanEChartsCode(codeContent);
    // eslint-disable-next-line no-new-func
    const option  = (new Function(`return (${cleaned})`))();
    const chart   = echarts.init(wrapper, null, { renderer: 'canvas', width: 900, height: 450 });
    chart.setOption(mergeEChartsOption(option));
    await new Promise<void>(resolve => {
      let settled = false;
      const onFinish = () => { if (settled) return; settled = true; setTimeout(resolve, 200); };
      chart.on('finished', onFinish);
      setTimeout(() => { if (!settled) onFinish(); }, 500);
    });
    const dataUrl = chart.getDataURL({ type: 'png', pixelRatio: 2, backgroundColor: '#ffffff' });
    chart.dispose();
    const base64 = dataUrl.split(',')[1];
    const binary = atob(base64);
    const imgData = new Uint8Array(binary.length);
    for (let k = 0; k < binary.length; k++) imgData[k] = binary.charCodeAt(k);
    return imgData;
  } finally {
    document.body.removeChild(wrapper);
  }
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, payload?: unknown) => Promise<T>;
    };
  }
}

export async function generateDocx(markdownContent: string, filename?: string): Promise<void> {
  const lines    = markdownContent.split('\n');
  const children: (Paragraph | Table)[] = [];
  let i = 0;

  while (i < lines.length) {
    const line = lines[i];

    // H1-H4
    if (/^# /.test(line))    { children.push(new Paragraph({ heading: HeadingLevel.HEADING_1, children: inlineRuns(line.slice(2).trim(), COLOR_H1, 36, true), spacing: { before: 300, after: 160 }, border: { bottom: { style: BorderStyle.SINGLE, size: 6, color: 'cccccc', space: 4 } } })); i++; continue; }
    if (/^## /.test(line))   { children.push(new Paragraph({ heading: HeadingLevel.HEADING_2, children: inlineRuns(line.slice(3).trim(), COLOR_H2, 28, true), spacing: { before: 240, after: 120 } })); i++; continue; }
    if (/^### /.test(line))  { children.push(new Paragraph({ heading: HeadingLevel.HEADING_3, children: inlineRuns(line.slice(4).trim(), COLOR_H3, 24, true), spacing: { before: 180, after: 80 } })); i++; continue; }
    if (/^#### /.test(line)) { children.push(new Paragraph({ children: inlineRuns(line.slice(5).trim(), COLOR_SUBTLE, 22, true), spacing: { before: 140, after: 60 } })); i++; continue; }

    // HR
    if (/^---+$/.test(line.trim())) {
      children.push(new Paragraph({ children: [new TextRun({ text: '' })], border: { bottom: { style: BorderStyle.SINGLE, size: 4, color: 'dddddd', space: 1 } }, spacing: { before: 120, after: 120 } }));
      i++; continue;
    }

    // Blockquote
    if (/^> /.test(line)) {
      children.push(new Paragraph({ children: inlineRuns(line.slice(2).trim(), COLOR_SUBTLE), indent: { left: 560 }, border: { left: { style: BorderStyle.SINGLE, size: 12, color: 'aaaaaa', space: 8 } }, spacing: { before: 80, after: 80 } }));
      i++; continue;
    }

    // Code block
    if (line.startsWith('```')) {
      const blockLang = line.slice(3).trim().toLowerCase();
      i++;
      const codeLines: string[] = [];
      let depth = 1;
      while (i < lines.length && depth > 0) {
        const current = lines[i];
        if (current.startsWith('```')) {
          const lang = current.slice(3).trim();
          if (lang) { depth++; } else { depth--; if (depth === 0) { i++; break; } }
        }
        if (depth > 0) codeLines.push(current);
        i++;
      }
      const codeContent = codeLines.join('\n');

      if (blockLang === 'echarts') {
        try {
          const imgData = await renderEChartsToImage(codeContent);
          children.push(new Paragraph({ children: [new ImageRun({ data: imgData, transformation: { width: 600, height: 300 }, type: 'png' })], spacing: { before: 120, after: 120 } }));
        } catch (e) {
          console.warn('ECharts render failed in generateDocx:', e);
          children.push(new Paragraph({ children: [new TextRun({ text: '[Graphique — rendu indisponible]', color: COLOR_SUBTLE, italics: true, size: 20 })], spacing: { before: 60, after: 60 } }));
        }
        continue;
      }
      if (blockLang === 'word' || blockLang === 'mermaid') continue;
      for (const cl of codeLines) {
        children.push(new Paragraph({ children: [new TextRun({ text: cl, font: 'Courier New', color: COLOR_CODE, size: 18 })], shading: { type: ShadingType.CLEAR, fill: COLOR_CODE_BG }, spacing: { before: 0, after: 0 }, indent: { left: 280 } }));
      }
      continue;
    }

    // Table
    if (/^\|/.test(line)) {
      const tableLines: string[] = [];
      while (i < lines.length && /^\|/.test(lines[i])) { tableLines.push(lines[i]); i++; }
      const dataRows = tableLines.filter(r => !/^\|[-:\s|]+\|$/.test(r.trim()));
      if (dataRows.length >= 1) {
        const [head, ...body] = dataRows;
        const parseRow = (r: string) => r.split('|').filter((_, j, a) => j > 0 && j < a.length - 1).map(c => c.trim());
        const headers  = parseRow(head);
        const colCount = headers.length;
        const colW     = Math.floor(9360 / Math.max(colCount, 1));
        const tableRows = [
          new TableRow({ tableHeader: true, children: headers.map(h => new TableCell({ children: [new Paragraph({ children: [new TextRun({ text: h, bold: true, color: COLOR_H2, size: 20 })], spacing: { before: 60, after: 60 } })], shading: { type: ShadingType.CLEAR, fill: 'f5f5f5' }, margins: { top: 80, bottom: 80, left: 140, right: 140 }, width: { size: colW, type: WidthType.DXA } })) }),
          ...body.map((row, ri) => new TableRow({ children: parseRow(row).map(cell => new TableCell({ children: [new Paragraph({ children: inlineRuns(cell, COLOR_TEXT), spacing: { before: 40, after: 40 } })], shading: ri % 2 === 1 ? { type: ShadingType.CLEAR, fill: 'fafafa' } : undefined, margins: { top: 60, bottom: 60, left: 140, right: 140 }, width: { size: colW, type: WidthType.DXA } })) })),
        ];
        children.push(new Table({ rows: tableRows, width: { size: 9360, type: WidthType.DXA }, columnWidths: Array(colCount).fill(colW) }));
        children.push(new Paragraph({ children: [new TextRun('')], spacing: { before: 80, after: 0 } }));
      }
      continue;
    }

    // Unordered list
    if (/^[\*\-] /.test(line)) {
      children.push(new Paragraph({ bullet: { level: 0 }, children: inlineRuns(line.slice(2).trim(), COLOR_TEXT), spacing: { before: 40, after: 40 } }));
      i++; continue;
    }

    // Ordered list
    const olMatch = line.match(/^(\d+)\. (.+)/);
    if (olMatch) {
      children.push(new Paragraph({ numbering: { reference: 'default-numbering', level: 0 }, children: inlineRuns(olMatch[2].trim(), COLOR_TEXT), spacing: { before: 40, after: 40 } }));
      i++; continue;
    }

    // Empty line
    if (!line.trim()) {
      children.push(new Paragraph({ children: [new TextRun('')], spacing: { before: 60, after: 0 } }));
      i++; continue;
    }

    // Bare ECharts JSON fallback (starts with '{' and contains "series")
    if (line.trim() === '{') {
      const jsonLines = [line]; i++;
      let depth = 1;
      while (i < lines.length && depth > 0) {
        const l = lines[i];
        for (const ch of l) { if (ch === '{') depth++; else if (ch === '}') depth--; }
        jsonLines.push(l); i++;
        if (depth === 0) break;
      }
      const jsonText = jsonLines.join('\n');
      if (/"series"/.test(jsonText)) {
        try {
          const imgData = await renderEChartsToImage(jsonText);
          children.push(new Paragraph({ children: [new ImageRun({ data: imgData, transformation: { width: 600, height: 300 }, type: 'png' })], spacing: { before: 120, after: 120 } }));
        } catch {
          children.push(new Paragraph({ children: [new TextRun({ text: '[Graphique — rendu indisponible]', color: COLOR_SUBTLE, italics: true, size: 20 })], spacing: { before: 60, after: 60 } }));
        }
      } else {
        for (const cl of jsonLines) {
          children.push(new Paragraph({ children: [new TextRun({ text: cl, font: 'Courier New', color: COLOR_CODE, size: 18 })], spacing: { before: 0, after: 0 }, indent: { left: 280 } }));
        }
      }
      continue;
    }

    // Bold-only line → promoted heading
    const boldLineMatch = line.match(/^\*\*(.+)\*\*\s*$/);
    if (boldLineMatch) {
      const boldText = boldLineMatch[1].trim();
      const level = /^\d+\.\d+/.test(boldText) ? 3 : /^\d+[.)]/.test(boldText) ? 2 : 1;
      if (level === 1) {
        children.push(new Paragraph({ heading: HeadingLevel.HEADING_1, children: [new TextRun({ text: boldText, bold: true, color: COLOR_H1, size: 36 })], spacing: { before: 300, after: 160 }, border: { bottom: { style: BorderStyle.SINGLE, size: 6, color: 'cccccc', space: 4 } } }));
      } else if (level === 2) {
        children.push(new Paragraph({ heading: HeadingLevel.HEADING_2, children: [new TextRun({ text: boldText, bold: true, color: COLOR_H2, size: 28 })], spacing: { before: 240, after: 120 } }));
      } else {
        children.push(new Paragraph({ heading: HeadingLevel.HEADING_3, children: [new TextRun({ text: boldText, bold: true, color: COLOR_H3, size: 24 })], spacing: { before: 180, after: 80 } }));
      }
      i++; continue;
    }

    // Normal paragraph
    children.push(new Paragraph({ children: inlineRuns(line, COLOR_TEXT), spacing: { before: 60, after: 60 } }));
    i++;
  }

  const doc = new Document({
    numbering: {
      config: [{ reference: 'default-numbering', levels: [{ level: 0, format: 'decimal', text: '%1.', alignment: AlignmentType.START, style: { paragraph: { indent: { left: 560, hanging: 360 } } } }] }],
    },
    styles: { default: { document: { run: { font: 'Calibri', color: COLOR_TEXT, size: 22 } } } },
    sections: [{ properties: { page: { margin: { top: 1134, right: 1134, bottom: 1134, left: 1134 } } }, children }],
  });

  const blob = await Packer.toBlob(doc);

  // Tauri v2 save dialog
  if (window.__TAURI_INTERNALS__) {
    try {
      const invoke = window.__TAURI_INTERNALS__.invoke;
      const savePath = await invoke<string | null>('plugin:dialog|save', {
        payload: { defaultPath: filename || 'document.docx', filters: [{ name: 'Word Document', extensions: ['docx'] }] },
      });
      if (savePath) {
        const arrayBuffer = await blob.arrayBuffer();
        const bytes = Array.from(new Uint8Array(arrayBuffer));
        await invoke('plugin:fs|write_file', { payload: { path: savePath, data: bytes } });
      }
      return;
    } catch (e) {
      console.warn('Tauri save failed, falling back to browser download', e);
    }
  }

  // Browser fallback
  const url = URL.createObjectURL(blob);
  const a   = document.createElement('a');
  a.href = url; a.download = filename || 'document.docx';
  document.body.appendChild(a); a.click(); document.body.removeChild(a);
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}
