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

// ── Réconciliation des locuteurs entre tranches transcrites séparément ──────
// Quand un fichier audio est découpé en plusieurs tranches chevauchantes, chaque
// tranche est diarizée indépendamment par le modèle : les labels de locuteurs
// (ex. "SPEAKER_00") ne sont PAS garantis cohérents d'une tranche à l'autre.
// On aligne les tranches sur leur zone de recouvrement (mêmes timestamps
// absolus) pour ré-attribuer des labels canoniques stables sur tout le fichier.
//
// Heuristique : proximité temporelle + similarité textuelle des segments situés
// dans la zone de recouvrement. Ce n'est pas une identification vocale réelle —
// en cas de recouvrement ambigu (silence, plusieurs locuteurs qui s'enchaînent
// vite), l'attribution peut être imparfaite.

export interface RawSegment {
  id: number;
  text: string;
  start: number; // secondes, relatif au début de la tranche
  end: number;
  speaker?: string | null;
}

export interface ChunkResult {
  startSec: number; // décalage de la tranche dans le fichier original
  segments: RawSegment[];
}

export interface MergedSegment {
  id: number;
  text: string;
  start: number; // secondes, absolu (fichier original)
  end: number;
  speaker?: string | null; // label canonique, stable sur tout le fichier
}

// ── Similarité textuelle simple (Jaccard sur tokens normalisés) ──────────────
function normalizeTokens(text: string): Set<string> {
  return new Set(
    text
      .toLowerCase()
      .normalize('NFD').replace(/[\u0300-\u036f]/g, '') // retire les accents
      .replace(/[^a-z0-9\s]/g, ' ')
      .split(/\s+/)
      .filter(Boolean),
  );
}

function textSimilarity(a: string, b: string): number {
  const ta = normalizeTokens(a);
  const tb = normalizeTokens(b);
  if (ta.size === 0 && tb.size === 0) return 1;
  if (ta.size === 0 || tb.size === 0) return 0;
  let inter = 0;
  for (const t of ta) if (tb.has(t)) inter++;
  const union = ta.size + tb.size - inter;
  return union === 0 ? 0 : inter / union;
}

/**
 * Fusionne les résultats de plusieurs tranches (déjà positionnées dans le
 * temps absolu via `startSec`) en une liste unique de segments, avec des
 * labels de locuteurs réconciliés entre les tranches successives.
 *
 * `overlapSec` doit correspondre au recouvrement utilisé lors du découpage.
 */
