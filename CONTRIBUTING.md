# Guide de contribution

Merci de l'intérêt pour Demeter ! Ce guide explique comment contribuer efficacement.

## Prérequis

- Rust ≥ 1.77 — [rustup.rs](https://rustup.rs)
- Node.js ≥ 18 — [nodejs.org](https://nodejs.org)
- Tauri CLI v2 — `cargo install tauri-cli --version "^2"`

## Lancer l'environnement de développement

```bash
git clone https://github.com/<votre-org>/demeter.git
cd demeter

# Installer les dépendances frontend
cd frontend && npm install && cd ..

# Démarrer en mode hot-reload
cargo tauri dev
```

Le serveur Rust écoute sur `http://localhost:45678`.
Le frontend React tourne sur `http://localhost:3010`.

## Structure du projet

```
demeter/
├── frontend/               ← React 18 + TypeScript + Vite
│   └── src/
│       ├── App.tsx         ← UI principale
│       ├── components/     ← Composants réutilisables
│       ├── hooks/          ← Hooks React
│       └── utils/          ← Utilitaires (docx, text, mermaid…)
│
└── src-tauri/              ← Backend Rust + config Tauri
    └── src/
        ├── api.rs          ← Endpoints Axum
        ├── db.rs           ← SQLite (tokio-rusqlite)
        ├── rag.rs          ← Pipeline RAG
        ├── mcp.rs          ← Client MCP (HTTP Streamable)
        ├── web_search.rs   ← Recherche web Tavily
        └── extract.rs      ← Extraction PDF / DOCX
```

## Workflow de contribution

1. **Fork** le dépôt et crée une branche depuis `main` :
   ```bash
   git checkout -b feat/ma-fonctionnalite
   ```

2. **Développe** en suivant les conventions existantes :
   - Rust : `cargo fmt` et `cargo clippy` avant de commiter
   - TypeScript : le projet utilise du TSX strict

3. **Teste** tes modifications :
   ```bash
   cargo test
   cd frontend && npm run typecheck
   ```

4. **Commite** avec un message clair (style [Conventional Commits](https://www.conventionalcommits.org/fr)) :
   ```
   feat: ajout du support des fichiers .xlsx dans l'ingestion RAG
   fix: correction du parsing SSE pour les réponses MCP longues
   docs: mise à jour du README avec les variables d'environnement
   ```

5. **Ouvre une Pull Request** vers `main` en décrivant :
   - Le problème résolu ou la fonctionnalité ajoutée
   - Comment tester le changement
   - Toute dépendance nouvelle introduite

## Signaler un bug

Utilise le [template d'issue](.github/ISSUE_TEMPLATE/bug_report.md) et inclus :
- La version de Demeter (`cargo tauri --version`)
- L'OS et sa version
- Les logs (`RUST_LOG=demeter=debug cargo tauri dev`)
- Les étapes pour reproduire

## Questions

Ouvre une [Discussion GitHub](../../discussions) pour toute question générale.
