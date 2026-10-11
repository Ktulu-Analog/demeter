// ============================================================================
// Demeter — Assistant IA desktop
// ============================================================================
// Licence : GNU Affero General Public License v3.0 (AGPL-3.0)
// ----------------------------------------------------------------------------
// Assainissement du SVG produit par le LLM (constat D-07).
//
// L'ancienne version retirait <script> et les attributs on*="…" par expressions
// régulières : contournable (`<svg/onload=…>`, attributs non quotés,
// `href="javascript:…"`, <foreignObject>, <set>/<animate>…).
//
// Ici le SVG est analysé par le parseur HTML du navigateur dans un document
// INERT (DOMParser : ni script, ni chargement de ressource), puis l'arbre est
// reconstruit avec des LISTES BLANCHES d'éléments et d'attributs. Tout ce qui
// n'est pas explicitement autorisé est supprimé.
// ============================================================================

const SVG_NS = 'http://www.w3.org/2000/svg';

// Éléments autorisés (minuscules). Absents volontairement : script, style,
// foreignObject, image, a, animate*, set, title, desc, switch, iframe…
const ALLOWED_ELEMENTS = new Set([
  'svg', 'g', 'defs', 'symbol', 'marker', 'path', 'rect', 'circle', 'ellipse',
  'line', 'polyline', 'polygon', 'text', 'tspan', 'textpath', 'lineargradient',
  'radialgradient', 'stop', 'clippath', 'mask', 'pattern', 'use',
]);

// Attributs autorisés (minuscules). Les attributs on* n'y figurent jamais.
const ALLOWED_ATTRS = new Set([
  'viewbox', 'width', 'height', 'x', 'y', 'x1', 'x2', 'y1', 'y2', 'cx', 'cy', 'r', 'rx', 'ry',
  'd', 'points', 'transform', 'fill', 'fill-opacity', 'fill-rule', 'stroke', 'stroke-width',
  'stroke-linecap', 'stroke-linejoin', 'stroke-dasharray', 'stroke-dashoffset', 'stroke-opacity',
  'stroke-miterlimit', 'opacity', 'class', 'id', 'style', 'font-family', 'font-size',
  'font-weight', 'font-style', 'text-anchor', 'dominant-baseline', 'alignment-baseline',
  'baseline-shift', 'letter-spacing', 'word-spacing', 'text-decoration', 'dx', 'dy', 'rotate',
  'textlength', 'lengthadjust', 'marker-start', 'marker-mid', 'marker-end', 'markerwidth',
  'markerheight', 'markerunits', 'refx', 'refy', 'orient', 'preserveaspectratio', 'xmlns',
  'offset', 'stop-color', 'stop-opacity', 'gradientunits', 'gradienttransform', 'spreadmethod',
  'fx', 'fy', 'fr', 'clip-path', 'mask', 'clippathunits', 'patternunits', 'patterncontentunits',
  'patterntransform', 'maskunits', 'maskcontentunits', 'href', 'xlink:href', 'visibility',
  'display', 'vector-effect', 'shape-rendering', 'text-rendering', 'startoffset', 'method',
  'spacing', 'xml:space', 'color', 'pathlength',
]);

// Propriétés CSS refusées dans style="…" (superposition à l'interface, etc.)
const BLOCKED_CSS_PROPS = new Set(['position', 'z-index', 'behavior', '-moz-binding']);

/** Valeur d'attribut dangereuse : schéma actif, expression CSS, url() non locale. */
function isUnsafeValue(value: string): boolean {
  const v = value.replace(/[\u0000-\u001f\u007f\s]+/g, '').toLowerCase();
  if (/javascript:|vbscript:|data:|expression\(|@import/.test(v)) return true;
  // url(...) autorisé uniquement vers une ancre locale : url(#id)
  const urls = value.match(/url\(\s*[^)]*\)/gi) || [];
  return urls.some((u) => !/^url\(\s*(['"]?)#[^'")\s]+\1\s*\)$/i.test(u));
}

function sanitizeStyle(style: string): string {
  return style
    .split(';')
    .map((d) => d.trim())
    .filter((d) => {
      if (!d) return false;
      const colon = d.indexOf(':');
      if (colon < 1) return false;
      const prop = d.slice(0, colon).trim().toLowerCase();
      const val = d.slice(colon + 1);
      return !BLOCKED_CSS_PROPS.has(prop) && !isUnsafeValue(val);
    })
    .join(';');
}

function sanitizeAttributes(el: Element): void {
  const tag = el.localName.toLowerCase();
  for (const attr of Array.from(el.attributes)) {
    const name = attr.name.toLowerCase();
    const value = attr.value;
    let keep = ALLOWED_ATTRS.has(name) && !name.startsWith('on');

    if (keep && (name === 'href' || name === 'xlink:href')) {
      // Uniquement une ancre locale, et seulement sur <use> / <textPath>.
      keep = (tag === 'use' || tag === 'textpath') && /^#[\w.:-]+$/.test(value.trim());
    } else if (keep && name === 'style') {
      const cleaned = sanitizeStyle(value);
      if (cleaned) el.setAttribute('style', cleaned);
      else keep = false;
    } else if (keep && isUnsafeValue(value)) {
      keep = false;
    }
    if (!keep) el.removeAttribute(attr.name);
  }
}

function sanitizeNode(parent: Element): void {
  for (const child of Array.from(parent.childNodes)) {
    if (child.nodeType === 3) continue; // texte : conservé (sérialisé échappé)
    if (child.nodeType !== 1) { parent.removeChild(child); continue; } // commentaires, PI…
    const el = child as Element;
    if (!ALLOWED_ELEMENTS.has(el.localName.toLowerCase()) || el.namespaceURI !== SVG_NS) {
      parent.removeChild(el); // élément non autorisé : supprimé avec ses enfants
      continue;
    }
    sanitizeAttributes(el);
    sanitizeNode(el);
  }
}

/**
 * Retourne le balisage d'un SVG assaini.
 * @throws si aucun élément <svg> exploitable n'est trouvé.
 */
export function sanitizeSvgMarkup(raw: string): string {
  const doc = new DOMParser().parseFromString(raw, 'text/html');
  const root = doc.body.querySelector('svg');
  if (!root || root.namespaceURI !== SVG_NS) {
    throw new Error('Aucun élément <svg> valide');
  }
  sanitizeAttributes(root);
  sanitizeNode(root);
  // Nécessaire pour que le SVG reste valide une fois téléchargé / copié seul.
  root.setAttribute('xmlns', SVG_NS);
  return root.outerHTML;
}
