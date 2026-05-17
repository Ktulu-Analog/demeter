# Demeter

> Assistant RH intelligent — application desktop cross-platform (Tauri v2 · Rust · React)

[![CI](https://github.com/Ktulu-Analog/demeter/actions/workflows/ci.yml/badge.svg)](https://github.com/<votre-org>/demeter/actions/workflows/ci.yml)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL%20v3-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-v2-24C8D8?logo=tauri)](https://tauri.app)
[![Rust](https://img.shields.io/badge/Rust-1.77+-orange?logo=rust)](https://rustup.rs)

Demeter est un assistant IA spécialisé en ressources humaines (fonction publique d'État),
fonctionnant entièrement en local sans service externe — aucun Docker, aucun cloud requis.
Le backend **Rust/Axum** est embarqué dans l'application **Tauri v2** et pilote un LLM
via n'importe quelle API compatible OpenAI (Albert, OpenAI, Mistral, Ollama…).

---

## Fonctionnalités

| Capacité | Détail |
|---|---|
| 💬 **Chat IA multi-espaces** | Demeter RH, Recrutement, Congés, Fiches de paie, Formations |
| 📚 **RAG** | Ingestion PDF/DOCX, recherche sémantique + reranking via Albert |
| 🔌 **MCP** | Connexion à des serveurs MCP externes (HTTP Streamable 2025-03-26) |
| 🌐 **Recherche web** | Intégration Tavily API — résultats injectés dans le contexte |
| 📄 **Génération Word** | Export `.docx` depuis le chat en un clic |
| 📊 **Graphiques ECharts** | Blocs ` ```echarts ` rendus interactivement |
| 📐 **Diagrammes Mermaid** | Flowcharts, séquences, Gantt… |
| ∑ **Formules KaTeX** | Rendu LaTeX inline (`$…$`) et bloc (`$$…$$`) |
| 💾 **Persistance locale** | Conversations en SQLite — aucun compte requis |
| 🖥️ **Cross-platform** | Linux · macOS · Windows |

---

## Architecture

```
demeter/
├── frontend/                   ← React 18 + TypeScript + Vite
│   └── src/
│       ├── App.tsx             ← UI principale & gestion de l'état
│       ├── DialogContext.tsx   ← Contexte des dialogues
│       ├── components/
│       │   ├── MarkdownComponents.tsx   ← Rendu Markdown + ECharts + Mermaid
│       │   ├── IngestionModal.tsx       ← Interface RAG
│       │   ├── ArtifactsPanel.tsx       ← Panneau Word/artefacts
│       │   ├── PromptsEditorModal.tsx   ← Éditeur d'espaces & prompts
│       │   └── Message.tsx             ← Rendu d'un message
│       ├── hooks/              ← Hooks React (streaming, MCP…)
│       └── utils/              ← docx, text, mermaid-sanitizer, artifacts
│
└── src-tauri/                  ← Backend Rust + config Tauri
    └── src/
        ├── api.rs              ← Endpoints Axum (chat, RAG, MCP, search…)
        ├── db.rs               ← SQLite via tokio-rusqlite
        ├── models.rs           ← Types Serde (Chat, Conversation, Chunk…)
        ├── rag.rs              ← Pipeline RAG (retrieve → rerank → inject)
        ├── mcp.rs              ← Client MCP HTTP Streamable
        ├── web_search.rs       ← Intégration Tavily API
        ├── extract.rs          ← Extraction texte PDF (lopdf) et DOCX (zip+XML)
        ├── spaces.rs           ← Lecture/écriture prompts.yml
        └── lib.rs              ← Setup Tauri + démarrage serveur HTTP
```

### Stack technique

| Couche | Technologie |
|---|---|
| Desktop shell | [Tauri v2](https://tauri.app) |
| Backend | Rust · [Axum](https://github.com/tokio-rs/axum) · [tokio](https://tokio.rs) |
| Base de données | SQLite via [tokio-rusqlite](https://github.com/programatik29/tokio-rusqlite) |
| HTTP client | [reqwest](https://github.com/seanmonstar/reqwest) (rustls, HTTP/2) |
| Frontend | React 18 · TypeScript · Vite |
| Rendu | react-markdown · KaTeX · Mermaid · ECharts |
| Documents | [docx](https://www.npmjs.com/package/docx) (génération Word) |

---

## Prérequis

### Rust ≥ 1.77
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

### Node.js ≥ 18
```bash
nvm install 20 && nvm use 20
```

### Tauri CLI v2
```bash
cargo install tauri-cli --version "^2"
# ou via npm :
npm install -g @tauri-apps/cli@^2
```

### Dépendances système

**Linux (Debian/Ubuntu)**
```bash
sudo apt update && sudo apt install -y \
  libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  patchelf build-essential
```

**macOS** — `xcode-select --install`

**Windows** — aucune dépendance supplémentaire (WebView2 inclus avec Windows 10/11).

---

## Démarrage rapide

```bash
git clone https://github.com/<votre-org>/demeter.git
cd demeter

cd frontend && npm install && cd ..
cargo tauri dev
```

L'interface s'ouvre automatiquement.
Le serveur Rust écoute sur `http://localhost:45678`.

### Build production

```bash
cd frontend && npm install && npm run build && cd ..
cargo tauri build
```

Les installateurs sont générés dans `src-tauri/target/release/bundle/` :
- Linux : `.deb` + `.AppImage`
- macOS : `.dmg`
- Windows : `.msi` + `.exe`

---

## Configuration

### Connexion au LLM

Au premier lancement, ouvre **Paramètres** (⌘, / Ctrl+,) et renseigne :

- **Endpoint** — URL de l'API compatible OpenAI  
  ex. `https://albert.api.etalab.gouv.fr/v1`
- **Clé API** — ton token Bearer
- **Modèle** — identifiant du modèle  
  ex. `meta-llama/Llama-3.1-8B-Instruct`

### Variables d'environnement

| Variable | Défaut | Description |
|---|---|---|
| `RERANK_MODEL` | `bge-reranker-v2-m3` | Modèle de reranking pour le RAG |
| `RAG_TOP_K` | `20` | Chunks récupérés avant reranking |
| `RAG_TOP_RERANK` | `5` | Chunks conservés après reranking |
| `RAG_MIN_SCORE` | `0.15` | Score minimum (filtre qualité) |
| `RUST_LOG` | `demeter=info` | Niveau de log (`debug`, `info`, `warn`) |

### Espaces & Prompts

Les espaces sont configurés dans `src-tauri/prompts.yml` (embarqué dans le binaire, copié au premier lancement dans le répertoire de données utilisateur). L'éditeur intégré (**Espaces** → ⌘E / Ctrl+E) permet de les modifier sans toucher au YAML.

---

## Données persistées

| OS | Chemin |
|---|---|
| Linux | `~/.local/share/fr.demeter.app/` |
| macOS | `~/Library/Application Support/fr.demeter.app/` |
| Windows | `%APPDATA%\fr.demeter.app\` |

- `conversations.db` — SQLite des conversations
- `prompts.yml` — configuration des espaces

---

## RAG — Ingestion de documents

1. Ouvre **Ingestion RAG** (⌘R / Ctrl+R)
2. Crée ou sélectionne une collection
3. Dépose tes fichiers PDF ou DOCX
4. Active le RAG dans une conversation via le bouton 📚

Pipeline : extraction texte → chunking → embeddings (Albert) → reranking (BGE) → injection contexte.

---

## MCP — Outils externes

Demeter supporte le [Model Context Protocol](https://modelcontextprotocol.io) en transport **HTTP Streamable** (spec 2025-03-26), compatible avec `mcp.data.gouv.fr` et tout serveur FastMCP stateless.

Ajoute des serveurs MCP dans les paramètres. Les outils disponibles sont découverts automatiquement.

---

## Contribution

Les contributions sont les bienvenues ! Consulte [CONTRIBUTING.md](CONTRIBUTING.md) pour les détails.

```bash
# Fork → branche
git checkout -b feat/ma-fonctionnalite

# Vérifications avant PR
cargo fmt && cargo clippy
cd frontend && npm run typecheck
```

---

## Licence

[AGPL-3.0](LICENSE) — © 2026 **Pierre Couget** — [ktulu.analog@gmail.com](mailto:ktulu.analog@gmail.com)
