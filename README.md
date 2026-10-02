# Feuillet

Lecteur et éditeur PDF pour Linux, macOS et Windows : simple, rapide, beau. Tauri 2 (Rust) + Vue 3.

- Architecture : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Choix du moteur PDF : [docs/ADR-001-moteur-pdf.md](docs/ADR-001-moteur-pdf.md)
- Avancement : [docs/ROADMAP.md](docs/ROADMAP.md)

## Développement

Prérequis : Rust stable, Node 24+, pnpm, et sous Linux les bibliothèques de WebKitGTK 4.1 (`webkit2gtk-4.1`).

```sh
pnpm install
pnpm pdfium          # télécharge libpdfium (version épinglée) dans src-tauri/pdfium/
pnpm tauri dev       # lance l'app ; `pnpm tauri dev -- -- fichier.pdf` pour ouvrir un fichier
```

Tests :

```sh
cargo test --workspace                    # moteur + app (régénère src/bindings.ts)
pnpm test                                  # unitaires (Vitest)
pnpm test:e2e --project=chromium           # parcours UI (Playwright, backend simulé)
```

Paquets : `pnpm tauri build --bundles appimage,deb,rpm` (ou `dmg`, `msi`).

Les PDF de test se régénèrent avec `fixtures/scripts/` (voir l'en-tête des scripts).
