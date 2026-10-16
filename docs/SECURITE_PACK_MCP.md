# Sécurité du pack de serveurs MCP officiels

Les serveurs MCP stdio sont des **exécutables lancés avec vos droits** (certains, comme
`mcp-system`, lisent et écrivent dans votre dossier personnel). Leur provenance est donc
une question de sécurité de premier ordre. Ce document décrit la chaîne de confiance et
les opérations à mener (constats F-02 et F-03 de l'audit).

## 1. Chaîne de confiance

| Étape | Contrôle |
|---|---|
| **Build** (`scripts/fetch-mcp-pack.sh`) | Au moins une ancre de confiance gérée dans ce dépôt est **obligatoire** : signature minisign (`src-tauri/mcp-pack.pub`) et/ou empreinte épinglée (`MCP_PACK_SHA256`). Sans aucune des deux, le build échoue. Si les deux existent, les deux sont exigées. |
| **Mise à jour en ligne** (`mcp_pack.rs`) | Exige une signature minisign valide, liée à la version (anti-rétrogradation). Sans clé publique dans l'application, la mise à jour est refusée. Aucune variable d'environnement ne peut lever cette exigence dans une version distribuée. Les téléchargements sont limités à GitHub en HTTPS. |
| **Première utilisation d'un serveur** | L'utilisateur l'autorise (panneau « Serveurs MCP »). Demeter enregistre l'empreinte SHA-256 du binaire et du `manifest.json` dans le trousseau du système. |
| **Chaque démarrage** | Les empreintes sont recalculées. Binaire ou manifeste modifié → le serveur n'est **pas lancé** tant qu'il n'est pas autorisé de nouveau. |
| **Mise à jour du pack officiel** | Les serveurs déjà autorisés sont ré-enregistrés automatiquement (le contenu vient d'un pack vérifié). Un serveur jamais autorisé reste non autorisé. |

> `checksums.txt` publié dans la même release que l'archive ne prouve **pas** l'authenticité :
> qui contrôle la release remplace l'archive *et* ce fichier. C'est pourquoi il faut une ancre
> de confiance extérieure à la release.

## 2. Option A — empreinte épinglée (immédiate)

```bash
./scripts/pin-mcp-pack.sh        # écrit MCP_PACK_SHA256 pour la version de MCP_PACK_VERSION
git diff MCP_PACK_SHA256         # relire
git add MCP_PACK_SHA256 && git commit
```

C'est du *trust on first use* : vérifiez la release sur GitHub avant de committer. À chaque
changement de `MCP_PACK_VERSION`, relancez le script et faites relire le diff comme du code.
Un tag déplacé ou une release remplacée fait échouer le build.

## 3. Option B — signature minisign (recommandée, et seule voie pour la mise à jour en ligne)

1. Générer la paire de clés **hors de GitHub** (poste du mainteneur ou coffre-fort) :
   `minisign -G -p mcp-pack.pub -s ~/.minisign/demeter-mcp-pack.key`
2. Remplacer le contenu de `src-tauri/mcp-pack.pub` par le fichier généré (ligne
   `untrusted comment:` puis clé publique en base64) et committer.
3. À chaque release du pack : `scripts/sign-mcp-pack.sh <version> checksums.txt`, puis publier
   `checksums.txt.minisig` à côté de `checksums.txt`.
4. Protéger le dépôt `demeter-mcp-pack` : 2FA obligatoire, protection de branche et de tags,
   releases immuables, clé privée jamais stockée dans la CI sans coffre.

Ne jamais committer la clé **privée**.

## 4. Autorisation et empreintes des serveurs (F-03)

- Les empreintes sont conservées dans le trousseau (entrée `mcp_trust`), jamais exposées à
  l'interface. Si le trousseau est indisponible, l'autorisation ne vaut que pour la session.
- Le panneau MCP affiche l'état de chaque serveur : **autorisé**, **non autorisé**, **modifié**.
- **Limites assumées** : seuls le binaire désigné et le manifeste sont couverts (un serveur peut
  écrire dans son dossier) ; le contrôle et le lancement ne sont pas atomiques ; **le processus
  n'est pas confiné** (pas de sandbox) — il dispose des droits de l'utilisateur. Un serveur
  autorisé doit donc être considéré comme du code de confiance.
- Les variables d'environnement qu'un manifeste peut déclarer sont filtrées par une liste de
  refus (interpréteurs, chargeur dynamique, proxy et certificats, outils de build).

## 5. Dérogations de développement

| Variable | Effet | Disponibilité |
|---|---|---|
| `MCP_PACK_ALLOW_UNSIGNED=1` | `fetch-mcp-pack.sh` accepte un pack sans clé ni empreinte (message d'alerte) | script de build |
| `DEMETER_ALLOW_UNSIGNED_PACK=1` | Mise à jour en ligne sans clé publique | **build de debug uniquement** |
| `DEMETER_MCP_SKIP_TRUST=1` | Désactive le contrôle d'autorisation des binaires | **build de debug uniquement** |
