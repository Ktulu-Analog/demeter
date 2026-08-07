# Serveurs MCP dans Demeter — Guide d'installation, de configuration et d'utilisation

Ce guide explique comment activer, configurer et utiliser des serveurs MCP dans Demeter — que ce soient les MCP **officiels** fournis avec l'application (Judilibre, Grist, Légifrance, Système de fichiers) ou des MCP **tiers/personnalisés**.

Aucune compétence technique particulière n'est requise pour la partie installation/configuration : tout se fait depuis le panel **Serveurs MCP** de Demeter, en quelques clics.

---

## Sommaire

1. [C'est quoi un MCP, au juste ?](#1-cest-quoi-un-mcp-au-juste-)
2. [Démarrage rapide (3 minutes)](#2-démarrage-rapide-3-minutes)
3. [Installation détaillée, pas à pas](#3-installation-détaillée-pas-à-pas)
4. [Configurer chaque MCP officiel](#4-configurer-chaque-mcp-officiel)
5. [Utiliser les MCP dans une conversation](#5-utiliser-les-mcp-dans-une-conversation)
6. [Ajouter un serveur MCP distant (HTTP)](#6-ajouter-un-serveur-mcp-distant-http)
7. [Ajouter un MCP tiers ou personnalisé](#7-ajouter-un-mcp-tiers-ou-personnalisé)
8. [Dépannage — FAQ](#8-dépannage--faq)
9. [Sécurité et confidentialité](#9-sécurité-et-confidentialité)
10. [Annexe — référence technique](#10-annexe--référence-technique)

---

## 1. C'est quoi un MCP, au juste ?

Un **MCP** (*Model Context Protocol*) est un petit programme qui donne au LLM de nouvelles capacités : consulter une base de jurisprudence, lire un tableur Grist, chercher un texte de loi, lire/écrire un fichier sur votre ordinateur, etc. Demeter s'appuie sur eux pour aller chercher de l'information ou effectuer des actions que le LLM seul ne sait pas faire.

Il existe deux façons de connecter un MCP à Demeter :

| Type | Fonctionnement | Ce qu'il faut faire |
|---|---|---|
| **MCP stdio** *(ce guide)* | Un petit programme lancé automatiquement par Demeter en arrière-plan | Rien à héberger, rien à lancer manuellement — juste le configurer une fois |
| **MCP distant (HTTP)** | Un serveur qui tourne ailleurs (chez vous, sur un serveur d'entreprise…) | Il faut l'héberger vous-même et renseigner son URL dans Demeter |

Ce guide couvre principalement les **MCP stdio**, plus simples à mettre en œuvre — c'est la solution recommandée pour la plupart des usages. La section 6 couvre brièvement les MCP distants.

Demeter fournit 4 MCP stdio **officiels**, prêts à l'emploi :

| MCP | Ce qu'il permet | Nécessite une clé ? |
|---|---|---|
| **Judilibre** | Rechercher des décisions de justice (Cour de cassation, cours d'appel, tribunaux judiciaires) | Oui (gratuite, PISTE) |
| **Grist** | Lire/écrire dans vos tableurs Grist (organisations, documents, tables, enregistrements) | Oui (votre compte Grist) |
| **Légifrance** | Consulter codes, lois, Journal Officiel, conventions collectives, jurisprudence administrative | Oui (gratuite, PISTE) |
| **Système de fichiers** | Lire/écrire des fichiers dans votre dossier personnel (HOME) | Non |

---

## 2. Démarrage rapide (3 minutes)

Pour les utilisateurs pressés :

1. Ouvrez le panel **Serveurs MCP** — menu **Espaces → Serveurs MCP…**, ou raccourci `Ctrl+M` (`Cmd+M` sur Mac), ou bouton **MCP** en haut de l'écran.
2. Cliquez sur **Choisir un dossier…** et sélectionnez (ou créez) un dossier vide, par exemple `Documents/MCP Demeter`.
3. Cliquez sur **Installer les MCP officiels** — Demeter y copie automatiquement Judilibre, Grist, Légifrance et Système de fichiers.
4. Cochez les serveurs que vous voulez utiliser. Pour ceux qui le demandent (Judilibre, Grist, Légifrance), cliquez sur **Configurer** et renseignez la ou les clés d'accès (voir [section 4](#4-configurer-chaque-mcp-officiel) pour savoir où les obtenir).
5. Fermez le panel — c'est prêt. Posez une question en lien avec un des MCP activés, Demeter s'en sert automatiquement si c'est pertinent.

---

## 3. Installation détaillée, pas à pas

### 3.1 Ouvrir le panel MCP

Trois façons d'y accéder :
- Bouton **🔌 MCP** dans la barre du haut de Demeter ;
- Menu **Espaces → Serveurs MCP…** ;
- Raccourci clavier `Ctrl+M` (Windows/Linux) ou `Cmd+M` (macOS).

### 3.2 Choisir un dossier pour vos serveurs MCP

C'est la première chose à faire : Demeter a besoin de savoir **où** chercher les serveurs MCP stdio sur votre ordinateur.

- Cliquez sur **Choisir un dossier…** en haut du panel.
- Sélectionnez un dossier existant, ou créez-en un nouveau (par exemple `Documents/MCP Demeter`, ou `~/mcp-servers`). N'importe quel emplacement convient, tant que vous avez le droit d'y écrire.
- Ce dossier peut être vide au départ : c'est vous (via le bouton d'installation, ou manuellement) qui allez y déposer des serveurs.

> 💡 Ce choix n'est à faire qu'une seule fois. Demeter s'en souvient et rouvre automatiquement ce dossier à chaque démarrage.

### 3.3 Installer les MCP officiels

Une fois le dossier choisi, le bouton **Installer les MCP officiels** apparaît. Il copie dans votre dossier les 4 MCP fournis avec Demeter (Judilibre, Grist, Légifrance, Système de fichiers), avec le bon binaire pour votre système d'exploitation (Windows, macOS ou Linux — détecté automatiquement).

- Ce bouton ne remplace jamais un MCP déjà présent dans le dossier : vos éventuels réglages ne sont jamais écrasés.
- Vous pouvez le recliquer après une mise à jour de Demeter pour récupérer une nouvelle version d'un MCP officiel, ou un MCP officiel ajouté depuis votre dernière installation.

### 3.3 bis Garder les MCP officiels à jour sans réinstaller Demeter

Un bouton **Mettre à jour (vX.Y.Z)** apparaît automatiquement dans le panel dès qu'une nouvelle version des MCP officiels est disponible en ligne — inutile d'attendre une mise à jour de Demeter pour en profiter. Un clic télécharge la nouvelle version, vérifie son intégrité (empreinte SHA-256) puis l'installe, sans jamais toucher à vos clés déjà renseignées. Si vous êtes hors-ligne, Demeter continue de fonctionner normalement avec la version déjà installée.

### 3.4 Vérifier que les serveurs sont détectés

Une fois l'installation terminée, la section **Serveurs découverts** liste les MCP trouvés dans votre dossier, avec pour chacun :
- son nom et sa description ;
- une case à cocher pour l'activer ;
- un bouton **Configurer** s'il attend des paramètres (clé d'API…) ;
- un indicateur **⚠** si des paramètres obligatoires manquent encore ;
- la mention *Indisponible sur cette plateforme* si aucun binaire n'existe pour votre OS (rare, uniquement pour des MCP tiers mal empaquetés).

---

## 4. Configurer chaque MCP officiel

Cliquez sur **Configurer** sur la carte du serveur concerné pour faire apparaître son formulaire. Une fois les champs remplis, cliquez sur **Enregistrer**.

### Judilibre (jurisprudence judiciaire)

| Champ | Description |
|---|---|
| Client ID PISTE | Identifiant de votre application PISTE |
| Client Secret PISTE | Secret associé (champ masqué) |

**Où les obtenir :**
1. Créez un compte sur [piste.gouv.fr](https://piste.gouv.fr).
2. Créez une application et abonnez-la à l'API **JUDILIBRE**.
3. Récupérez le *Client ID* et le *Client Secret* de votre application dans son tableau de bord PISTE.

C'est gratuit et généralement instantané pour l'accès en lecture.

### Grist (tableur collaboratif)

| Champ | Description |
|---|---|
| Clé API Grist | Votre clé personnelle Grist (champ masqué) |
| URL de l'instance Grist | Laissez vide pour utiliser `https://docs.getgrist.com` (Grist Cloud), ou indiquez l'URL de votre instance auto-hébergée |

**Où l'obtenir :** connectez-vous à Grist → cliquez sur votre avatar → **Préférences du compte** → **Clés API** → générez une clé.

### Légifrance (droit français)

| Champ | Description |
|---|---|
| Client ID PISTE | Identifiant de votre application PISTE |
| Client Secret PISTE | Secret associé (champ masqué) |

Même démarche que pour Judilibre, mais en abonnant votre application PISTE à l'API **Légifrance** (vous pouvez utiliser la même application PISTE pour les deux, en l'abonnant aux deux API).

### Système de fichiers

Aucune configuration nécessaire — pas de clé d'API. Il suffit de cocher la case pour l'activer.

⚠️ **Point d'attention** : ce MCP donne au LLM un accès en **lecture et en écriture** à votre dossier personnel (HOME). Les chemins système sensibles sont protégés côté serveur, mais réfléchissez avant de l'activer si vous ne voulez pas que le LLM puisse consulter ou modifier vos fichiers.

---

## 5. Utiliser les MCP dans une conversation

Une fois un MCP activé (et configuré si nécessaire), il n'y a rien de plus à faire : Demeter transmet automatiquement au LLM la liste des actions disponibles, et celui-ci les utilise de lui-même quand c'est pertinent pour répondre à votre question.

Exemples de questions qui déclenchent naturellement un MCP :

- *« Trouve-moi les décisions récentes de la Cour de cassation sur le licenciement pour faute grave »* → Judilibre
- *« Quelles sont les lignes de mon tableau Grist "Budget 2026" dont le montant dépasse 5000 € ? »* → Grist
- *« Que dit l'article L.1234-1 du Code du travail ? »* → Légifrance
- *« Résume-moi le contenu du fichier rapport.docx dans mon dossier Documents »* → Système de fichiers

Vous pouvez vérifier à tout moment quels MCP sont actifs et quels outils ils exposent via l'icône de statut MCP dans la conversation (info-bulle listant les serveurs connectés et leurs outils).

> 💡 Le premier appel à un MCP peut prendre une seconde ou deux de plus que les suivants : Demeter démarre le process à la demande, puis le garde actif pour les appels suivants (pas de redémarrage à chaque message).

---

## 6. Ajouter un serveur MCP distant (HTTP)

Si vous (ou votre organisation) hébergez déjà un serveur MCP accessible par HTTP, vous pouvez l'ajouter dans la section **MCP distants (HTTP)**, en bas du panel :

1. Renseignez son URL (ex. `http://localhost:3001/sse` ou une URL distante).
2. Donnez-lui un nom (optionnel, pour l'identifier plus facilement).
3. Cliquez sur **Ajouter**.

Contrairement aux MCP stdio, Demeter ne gère pas le cycle de vie de ces serveurs : ils doivent déjà être en cours d'exécution et joignables au moment où vous les utilisez.

---

## 7. Ajouter un MCP tiers ou personnalisé

Vous pouvez ajouter n'importe quel serveur MCP compatible stdio dans votre dossier, du moment qu'il respecte une convention simple.

### 7.1 Structure attendue

Dans le dossier que vous avez choisi (section 3.2), chaque serveur est un sous-dossier, par convention nommé `mcp-<nom>` :

```
Documents/MCP Demeter/
├── mcp-judilibre/
│   ├── manifest.json
│   └── bin/
│       └── mcp-judilibre-<plateforme>[.exe]
├── mcp-grist/
│   ├── manifest.json
│   └── bin/…
└── mcp-mon-serveur-perso/       ← votre MCP personnalisé
    ├── manifest.json
    └── bin/
        └── mcp-mon-serveur-perso-linux-x64
```

### 7.2 Le fichier `manifest.json`

```json
{
  "id": "mcp-mon-serveur-perso",
  "name": "Mon serveur perso",
  "description": "Ce que fait ce serveur, en une phrase.",
  "bin": {
    "linux-x64": "bin/mcp-mon-serveur-perso-linux-x64",
    "macos-x64": "bin/mcp-mon-serveur-perso-macos-x64",
    "macos-arm64": "bin/mcp-mon-serveur-perso-macos-arm64",
    "windows-x64": "bin/mcp-mon-serveur-perso-windows-x64.exe"
  },
  "args": ["--transport", "stdio"],
  "params": [
    { "key": "MON_API_KEY", "label": "Clé API", "required": true, "secret": true, "help": "Où l'obtenir…" }
  ]
}
```

| Champ | Obligatoire | Description |
|---|---|---|
| `id` | ✅ | Identifiant unique — utilisez le nom du dossier |
| `name` | ✅ | Nom affiché dans le panel |
| `description` | — | Description affichée sous le nom |
| `bin` | ✅ | Chemin du binaire, par plateforme (`linux-x64`, `macos-x64`, `macos-arm64`, `windows-x64`) — seul le binaire de votre plateforme est requis, les autres peuvent être omis |
| `args` | — | Arguments passés au lancement |
| `params` | — | Champs de configuration affichés dans le panel (voir ci-dessous) |

Chaque entrée de `params` accepte : `key` (nom de la variable d'environnement transmise au process), `label` (libellé affiché), `secret` (masque le champ), `required` (marque le champ comme obligatoire), `default` (indication affichée en filigrane), `help` (texte d'aide sous le champ).

### 7.3 Contraintes techniques du binaire

Pour qu'un serveur MCP fonctionne en stdio avec Demeter, son binaire doit :
- lire les requêtes JSON-RPC **une par ligne** sur son entrée standard (stdin) ;
- écrire ses réponses JSON-RPC **une par ligne** sur sa sortie standard (stdout) ;
- écrire tous ses journaux/logs sur la sortie d'erreur (**stderr**), jamais sur stdout — cela casserait la communication.

Une fois le dossier ajouté avec son `manifest.json` et son binaire, rouvrez le panel MCP (ou fermez/rouvrez-le) : votre serveur apparaît automatiquement dans la liste des serveurs découverts.

---

## 8. Dépannage — FAQ

**Le bouton "Configurer" ne s'affiche pas pour un serveur.**
→ Ce serveur n'attend aucun paramètre (`params` vide dans son manifest) — c'est normal, par exemple pour le MCP Système de fichiers.

**Un serveur affiche "Indisponible sur cette plateforme".**
→ Son manifest ne fournit pas de binaire pour votre système d'exploitation/architecture. Contactez l'auteur du MCP, ou compilez-le vous-même pour votre plateforme.

**J'ai coché un serveur mais le LLM ne semble pas l'utiliser.**
→ Vérifiez qu'il n'affiche pas l'icône ⚠ (paramètres obligatoires manquants). Vérifiez aussi que votre question appelle réellement l'un de ses outils — le LLM ne déclenche un MCP que s'il le juge pertinent pour répondre.

**J'ai changé une clé d'API mais rien ne se passe.**
→ Après avoir cliqué sur **Enregistrer** dans le formulaire de configuration, le serveur redémarre automatiquement au prochain appel avec la nouvelle valeur — inutile de relancer Demeter.

**J'ai changé de dossier MCP et mes serveurs ont disparu.**
→ C'est normal : Demeter ne regarde que dans le dossier actuellement sélectionné. Reconfigurez ou réinstallez-y vos serveurs (bouton **Installer les MCP officiels** pour les officiels), ou resélectionnez votre ancien dossier.

**Un serveur reste inactif malgré une configuration correcte.**
→ Décochez-le puis recochez-le pour forcer un redémarrage. Si le problème persiste, le binaire est peut-être corrompu ou incompatible avec votre système — réinstallez-le.

---

## 9. Sécurité et confidentialité

- Le dossier choisi, la liste des serveurs activés et les valeurs de configuration (clés d'API…) sont enregistrés **uniquement en local**, dans les préférences de Demeter — ils ne sont jamais transmis à un tiers.
- Les serveurs MCP stdio tournent en tant que **sous-processus de Demeter**, sur votre machine : aucune donnée ne transite par un serveur externe, sauf celle que le MCP lui-même contacte pour fonctionner (ex. l'API Judilibre, l'API Grist).
- Le MCP **Système de fichiers** est le seul à donner un accès direct à vos fichiers locaux (lecture/écriture, restreintes à votre dossier personnel) — activez-le en connaissance de cause.
- N'installez un MCP tiers que si vous faites confiance à sa source : comme tout programme exécuté sur votre machine, un MCP malveillant pourrait causer du tort. Demeter ne peut pas vérifier le contenu des MCP tiers que vous ajoutez vous-même.

---

## 10. Annexe — référence technique

### Détection de plateforme

Demeter choisit automatiquement le bon binaire selon votre système, avec les clés suivantes dans `manifest.json → bin` :

| Système | Clé |
|---|---|
| Linux 64 bits | `linux-x64` |
| macOS Intel | `macos-x64` |
| macOS Apple Silicon | `macos-arm64` |
| Windows 64 bits | `windows-x64` |

### Cycle de vie d'un process MCP stdio

- Démarré **à la demande**, au premier appel d'un outil de ce serveur.
- Conservé actif entre deux messages (pas de redémarrage systématique).
- Arrêté automatiquement à la fermeture de Demeter.
- Redémarré automatiquement si vous modifiez sa configuration (nouvelle clé d'API…) ou changez de dossier MCP.

### Récapitulatif des MCP officiels

| MCP | Identifiant (`id`) | Paramètres requis |
|---|---|---|
| Judilibre | `mcp-judilibre` | `JUDILIBRE_CLIENT_ID`, `JUDILIBRE_CLIENT_SECRET` |
| Grist | `mcp-grist` | `GRIST_API_KEY` (+ `GRIST_BASE_URL` optionnel) |
| Légifrance | `mcp-legifrance` | `LEGIFRANCE_CLIENT_ID`, `LEGIFRANCE_CLIENT_SECRET` |
| Système de fichiers | `mcp-system` | Aucun |

---

*Ce guide couvre le panel MCP de Demeter. Pour toute question ne trouvant pas de réponse ici, consultez la documentation technique dans le dépôt de code de Demeter, ou contactez votre administrateur.*
