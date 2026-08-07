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

// ── Découpage audio côté client (Web Audio API, sans dépendance externe) ─────
// Utilisé pour transcrire des fichiers trop volumineux pour un seul appel API :
// on décode le fichier, on le retaille en tranches chevauchantes, on ré-encode
// chaque tranche en WAV mono 16 kHz (léger, largement suffisant pour l'ASR),
// puis chaque tranche est envoyée séparément à l'endpoint de transcription.

export interface AudioChunk {
  /** Décalage (en secondes) du début de la tranche dans le fichier original. */
  startSec: number;
  /** Fin (en secondes) de la tranche dans le fichier original. */
  endSec: number;
  /** Fichier WAV prêt à être envoyé (mono, 16 kHz, PCM 16 bits). */
  file: File;
}

/** Fréquence d'échantillonnage cible pour l'export — largement suffisante pour la voix. */
const TARGET_SAMPLE_RATE = 16000;

/**
 * Récupère la durée (en secondes) d'un fichier audio sans le décoder entièrement,
 * via les métadonnées d'un élément <audio> — beaucoup plus léger que decodeAudioFile
 * pour un simple besoin de durée (utilisé pour estimer le temps de transcription restant).
 */
export function getAudioDurationSec(file: File): Promise<number> {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const audio = new Audio();
    const cleanup = () => URL.revokeObjectURL(url);
    audio.preload = 'metadata';
    audio.onloadedmetadata = () => {
      const duration = audio.duration;
      cleanup();
      if (Number.isFinite(duration) && duration > 0) resolve(duration);
      else reject(new Error('Durée audio indisponible'));
    };
    audio.onerror = () => { cleanup(); reject(new Error('Impossible de lire les métadonnées audio')); };
    audio.src = url;
  });
}

/** Décode un fichier audio (mp3/wav/…) en AudioBuffer via l'API Web Audio du navigateur. */
export async function decodeAudioFile(file: File): Promise<AudioBuffer> {
  const arrayBuffer = await file.arrayBuffer();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const Ctx: typeof AudioContext = (window as any).AudioContext || (window as any).webkitAudioContext;
  const ctx = new Ctx();
  try {
    const buffer = await ctx.decodeAudioData(arrayBuffer.slice(0));
    return buffer;
  } finally {
    ctx.close().catch(() => { /* silencieux */ });
  }
}

/**
 * Ré-échantillonne + downmixe un AudioBuffer vers mono / TARGET_SAMPLE_RATE.
 * Réduit fortement le poids de chaque tranche (utile pour rester sous une limite de taille).
 */
async function resampleToMono(buffer: AudioBuffer): Promise<AudioBuffer> {
  if (buffer.sampleRate === TARGET_SAMPLE_RATE && buffer.numberOfChannels === 1) return buffer;
  const targetLength = Math.ceil(buffer.duration * TARGET_SAMPLE_RATE);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const OfflineCtx: typeof OfflineAudioContext = (window as any).OfflineAudioContext || (window as any).webkitOfflineAudioContext;
  const offlineCtx = new OfflineCtx(1, targetLength, TARGET_SAMPLE_RATE);
  const source = offlineCtx.createBufferSource();
  source.buffer = buffer;
  source.connect(offlineCtx.destination);
  source.start(0);
  return offlineCtx.startRendering();
}

/** Encode un AudioBuffer mono en WAV PCM 16 bits (implémentation pure JS, sans dépendance). */
function audioBufferToWav(buffer: AudioBuffer): Blob {
  const numChannels = buffer.numberOfChannels; // 1 attendu après resampleToMono
  const sampleRate = buffer.sampleRate;
  const bytesPerSample = 2;
  const blockAlign = numChannels * bytesPerSample;
  const numFrames = buffer.length;
  const dataSize = numFrames * blockAlign;

  const arrayBuf = new ArrayBuffer(44 + dataSize);
  const view = new DataView(arrayBuf);

  const writeStr = (offset: number, str: string) => {
    for (let i = 0; i < str.length; i++) view.setUint8(offset + i, str.charCodeAt(i));
  };

  writeStr(0, 'RIFF');
  view.setUint32(4, 36 + dataSize, true);
  writeStr(8, 'WAVE');
  writeStr(12, 'fmt ');
  view.setUint32(16, 16, true);           // taille du sous-bloc fmt
  view.setUint16(20, 1, true);            // format PCM
  view.setUint16(22, numChannels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * blockAlign, true); // byte rate
  view.setUint16(32, blockAlign, true);
  view.setUint16(34, 16, true);           // bits par échantillon
  writeStr(36, 'data');
  view.setUint32(40, dataSize, true);

  const channelData = buffer.getChannelData(0);
  let offset = 44;
  for (let i = 0; i < numFrames; i++) {
    const s = Math.max(-1, Math.min(1, channelData[i]));
    view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    offset += 2;
  }

  return new Blob([arrayBuf], { type: 'audio/wav' });
}

/** Découpe un AudioBuffer en tranches chevauchantes de `chunkSec` secondes (recouvrement `overlapSec`). */
function sliceBuffer(buffer: AudioBuffer, chunkSec: number, overlapSec: number): { startSec: number; endSec: number; buffer: AudioBuffer }[] {
  const sampleRate = buffer.sampleRate;
  const totalSec = buffer.duration;
  const slices: { startSec: number; endSec: number; buffer: AudioBuffer }[] = [];

  let start = 0;
  while (start < totalSec) {
    const end = Math.min(start + chunkSec, totalSec);
    const startSample = Math.floor(start * sampleRate);
    const endSample = Math.floor(end * sampleRate);
    const length = endSample - startSample;

    const slice = new AudioBuffer({ length, sampleRate, numberOfChannels: buffer.numberOfChannels });
    for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
      const src = buffer.getChannelData(ch).slice(startSample, endSample);
      slice.copyToChannel(src, ch);
    }
    slices.push({ startSec: start, endSec: end, buffer: slice });

    if (end >= totalSec) break;
    start = end - overlapSec; // recouvrement avec la tranche suivante
  }

  return slices;
}

/**
 * Découpe un fichier audio en tranches WAV (mono 16 kHz) prêtes à être envoyées
 * individuellement à l'endpoint de transcription.
 */
export async function chunkAudioFile(
  file: File,
  opts: { chunkSec: number; overlapSec: number },
): Promise<AudioChunk[]> {
  const decoded = await decodeAudioFile(file);
  const slices = sliceBuffer(decoded, opts.chunkSec, opts.overlapSec);

  const chunks: AudioChunk[] = [];
  for (let i = 0; i < slices.length; i++) {
    const mono = await resampleToMono(slices[i].buffer);
    const blob = audioBufferToWav(mono);
    const chunkFile = new File([blob], `chunk-${String(i + 1).padStart(3, '0')}.wav`, { type: 'audio/wav' });
    chunks.push({ startSec: slices[i].startSec, endSec: slices[i].endSec, file: chunkFile });
  }
  return chunks;
}
