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


export type ArtifactType = 'full' | 'table' | 'word' | 'chart' | 'mermaid';

export interface Artifact {
  id: string;
  type: ArtifactType;
  label: string;
  icon: string;
  content: string;
}

/**
 * Reconstruit le contenu d'un bloc word en fusionnant les blocs echarts/mermaid orphelins qui suivent.
 */
export function reconstructWordContent(
  fullContent: string,
  _wordStart: number,
  wordEnd: number,
  wordBodyContent: string,
): string {
  const after = fullContent.slice(wordEnd);
  if (!after.trim()) return wordBodyContent;

  const extra: string[] = [];
  let pos = 0;
  let lastTextEnd = 0;

  while (pos < after.length) {
    const tickPos = after.indexOf('```', pos);
    if (tickPos === -1) {
      const tail = after.slice(lastTextEnd).trimEnd();
      if (tail) extra.push(tail);
      break;
    }

    const textChunk = after.slice(lastTextEnd, tickPos).trimEnd();
    if (textChunk) extra.push(textChunk);

    const lineEnd = after.indexOf('\n', tickPos + 3);
    if (lineEnd === -1) break;
    const lang = after.slice(tickPos + 3, lineEnd).trim().toLowerCase();

    const closePos = after.indexOf('\n```', lineEnd);
    if (closePos === -1) break;
    const blockEnd = closePos + 4;

    if (lang === 'echarts' || lang === 'mermaid') {
      const blockContent = after.slice(lineEnd + 1, closePos);
      extra.push('```' + lang + '\n' + blockContent + '\n```');
    }

    pos = blockEnd;
    lastTextEnd = blockEnd;
  }

  const extraContent = extra.join('\n').trim();
  if (!extraContent) return wordBodyContent;
  return wordBodyContent + '\n\n' + extraContent;
}

/**
 * Extrait tous les artefacts d'un contenu markdown (tables, word, echarts, mermaid).
 */
export function extractArtifacts(content: string, msgIndex: number): Artifact[] {
  const artifacts: Artifact[] = [];
  artifacts.push({ id: `full-${msgIndex}`, type: 'full', label: 'Réponse complète', icon: '📄', content });

  // Tables
  const tableRegex = /(\|.+\|\n(?:\|[-:| ]+\|\n)(?:\|.+\|\n?)+)/g;
  let tMatch: RegExpExecArray | null;
  let tIdx = 0;
  while ((tMatch = tableRegex.exec(content)) !== null) {
    tIdx++;
    artifacts.push({ id: `table-${msgIndex}-${tIdx}`, type: 'table', label: `Tableau ${tIdx}`, icon: '📊', content: tMatch[1].trim() });
  }

  // Blocs ```word``` (avec gestion de l'imbrication)
  let wIdx = 0;
  {
    let pos = 0;
    while (pos < content.length) {
      const startMarker = content.indexOf('```word', pos);
      if (startMarker === -1) break;
      const titleStart = startMarker + 7;
      const titleEnd = content.indexOf('\n', titleStart);
      if (titleEnd === -1) break;
      const inlineTitle = content.slice(titleStart, titleEnd).trim();
      let depth = 1;
      let cur = titleEnd + 1;
      while (cur < content.length && depth > 0) {
        const nextTicks = content.indexOf('```', cur);
        if (nextTicks === -1) { cur = content.length; break; }
        const afterTicks = content.slice(nextTicks + 3, nextTicks + 30);
        const isOpen = /^[a-zA-Z]/.test(afterTicks);
        if (isOpen) {
          depth++;
          cur = nextTicks + 3;
        } else {
          depth--;
          if (depth === 0) {
            const wordEnd = nextTicks + 3;
            const rawBody = content.slice(titleEnd + 1, nextTicks).trim();
            const bodyContent = reconstructWordContent(content, startMarker, wordEnd, rawBody);
            wIdx++;
            const firstLine = bodyContent.split('\n')[0];
            const h1Match = firstLine.match(/^#\s+(.+)/);
            const label = (inlineTitle || (h1Match && h1Match[1]) || `Document ${wIdx}`).slice(0, 40);
            artifacts.push({ id: `word-${msgIndex}-${wIdx}`, type: 'word', label, icon: '📄', content: bodyContent });
            pos = wordEnd;
          } else {
            cur = nextTicks + 3;
          }
        }
      }
      if (depth > 0) break;
    }
  }

  // ECharts
  const echartsRegex = /```echarts\n([\s\S]*?)```/g;
  let eMatch: RegExpExecArray | null;
  let eIdx = 0;
  while ((eMatch = echartsRegex.exec(content)) !== null) {
    eIdx++;
    artifacts.push({ id: `chart-${msgIndex}-${eIdx}`, type: 'chart', label: `Graphique ${eIdx}`, icon: '📈', content: eMatch[1].trim() });
  }

  // Mermaid
  const mermaidRegex = /```mermaid\n([\s\S]*?)```/g;
  let mMatch: RegExpExecArray | null;
  let mIdx = 0;
  while ((mMatch = mermaidRegex.exec(content)) !== null) {
    mIdx++;
    artifacts.push({ id: `mermaid-${msgIndex}-${mIdx}`, type: 'mermaid', label: `Diagramme ${mIdx}`, icon: '🔀', content: mMatch[1].trim() });
  }

  return artifacts;
}

/**
 * Retourne true si le contenu contient des artefacts visuels.
 */
export function hasArtifacts(content: string | null | undefined): boolean {
  if (!content) return false;
  return /\|.+\|\n(?:\|[-:| ]+\|\n)/.test(content) ||
         /```echarts/.test(content) ||
         /```mermaid/.test(content) ||
         /```word/.test(content);
}
