# Changelog

Toutes les modifications notables de ce projet sont documentées dans ce fichier.

Le format suit [Keep a Changelog](https://keepachangelog.com/fr/1.0.0/),
et ce projet adhère au [Versioning Sémantique](https://semver.org/lang/fr/).

---

## [2.4.5] — 2026-10-11

### Sécurité

- **API locale authentifiée** — le serveur HTTP interne exige désormais un jeton secret (`X-Demeter-Token`, 244 bits, régénéré à chaque lancement et transmis au frontend par la commande IPC `get_api_token`, jamais par HTTP), vérifie l'en-tête `Host` (anti DNS rebinding) et l'en-tête `Origin`, et limite CORS aux origines de l'application. Seul `GET /health` reste public. Nouveau module `security.rs` (avec tests unitaires) ; le frontend ajoute le jeton via `utils/api-auth.ts`.
- **Jeton Albert** — les routes `/users/me`, `/models` et `/audio/transcriptions` n'utilisent plus le paramètre `endpoint` de la requête ni l'en-tête `X-Albert-Token` : seuls l'endpoint et le jeton enregistrés par `init_credentials` sont utilisés. L'endpoint doit être en `http(s)://`.
- **Serveurs MCP** — le backend n'accepte plus que les serveurs MCP de la configuration enregistrée (liste blanche transmise par `init_credentials`) pour `/api/chat` et `/api/mcp/tools`.
- **Exécution de code** — CSP stricte activée (`csp` et `devCsp` dans `tauri.conf.json`) ; `new Function` supprimé : les blocs `echarts` sont analysés par `parseEChartsOption` (JSON uniquement, aucune exécution).
- **Images distantes** — suppression de l'ancienne route `/api/image-proxy` (SSRF) et de la consigne `IMAGE_INSTRUCTION` du prompt. Une image distante n'est plus jamais chargée automatiquement (exfiltration par URL d'image) : elle s'affiche comme un bouton « Charger depuis <hôte> ». Au clic, le backend la télécharge via `/api/image-fetch` (module `image_fetch.rs` : adresses publiques uniquement, DNS épinglé, redirections revalidées, ports 80/443, types raster, 8 Mo max) et le frontend l'affiche en `blob:`. Les images MCP en base64 (ex. ComfyUI) s'affichent comme avant.
- **SVG** — assainissement par listes blanches sur le DOM (`utils/svg-sanitizer.ts`) à la place des expressions régulières.
- **Mermaid** — `securityLevel` passé de `loose` à `strict`.
- **Ressources tierces** — polices (`@fontsource/*`) et feuille de style KaTeX désormais embarquées : plus aucun appel à Google Fonts ni jsDelivr.
- **MCP HTTP** — cibles link-local / métadonnées cloud refusées (y compris après résolution DNS), redirections limitées à 5 sans rétrogradation https→http ni vers une adresse interdite, réponses plafonnées à 25 Mo (JSON et flux SSE), corps d'erreur tronqués dans les journaux. Les serveurs locaux et de réseau privé restent autorisés.
- **MCP stdio** — le binaire doit rester dans le dossier du serveur (chemins absolus, `..` et liens symboliques sortants refusés) ; environnement du processus vidé puis limité à une liste blanche (PATH, HOME, proxys, certificats…) + paramètres déclarés dont le nom est sûr (`LD_*`, `NODE_OPTIONS`, `PYTHONPATH`… refusés) ; lignes stdout/stderr plafonnées ; stderr passé en `debug` ; liens symboliques non copiés à l'installation.
- **Pack MCP** — étiquette de release validée (numéro de version), dossier temporaire à nom aléatoire en 0700, téléchargement plafonné (300 Mo), extraction ZIP durcie (zip slip, liens symboliques, nombre d'entrées et taille décompressée).
- **Routage MCP** — slug de serveur unique (hachage de l'URL complète) et routage exact ; un outil inconnu n'est plus envoyé au premier serveur.
- **Qdrant** — noms de collection validés (400) et encodés dans les URL.
- **Découpage** — `chunk_text` ne boucle plus avec `chunk_size = 0` ; `chunk_size` borné à 128–8192 et recouvrement à la moitié.
- **Extraction** — troncature sur frontière UTF-8 (plus de panique), DOCX plafonné à 64 Mo décompressés, accents DOCX conservés (bug d'encodage corrigé), extraction hors du thread asynchrone avec délai de 120 s.
- **Jetons Qdrant** — durée par défaut 90 jours, bornée entre 60 s et 1 an ; calcul d'expiration sans débordement.
- **Clés et jetons Qdrant** — Demeter refuse de générer un jeton partageable si la clé d'administration fait moins de 32 caractères ou moins de 10 caractères distincts (un JWT HS256 permet une attaque par dictionnaire hors ligne sur la clé de signature) ; procédure de rotation et exigences dans `docs/SECURITE_QDRANT_JWT.md` ; commentaire explicite sur le décodage JWT côté interface (affichage seulement).
- **Erreurs et journaux** — les erreurs 5xx renvoyées au client sont assainies (première ligne, URL masquées, 240 caractères) ; le détail complet reste dans les journaux ; les noms de fichiers ingérés passent en `debug`.
- **Port de l'API** — le port est désormais réservé AVANT l'ouverture de l'interface ; s'il est déjà pris, Demeter affiche un message d'erreur et s'arrête (l'interface n'envoie ainsi jamais son jeton à un autre programme qui écouterait sur ce port). Le `.env` n'est lu qu'en debug (lot 2).
- **Vulnérabilités Rust (premier `cargo audit`)** — 9 vulnérabilités trouvées et traitées : `lopdf` (dépassement de pile sur un PDF malveillant, qui aurait fait tomber tout le processus) corrigée par `pdf-extract` 0.12 (test d'extraction d'un vrai PDF ajouté) ; `h2`, `quinn-proto`, `rustls`, `anyhow`, `spin` mis à jour ; `quick-xml` 0.36, inutilisée, supprimée ; deux exceptions justifiées dans `src-tauri/.cargo/audit.toml` (`rsa` sans correctif et jamais utilisée : HS256 seulement ; `quick-xml` 0.39 : dépendance de build de Tauri). Il reste 8 avertissements « non maintenue » transitifs via Tauri.
- **Dépendances, CI, signature** — `serde_yaml` (archivé) remplacé par `serde_norway` (fork maintenu, même API) ; `npm audit fix` complet (reste 5 alertes « low » KaTeX, non exploitables : l'option `trust` n'est pas utilisée) ; actions GitHub épinglées par SHA de commit dans `ci.yml` et `release.yml` (rust-toolchain avec `toolchain: stable` explicite), `permissions: contents: read` sur le CI ; nouveau workflow `audit.yml` (npm audit niveau « high » + cargo audit, hebdomadaire et sur modification des dépendances) ; procédure de signature des installeurs dans `docs/SIGNATURE_INSTALLEURS.md`.
- **Contrôle d'accès aux collections (option A)** — l'ACL locale est désormais documentée comme un confort d'organisation : la protection réelle est l'authentification Qdrant (JWT scopés ; voir `docs/SECURITE_COLLECTIONS.md`). Écritures (ajout/suppression de documents, renommage, suppression, visibilité) réservées à l'administrateur ou au propriétaire identifié ; une collection **externe** (sans métadonnées) ou **orpheline** (propriétaire vide ou « unknown ») n'est plus modifiable que par un administrateur, qui peut l'« adopter » ; plus de propriétaire « unknown » (sans e-mail connu, un non-administrateur ne crée plus de collection) ; une erreur de lecture des métadonnées **refuse** l'accès au lieu de l'ouvrir ; nom de collection validé pour `DELETE …/documents/:id`. Les collections externes restent **lisibles** (Qdrant filtre via le JWT). Logique centralisée dans `can_read_collection` / `can_write_collection` ; 5 tests.
- **Authenticité du pack MCP (option A : signature minisign)** — la mise à jour en ligne du pack exige désormais que `checksums.txt` soit signé (`checksums.txt.minisig`) avec la clé publique embarquée `src-tauri/mcp-pack.pub`, et que le commentaire signé contienne `version:<version>` (refus du rejeu d'une ancienne release). **Tant qu'aucune clé n'est configurée, la mise à jour en ligne est refusée** (le pack embarqué reste utilisable) ; dérogation explicite `DEMETER_ALLOW_UNSIGNED_PACK=1`. Nouveau script `scripts/sign-mcp-pack.sh`, vérification optionnelle dans `scripts/fetch-mcp-pack.sh`, procédure dans `docs/SECURITE_PACK_MCP.md`. 6 tests (signature valide, données modifiées, autre clé, rétrogradation, commentaire falsifié). Nouvelle dépendance : `minisign-verify`.
- **Secrets dans le trousseau du système** — le jeton Albert, la clé Qdrant, les clés d'API des serveurs MCP (`mcp_stdio_config`), les URL de serveurs MCP et l'URL de recherche web ne sont plus stockés dans le `localStorage` : ils vivent dans le trousseau (Credential Manager / Keychain / Secret Service) via les commandes IPC `secrets_get` / `secrets_set` (nom d'entrée fixe, le frontend ne peut lire aucun autre secret). Les valeurs longues sont découpées en morceaux pour respecter la limite de Windows. **Migration automatique** : au premier lancement, les secrets de l'ancien `localStorage` sont copiés dans le trousseau puis effacés. **Aucun repli en clair** : si le trousseau est indisponible, une erreur est affichée, les secrets restent en mémoire pour la session et ne sont pas réécrits dans le `localStorage`.
- **Base de conversations chiffrée (SQLCipher)** — `conversations.db` est chiffrée avec une clé aléatoire de 256 bits générée au premier lancement et conservée dans le trousseau (jamais exposée au frontend). Une base existante en clair est migrée automatiquement : export chiffré, **vérification** (nombre de conversations et de métadonnées), puis remplacement. L'ancienne base est conservée en `conversations.db.pre-encryption.bak` (0600) le temps d'un lancement, puis **écrasée et supprimée** au lancement suivant. Les droits du fichier sont restreints (0600 sous Unix). OpenSSL est compilé depuis les sources (`bundled-sqlcipher-vendored-openssl`) : Perl et un compilateur C sont requis pour compiler.
- **Mode dégradé explicite** — si la clé ne peut pas être obtenue (trousseau indisponible, clé introuvable, migration impossible), l'application démarre avec une base **en mémoire** (rien n'est détruit sur le disque) et affiche l'avertissement. Pour les postes sans trousseau (Linux sans Secret Service) : variable d'environnement `DEMETER_DB_KEY` (≥ 16 caractères) ou, en dernier recours, `DEMETER_ALLOW_PLAINTEXT_DB=1` (base non chiffrée, signalée à chaque lancement). Ces variables doivent être définies dans l'environnement du processus : le fichier `.env` n'est plus lu en production.
- Nouvelle commande IPC `get_security_status` (base chiffrée, persistante, trousseau disponible, avertissements).
**Développement dans un navigateur (hors Tauri)** : définir le même jeton des deux côtés — `DEMETER_API_TOKEN` (≥ 16 caractères, build de debug du backend uniquement) et `VITE_DEMETER_API_TOKEN` (frontend).

---
## [2.4.4] — non publiée
## [2.4.3] — non publiée
## [2.4.2] — non publiée
## [2.4.1] — non publiée

---

## [2.4.0] — 2026-08-06

### Ajouté

- **Transcription audio avec séparation des locuteurs** — nouvelle modale de transcription (API Albert `/v1/audio/transcriptions`) avec découpage automatique des fichiers longs (`audioChunking`), réconciliation des locuteurs entre segments (`speakerReconciliation`) et fusion des tours de parole consécutifs d'un même locuteur
- **Synthèse au format Word** — export `.docx` du transcript complet (locuteurs en gras) ainsi que des synthèses générées, via des prompts de résumé configurables (`audio_summary_prompts`)
- **MCP stdio** — Demeter peut désormais lancer et gérer des serveurs MCP en sous-processus (transport stdio), en complément du transport HTTP Streamable existant ; configuration via le panel **Serveurs MCP** (`Ctrl+M` / `Cmd+M`)
- **Installation automatique du pack MCP officiel** — récupération, vérification (checksum SHA-256) et déballage automatiques du pack [`demeter-mcp-pack`](https://github.com/Ktulu-Analog/demeter-mcp-pack) (Judilibre, Grist, Légifrance, Système de fichiers) via `scripts/fetch-mcp-pack.sh`, la version embarquée étant fixée par `MCP_PACK_VERSION` ; un bouton **Vérifier les mises à jour** dans le panel MCP permet une mise à jour ponctuelle du pack sans attendre une nouvelle version de Demeter

### Modifié

- **Menu « Ingestion RAG… » renommé en « Collections RAG… »** pour rester cohérent avec le titre déjà utilisé dans la modale elle-même

### Sécurité

- **ECharts 5.5.1 → 6.1.0** — corrige une vulnérabilité XSS modérée ([GHSA-fgmj-fm8m-jvvx](https://github.com/advisories/GHSA-fgmj-fm8m-jvvx)) ; changement majeur mais sans impact sur l'usage qu'en fait Demeter (`echarts.init` + `EChartsOption`), le thème par défaut évolue légèrement (position de la légende, palette) — voir le [guide de migration v5→v6](https://echarts.apache.org/handbook/en/basics/release-note/v6-upgrade-guide/) si besoin de restaurer l'ancien rendu

### Corrigé

- **Formatage `cargo fmt`** sur `api.rs`, `mcp_pack.rs` et `mcp_stdio.rs`
- **Code mort** — suppression de `mcp::collect_all_tools` (HTTP uniquement), devenue obsolète et non utilisée depuis son remplacement par `collect_all_tools_unified` (HTTP + stdio) dans `api.rs`
- **Mise à jour du pack MCP officiel échouait** (`Aucune entrée pour '<archive>.zip' dans checksums.txt`) — `verify_checksum` compare désormais les noms de fichiers en ignorant un éventuel préfixe `./`, produit par certaines versions de `sha256sum ./*.zip` côté [`demeter-mcp-pack`](https://github.com/Ktulu-Analog/demeter-mcp-pack)

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
