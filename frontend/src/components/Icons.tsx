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


export function CopyIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor">
      <path d="M4 2a2 2 0 012-2h6a2 2 0 012 2v10a2 2 0 01-2 2H6a2 2 0 01-2-2V2zm2-1a1 1 0 00-1 1v10a1 1 0 001 1h6a1 1 0 001-1V2a1 1 0 00-1-1H6z"/>
      <path d="M2 5a1 1 0 00-1 1v8a1 1 0 001 1h7v-1H2V6H1V5h1z"/>
    </svg>
  );
}

export function CheckIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor">
      <path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z"/>
    </svg>
  );
}

export function DownloadIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor">
      <path d="M8 10.5l-3.5-3.5h2.5V2h2v5h2.5L8 10.5z"/>
      <path d="M2 12h12v1.5H2V12z"/>
    </svg>
  );
}

export function SpinnerIcon() {
  return (
    <svg viewBox="0 0 16 16" width="13" height="13" fill="currentColor" style={{ animation: 'spin .8s linear infinite' }}>
      <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 1.5a5.5 5.5 0 110 11A5.5 5.5 0 018 2.5z" opacity=".3"/>
      <path d="M8 1a7 7 0 017 7h-1.5A5.5 5.5 0 008 2.5V1z"/>
    </svg>
  );
}
