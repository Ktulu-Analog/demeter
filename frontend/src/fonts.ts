// ============================================================================
// Polices et feuille de style KaTeX embarquées (constat D-21).
// Plus aucun chargement depuis Google Fonts ou jsDelivr : l'application
// fonctionne hors ligne, n'expose pas l'IP de l'utilisateur à des tiers et
// peut appliquer une CSP stricte (font-src 'self').
// ============================================================================
import '@fontsource/sora/latin-400.css';
import '@fontsource/sora/latin-500.css';
import '@fontsource/sora/latin-600.css';
import '@fontsource/dm-sans/latin-400.css';
import '@fontsource/dm-sans/latin-500.css';
import '@fontsource/dm-sans/latin-600.css';
import '@fontsource/dm-sans/latin-400-italic.css';
import '@fontsource/inter/latin-400.css';
import '@fontsource/inter/latin-500.css';
import '@fontsource/inter/latin-600.css';
import '@fontsource/ubuntu/latin-400.css';
import '@fontsource/ubuntu/latin-500.css';
import '@fontsource/ubuntu/latin-700.css';
import '@fontsource/jetbrains-mono/latin-400.css';
import '@fontsource/jetbrains-mono/latin-500.css';
import 'katex/dist/katex.min.css';
