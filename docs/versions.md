# Versions et mises à jour

Repris de Lunas Mail. Les versions sont publiées dans les **versions GitHub du dépôt**
(public), avec `latest.json` pour le module de mise à jour de l'app.

## Publier une version

```sh
bun run release 0.2.0          # ajouter --edit pour relire les notes
```

`scripts/release.sh` met le numéro dans `package.json`, `Cargo.toml` (racine) et
`src-tauri/tauri.conf.json`, ajoute en tête de `CHANGELOG.md` les nouveautés depuis la
version précédente (en anglais, rédigées par Claude Code s'il est installé, sinon à écrire
dans l'éditeur), commite, crée le tag `v0.2.0` et pousse. Le workflow `release.yml`
construit alors :

| Plateforme | Paquets | Mise à jour automatique |
|---|---|---|
| macOS (Apple Silicon) | `.app`, `.dmg` | oui (`.app.tar.gz`) |
| Linux | AppImage, `.deb`, `.rpm` | AppImage seulement |
| Windows | `.msi` | oui |

macOS : signature ad hoc, pas encore de Developer ID ni de notarisation ; au premier
téléchargement, macOS demande de lever la quarantaine.

## Mises à jour dans l'app

`src/stores/updates.ts` : vérification au lancement puis toutes les 6 h (jamais en
`tauri dev`). Bandeau « Lunas PDF x.y.z est disponible » avec les nouveautés (section du
CHANGELOG reprise dans `latest.json`) ; « Mettre à jour et redémarrer » propose d'abord
d'enregistrer les documents modifiés. Vérification manuelle dans les Préférences.

Les mises à jour sont signées (minisign) : la clé publique est dans
`src-tauri/tauri.conf.json` (`plugins.updater.pubkey`), la clé privée dans les secrets du
dépôt. `createUpdaterArtifacts` n'est activé que par `src-tauri/tauri.release.conf.json`,
pour que les builds de la CI n'aient pas besoin de la clé.

## Secrets du dépôt

| Secret | Contenu |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | contenu de `~/.tauri/lunas-pdf.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | contenu de `~/.tauri/lunas-pdf.key.password` |

Perdre la clé privée empêche de publier des mises à jour acceptées par les versions déjà
installées : la garder aussi dans un gestionnaire de mots de passe.
