/**
 * Corrige automatiquement les erreurs de syntaxe Mermaid les plus fréquentes générées par les LLMs.
 */
export function sanitizeMermaid(raw: string): string {
  let code = raw.trim();

  // 0. Détection et conversion C4 (retiré en Mermaid v11)
  if (/^C4\w*/i.test(code)) {
    const accentC4: Record<string, string> = { 'é': 'e', 'è': 'e', 'ê': 'e', 'ë': 'e', 'à': 'a', 'â': 'a', 'ä': 'a', 'ù': 'u', 'û': 'u', 'ü': 'u', 'ô': 'o', 'ö': 'o', 'î': 'i', 'ï': 'i', 'ç': 'c', 'É': 'E', 'È': 'E', 'À': 'A', 'Î': 'I', 'Ç': 'C' };
    const trlC4 = (s: string) => s.split('').map(c => accentC4[c] || c).join('').replace(/'/g, ' ').replace(/"/g, ' ');
    const lines = code.split('\n');
    const nodes = new Map<string, string>();
    const rels: Array<{ from: string; to: string; label: string }> = [];
    let title = '';
    for (const line of lines) {
      const t = line.trim();
      const titleM = t.match(/^title\s+(.+)$/i);
      if (titleM) { title = trlC4(titleM[1]); continue; }
      const nodeM = t.match(/^(?:Person|System|Container)\w*\s*\((\w+)\s*,\s*['"]([^'"]+)['"]/i);
      if (nodeM) { nodes.set(nodeM[1], trlC4(nodeM[2])); continue; }
      const relM = t.match(/^Rel\w*\s*\(\s*(\w+)\s*,\s*(\w+)\s*,\s*['"]([^'"]*)['"] /i);
      if (relM) rels.push({ from: relM[1], to: relM[2], label: trlC4(relM[3]) });
    }
    let out = title ? 'graph TD\n    %%' + title + '\n' : 'graph TD\n';
    const usedIds = new Set(rels.flatMap(r => [r.from, r.to]));
    for (const [id, label] of nodes) {
      if (usedIds.has(id)) out += '    ' + id + '[' + label + ']\n';
    }
    for (const { from, to, label } of rels) {
      out += label ? '    ' + from + ' -->|' + label + '| ' + to + '\n' : '    ' + from + ' --> ' + to + '\n';
    }
    return out.trim();
  }

  // 1. Normalise les fins de ligne
  code = code.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

  // 2. Tabulations → 4 espaces
  code = code.replace(/\t/g, '    ');

  // 3. Caractères problématiques globaux
  code = code.replace(/[\u2018\u2019\u201A\u201B`]/g, ' ');
  code = code.replace(/[\u2011\u2013\u2014\u2015\u2212]/g, '-');

  // 4. Supprime classDef / click / linkStyle / style inline
  code = code.replace(/^\s*(classDef|click|linkStyle|style\s+\w)\s.*$/gm, '');

  // 5. Supprime les commentaires %%
  code = code.replace(/^\s*%%.*$/gm, '');

  // ── Corrections flowchart / graph ──
  const isFlowchart = /^(graph|flowchart)\s/i.test(code);
  if (isFlowchart) {
    code = code.replace(/-->>/g, '-->');

    code = code.replace(/(\w+)\[([^\]"]*\([^\]]*)\)\]/g, (_match: string, id: string, label: string) => `${id}["${label})"]`);

    const accentMap: Record<string, string> = {
      'é': 'e', 'è': 'e', 'ê': 'e', 'ë': 'e', 'à': 'a', 'â': 'a', 'ä': 'a', 'ù': 'u', 'ú': 'u',
      'û': 'u', 'ü': 'u', 'ô': 'o', 'ó': 'o', 'ö': 'o', 'î': 'i', 'í': 'i', 'ï': 'i', 'ç': 'c',
      'É': 'E', 'È': 'E', 'Ê': 'E', 'À': 'A', 'Â': 'A', 'Î': 'I', 'Ï': 'I', 'Ç': 'C', 'Ô': 'O', 'Ù': 'U',
    };
    const transliterate = (s: string) => s.split('').map(c => accentMap[c] || c).join('');

    code = code.replace(/\[([^\]]+)\]/g, (_m: string, content: string) => {
      const fixed = transliterate(content).replace(/'/g, ' ').replace(/\(/g, ' ').replace(/\)/g, ' ').replace(/\s+/g, ' ').trim();
      return '[' + fixed + ']';
    });
    code = code.replace(/\{([^}]+)\}/g, (_m: string, content: string) => {
      const fixed = transliterate(content).replace(/'/g, ' ').replace(/\s+/g, ' ').trim();
      return '{' + fixed + '}';
    });
    code = code.replace(/\|([^|\n]+)\|/g, (_m: string, label: string) => '|' + transliterate(label).replace(/'/g, ' ') + '|');

    code = code.replace(/^(\s*)([A-Za-z\u00C0-\u00FF0-9_]+)(\s*[\[{(>])/gm, (_m: string, indent: string, id: string, sep: string) => indent + transliterate(id) + sep);

    const reserved = /^(\s*)(end|start|class|style|default)(\s*[\[{(>-])/gm;
    code = code.replace(reserved, (_m: string, indent: string, id: string, sep: string) => `${indent}N_${id}${sep}`);

    const seenNodeIds = new Map<string, string>();
    code = code.replace(/\b([A-Za-z][A-Za-z0-9_]*)(\[([^\]]*)\]|\{([^}]*)\}|\(([^)]*)\))/g, (_m: string, id: string, labelBlock: string) => {
      if (seenNodeIds.has(id)) return id;
      seenNodeIds.set(id, labelBlock);
      return _m;
    });
  }

  // ── Corrections gantt ──
  const isGantt = /^gantt\b/i.test(code);
  if (isGantt) {
    const accentMapGantt: Record<string, string> = {
      'é': 'e', 'è': 'e', 'ê': 'e', 'ë': 'e', 'à': 'a', 'â': 'a', 'ä': 'a', 'ù': 'u', 'ú': 'u',
      'û': 'u', 'ü': 'u', 'ô': 'o', 'ó': 'o', 'ö': 'o', 'î': 'i', 'í': 'i', 'ï': 'i', 'ç': 'c',
      'É': 'E', 'È': 'E', 'Ê': 'E', 'À': 'A', 'Â': 'A', 'Î': 'I', 'Ï': 'I', 'Ç': 'C', 'Ô': 'O', 'Ù': 'U',
    };
    const trl = (s: string) => s.split('').map(c => accentMapGantt[c] || c).join('');
    code = code.replace(/^(\s*section\s+)(.+)$/gm, (_m: string, prefix: string, name: string) => prefix + trl(name));
    code = code.replace(/^(\s*)([^:\n]+?)(\s*:[^\n]*)$/gm, (_m: string, indent: string, taskName: string, rest: string) => {
      if (/^\s*(title|dateFormat|axisFormat|%%|section)/i.test(indent + taskName)) return _m;
      return indent + trl(taskName).replace(/'/g, ' ') + rest;
    });
  }

  // ── Corrections sequenceDiagram ──
  const isSequence = /^sequenceDiagram/i.test(code);
  if (isSequence) {
    code = code.replace(/^\s*subgraph\s.*$/gm, '');
    code = code.replace(/(->+[^:\n]*:[^\n]*?)'/g, '$1');
    const opens  = (code.match(/^\s*(alt|opt|loop|par|critical|break)\b/gm) || []).length;
    const closes = (code.match(/^\s*end\s*$/gm) || []).length;
    const diff = opens - closes;
    if (diff > 0) {
      code = code + '\n' + 'end\n'.repeat(diff);
    } else if (diff < 0) {
      let toRemove = -diff;
      code = code.replace(/\n\s*end\s*$/gm, (m) => {
        if (toRemove > 0) { toRemove--; return ''; }
        return m;
      });
    }
  }

  // 11. Nettoyage final
  code = code.replace(/[ \t]+$/gm, '');
  code = code.replace(/\n{3,}/g, '\n\n');

  return code.trim();
}
