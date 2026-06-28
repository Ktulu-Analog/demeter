# Changelog

Toutes les modifications notables de ce projet sont documentées dans ce fichier.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/),
et ce projet adhère au [Versioning Sémantique](https://semver.org/lang/fr/).

---

## [2.3.1] — 2026-06-28

### Ajouté

- **Nouveaux prompts** — enrichissement des suggestions dans les espaces existants (`prompts.yml`)

### Amélioré

- **Génération SVG** — rendu plus robuste des blocs ` ```svg ` : meilleure gestion des cas limites et qualité visuelle améliorée

### Corrigé

- Corrections de bugs divers

## [2.3.0] — 2026-06-13

### Ajouté

- **Rendu SVG natif** — les blocs ` ```svg ` dans le markdown sont désormais rendus directement dans le chat comme graphiques vectoriels interactifs (`SvgBlock`) ; le composant sanitise le SVG, force `viewBox` et `preserveAspectRatio` pour un rendu responsive, et expose une barre d'outils permettant de copier le SVG dans le presse-papier ou de le télécharger en fichier `.svg`

- **Authentification Qdrant granulaire par collection** — le pipeline RAG supporte désormais des tokens JWT par collection (`qdrant_collection_tokens`) en complément de la clé admin globale ; la fonction `qdrant_auth` sélectionne automatiquement le bon token selon la collection cible : JWT utilisateur pré-signé → passé tel quel, clé admin + collections → JWT HS256 granulaire généré à la volée, liste vide → JWT read-only global

- **URL Qdrant configurable par utilisateur** — le champ `qdrant_url` dans les paramètres permet de pointer vers une instance Qdrant distante ou non-standard, en priorité sur la variable d'environnement `QDRANT_URL`

- **Indicateur de connexion Qdrant dans la modale RAG** — badge en temps réel (vert/rouge) indiquant l'état de l'instance Qdrant ; la modale effectue un ping toutes les 30 secondes

### Modifié

- **Remplacement des collections Albert par Qdrant** — le stockage vectoriel migre de l'API Albert vers une instance Qdrant locale ou distante ; `collection_id` (anciennement `i64` Albert) devient `collection_name` (`String` Qdrant) dans tous les modèles, routes et appels frontend

- **Pipeline RAG complet Qdrant** — toutes les opérations (création, suppression, renommage, listage de collections, ingestion de documents, recherche de chunks, suppression de documents) passent désormais par l'API REST Qdrant avec authentification JWT ou clé brute selon les credentials

- **Titre de la modale RAG** — renommé de « Collections RAG (Albert) » en « Collections RAG (Qdrant) »

- **`ExcalidrawBlock` redirigé vers `SvgBlock`** — le renderer Excalidraw (supprimé) est remplacé par un alias vers `SvgBlock` pour maintenir la compatibilité des imports existants

### Corrigé

- **Warnings de compilation** — suppression de l'import inutilisé `generate_admin_jwt` dans `api.rs` ; annotations `#[allow(dead_code)]` sur `build_accessible_collections`, `resolve_qdrant_key`, `Chunk::document_id` et `CollectionCreateRequest::description` pour les éléments conservés en prévision d'évolutions futures

---

## [2.2.1] — 2026-06-01

### Corrigé

- **RAG — Ingestion dans une collection privée** — trois bugs distincts empêchaient l'ingestion de documents dans les collections privées via l'API Albert :
  - Le proxy Rust ne transmettait pas correctement le `Content-Type` de la partie `file` dans le multipart ; Albert rejetait alors le champ comme un `UploadFile` invalide. Le content-type est désormais résolu depuis l'extension du fichier (`.pdf` → `application/pdf`, `.docx` → `application/vnd.openxmlformats-officedocument.wordprocessingml.document`, `.txt` → `text/plain`, `.md` → `text/markdown`) si le navigateur transmet `application/octet-stream` ou rien
  - La création de collection envoyait un champ `model` (`"BAAI/bge-m3"`) non supporté par le schéma `CollectionRequest` de l'API Albert, ce qui pouvait perturber l'assignation du modèle de vectorisation
  - Le fallback `unwrap_or_else` sur `mime_str()` recréait une `Part` sans `file_name`, rendant le fichier non identifiable côté serveur

- **RAG — Valeurs par défaut alignées avec l'API Albert** — `chunk_size` et `chunk_overlap` étaient initialisés à `1024` et `100` dans le proxy Rust, alors que l'UI affichait `2048` et `0`. Les deux couches utilisent maintenant `chunk_size = 2048` et `chunk_overlap = 0`

- **RAG — Libellé de l'unité** — les champs « Taille des chunks » et « Chevauchement » sont désormais libellés **(caractères)** et non plus **(tokens)**, conformément à la documentation de l'API Albert qui exprime ces valeurs en caractères

---

## [2.2.0] — 2026-05-29

### Ajouté

- **Alias MCP** — chaque serveur MCP peut désormais recevoir un nom d'affichage personnalisé (alias) dans les paramètres ; l'alias s'affiche à la place de l'URL dans le panel de statut, les boutons du chat et les info-bulles
- **Info-bulle de statut MCP** — survoler les boutons « Web » et « MCP » dans la zone de saisie affiche une info-bulle indiquant, pour chaque serveur concerné, son alias et son état de connexion en temps réel (point vert / rouge) ; le clic conserve son comportement habituel (panel détaillé avec la liste des outils)
- **MCP Web Search configurable** — le bouton « Web » utilise désormais un serveur MCP dédié (URL + alias configurables dans les paramètres) au lieu de l'API Tavily ; le serveur n'est activé que lorsque le bouton est allumé

### Amélioré

- **Appels MCP parallèles** — `list_tools` et `collect_all_tools` effectuent leurs requêtes en parallèle (`join_all`) ; le temps de réponse du panel MCP est désormais celui du serveur le plus lent, non la somme de tous les timeouts
- **Statut MCP fiable** — `list_tools` retourne maintenant un statut explicite (`bool`) distinct du nombre d'outils, ce qui évite de marquer « error » un serveur joignable mais exposant zéro outil
- **Interface MCP dans les paramètres** — la section Serveurs MCP est redessinée : cartes avec barre d'accentuation colorée, alias éditable inline, URL en monospace discret, bouton de suppression visible uniquement au survol ; le formulaire d'ajout adopte un style « zone pointillée » qui s'anime au focus
- **Console de développement** — ouverture automatique des DevTools au démarrage en mode debug

### Supprimé

- **Tavily API** — suppression complète de l'intégration Tavily (`web_search.rs`, `fetch_web_search`, champ `tavily_key`, variable d'environnement `TIMEOUT_WEB_SEARCH_SECS`, fonction `config::timeout_web_search`) ; remplacée par le MCP Web Search