export function reconcileChunks(chunks: ChunkResult[], overlapSec: number): MergedSegment[] {
  if (chunks.length === 0) return [];

  // Décale chaque segment vers son temps absolu dans le fichier original.
  const absoluteChunks = chunks.map(c => ({
    startSec: c.startSec,
    segments: c.segments.map(s => ({
      ...s,
      start: s.start + c.startSec,
      end: s.end + c.startSec,
    })),
  }));

  if (chunks.length === 1) {
    // Pas de recouvrement à réconcilier — on assigne juste des labels canoniques stables.
    const canonical = assignCanonicalLabels(absoluteChunks[0].segments, new Map());
    return canonical.segments;
  }

  let canonicalMap = new Map<string, string>(); // raw label (tranche courante) -> label canonique
  let nextSpeakerNum = 1;
  const merged: MergedSegment[] = [];

  // ── Tranche 0 : labels canoniques initiaux (première apparition = ordre d'apparition) ──
  const first = assignCanonicalLabels(absoluteChunks[0].segments, new Map(), nextSpeakerNum);
  canonicalMap = first.rawToCanonical;
  nextSpeakerNum = first.nextSpeakerNum;
  merged.push(...first.segments);

  for (let i = 1; i < absoluteChunks.length; i++) {
    const prevChunk = absoluteChunks[i - 1];
    const curChunk = absoluteChunks[i];
    const boundaryAbs = curChunk.startSec + overlapSec; // fin approximative de la zone de recouvrement

    // Segments de la tranche précédente situés dans la zone de recouvrement (juste avant sa fin)
    const prevTail = prevChunk.segments.filter(s => s.start >= curChunk.startSec - 1e-6);
    // Segments de la tranche courante situés dans la zone de recouvrement (juste après son début)
    const curHead = curChunk.segments.filter(s => s.start <= boundaryAbs);

    // ── Vote de correspondance raw-label(tranche i) -> canonical-label(tranche i-1) ──
    const votes = new Map<string, Map<string, number>>(); // rawLabel(i) -> (canonicalLabel -> score cumulé)
    for (const h of curHead) {
      if (!h.speaker) continue;
      let bestPrev: RawSegment | null = null;
      let bestScore = -Infinity;
      for (const p of prevTail) {
        if (!p.speaker) continue;
        const timeDist = Math.abs(p.start - h.start);
        if (timeDist > overlapSec + 5) continue; // trop loin pour être une correspondance plausible
        const sim = textSimilarity(p.text, h.text);
        const score = sim * 2 - timeDist / (overlapSec + 5); // priorité à la similarité textuelle
        if (score > bestScore) { bestScore = score; bestPrev = p; }
      }
      if (bestPrev && bestPrev.speaker) {
        const prevCanonical = canonicalMap.get(bestPrev.speaker) ?? bestPrev.speaker;
        const bucket = votes.get(h.speaker) ?? new Map<string, number>();
        bucket.set(prevCanonical, (bucket.get(prevCanonical) ?? 0) + Math.max(bestScore, 0.05));
        votes.set(h.speaker, bucket);
      }
    }

    // ── Construit le mapping raw -> canonical pour la tranche courante ──
    const rawToCanonicalCur = new Map<string, string>();
    const distinctRawSpeakers = Array.from(new Set(curChunk.segments.map(s => s.speaker).filter(Boolean) as string[]));
    const usedCanonicalThisChunk = new Set<string>();
    for (const raw of distinctRawSpeakers) {
      const bucket = votes.get(raw);
      let chosen: string | null = null;
      if (bucket && bucket.size > 0) {
        let best = -Infinity;
        for (const [canonical, score] of bucket) {
          if (score > best && !usedCanonicalThisChunk.has(canonical)) { best = score; chosen = canonical; }
        }
      }
      if (!chosen) {
        chosen = `Intervenant ${nextSpeakerNum}`;
        nextSpeakerNum += 1;
      }
      rawToCanonicalCur.set(raw, chosen);
      usedCanonicalThisChunk.add(chosen);
    }
    canonicalMap = rawToCanonicalCur;

    // ── Ajoute les segments de la tranche courante (relabelés), en coupant le
    //    recouvrement au milieu de la zone commune pour éviter les doublons ──
    const cutoff = curChunk.startSec + overlapSec / 2;
    // Retire de `merged` les segments de la tranche précédente situés après le cutoff
    while (merged.length > 0 && merged[merged.length - 1].start >= cutoff) merged.pop();

    for (const s of curChunk.segments) {
      if (s.end <= cutoff) continue; // couvert par la tranche précédente
      merged.push({
        id: merged.length,
        text: s.text,
        start: s.start,
        end: s.end,
        speaker: s.speaker ? (rawToCanonicalCur.get(s.speaker) ?? s.speaker) : null,
      });
    }
  }

  return merged.sort((a, b) => a.start - b.start).map((s, idx) => ({ ...s, id: idx }));
}

/**
 * Regroupe les segments consécutifs partageant le même locuteur en un seul bloc
 * (évite d'afficher 7-8 fois de suite le même intervenant avec un bout de phrase
 * à chaque fois). Ne fusionne que les locuteurs identifiés (speaker non nul) —
 * les segments sans locuteur (diarization désactivée) restent inchangés.
 */
export function mergeConsecutiveSameSpeaker(segments: MergedSegment[]): MergedSegment[] {
  if (segments.length === 0) return segments;
  const out: MergedSegment[] = [];
  for (const seg of segments) {
    const last = out[out.length - 1];
    if (last && seg.speaker && last.speaker === seg.speaker) {
      last.text = `${last.text.trim()} ${seg.text.trim()}`.trim();
      last.end = seg.end;
    } else {
      out.push({ ...seg });
    }
  }
  return out.map((s, idx) => ({ ...s, id: idx }));
}

function assignCanonicalLabels(
  segments: { id: number; text: string; start: number; end: number; speaker?: string | null }[],
  _unused: Map<string, string>,
  startNum = 1,
): { segments: MergedSegment[]; rawToCanonical: Map<string, string>; nextSpeakerNum: number } {
  const rawToCanonical = new Map<string, string>();
  let nextSpeakerNum = startNum;
  const out: MergedSegment[] = segments.map(s => {
    let canonical: string | null = null;
    if (s.speaker) {
      canonical = rawToCanonical.get(s.speaker) ?? null;
      if (!canonical) {
        canonical = `Intervenant ${nextSpeakerNum}`;
        nextSpeakerNum += 1;
        rawToCanonical.set(s.speaker, canonical);
      }
    }
    return { id: s.id, text: s.text, start: s.start, end: s.end, speaker: canonical };
  });
  return { segments: out, rawToCanonical, nextSpeakerNum };
}
