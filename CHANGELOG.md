# Changelog

Toutes les modifications notables de ce projet sont documentées dans ce fichier.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/),
et ce projet adhère au [Versioning Sémantique](https://semver.org/lang/fr/).

---

## [2.0.1] — 2026-05-18

### Amélioré

- **Modale Paramètres** — mise en page en deux colonnes (LLM à gauche, serveurs MCP à droite) pour une meilleure lisibilité et un accès plus rapide aux réglages
- **Modale RAG** — refonte ergonomique en deux colonnes : upload et gestion de la collection active à gauche, liste des collections Albert à droite ; la modale est plus large et moins haute
- **Cache collections RAG** — les collections Albert sont désormais mises en cache local (TTL 1 heure) pour un affichage instantané à la réouverture de la modale ; un indicateur d'âge du cache est affiché et un bouton permet de forcer le rechargement avant expiration
- **Tooltip collections** — au survol d'une collection, un panneau flottant affiche le propriétaire, le nombre de documents, la date de création et la date de dernière mise à jour
- **Correction boucle de chargement** — résolution d'un bug React provoquant un double appel à l'API collections à l'ouverture de la modale RAG (dépendance instable `showToast` dans `useCallback`)

---

## [2.0.0] — 2026-05-14

### Refonte majeure — Migration Python → Rust

Cette version constitue une réécriture complète du backend.
L'application Python/FastAPI + Docker est remplacée par un binaire **Rust (Axum)**
embarqué dans **Tauri v2**, sans dépendance externe.

### Ajouté
- Backend Rust (Axum) intégré à Tauri v2 — serveur HTTP sur `localhost:45678`
- Pipeline RAG complet : ingestion → chunking → embeddings → reranking (via API Albert)
- Support MCP (Model Context Protocol) — transport HTTP Streamable (spec 2025-03-26)
- Recherche web via Tavily API avec extraction de requête en français
- Génération de documents Word (`.docx`) directement depuis le chat
- Rendu de graphiques via **ECharts** (blocs ` ```echarts ` dans le markdown)
- Rendu de diagrammes via **Mermaid**
- Rendu de formules mathématiques via **KaTeX**
- Gestion des espaces (Demeter, Recrutement, Congés, Fiches de paie, Formations)
- Éditeur de prompts et d'espaces intégré
- Menu natif macOS/Linux/Windows avec raccourcis clavier
- Persistance SQLite des conversations sans service externe

### Modifié
- Frontend React adapté pour Vite (`.tsx` TypeScript)
- `API_BASE` pointe vers `http://localhost:45678` (plus de proxy)

### Supprimé
- Backend Python / FastAPI
- Docker et `docker-compose`
- Dépendance à Redis, PostgreSQL ou tout service externe

---

## [1.x.x] — Versions précédentes

Architecture Python/FastAPI + React (non publiée sur ce dépôt).