---

## [2.1.1] — 2026-05-26

### Amélioré

- **config MCP** — les serveurs MCP s'affichent correctement dans la fenêtre dédiée.

---

## [2.1.0] — 2026-05-24

### Ajouté

- **Mode Comparaison** — nouveau mode permettant de soumettre la même question à deux modèles simultanément et de comparer les réponses côte à côte ; l'utilisateur choisit la réponse préférée, qui est injectée dans le chat principal. Le bouton « Comparer » apparaît dans la barre d'outils dès que deux modèles ou plus sont disponibles
- **Personnalisation de la police** — nouvelle section « Apparence » dans les paramètres : choix de la police (DM Sans, Inter, Système, Ubuntu) et de la taille (XS 12 px → XL 16 px) avec aperçu en temps réel ; les préférences sont appliquées immédiatement via les variables CSS `--font-body` et `--font-size-base`
- **Bouton « Aller en bas »** — bouton flottant qui apparaît automatiquement lorsque l'historique est remonté et disparaît quand l'utilisateur se trouve en bas du chat
- **Score vectoriel dans l'API** — le champ `score` des chunks RAG est désormais exposé dans les réponses JSON (en complément du `rerank_score` existant) et affiché en tooltip dans le badge sources RAG
- **Contournement de bugs des modèles GPT** — lorsque l'utilisateur demande la production d'un fichier Word avec des graphiques et qu'il utilise un modèle GPT, Demeter bascule automatiquement sur un modèle Mistral le temps de la réponse

### Amélioré

- **Sources RAG** — les chunks sont maintenant regroupés par fichier source dans le badge ; le compteur affiche le nombre d'extraits et le nombre de fichiers distincts ; les scores (vectoriel et rerank) sont visibles en tooltip sur chaque pastille d'index
- **Thème Mermaid dynamique** — les diagrammes s'adaptent en temps réel au thème clair/sombre du système via `matchMedia` ; les couleurs utilisent les variables CSS de l'application
- **Police des diagrammes** — les labels Mermaid et les diagrammes ECharts héritent désormais de `var(--font-body)` pour rester cohérents avec le reste de l'interface
- **Fenêtre de contexte des modèles** — la liste des modèles récupère et expose le champ `max_context_length`, transmis au mode Comparaison
- **Couleurs d'erreur** — les valeurs hexadécimales codées en dur (`#fef2f2`, `#fca5a5`, `#b91c1c`) sont remplacées par les variables CSS `--red-light` et `--red-dark` dans `App.css` et `MarkdownComponents.tsx`

### Corrigé

- **TypeScript** — prop `onMcpClick` manquante sur `<CompareModeView>` dans `App.tsx`
- **TypeScript** — `findLast` indisponible en ES2020 : `lib` dans `tsconfig.json` passée de `ES2020` à `ES2023`
- **Organisation** — `CompareMode.css` déplacé de `src/components/` vers `src/` pour respecter la convention du projet ; import mis à jour dans `App.tsx`

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
