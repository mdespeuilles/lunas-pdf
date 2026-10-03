# Lunas PDF

Lecteur et éditeur PDF pour Linux, macOS et Windows : simple, rapide, beau. Tauri 2 (Rust) + Vue 3.

- Architecture : [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Choix du moteur PDF : [docs/ADR-001-moteur-pdf.md](docs/ADR-001-moteur-pdf.md)
- Avancement : [docs/ROADMAP.md](docs/ROADMAP.md)

## Développement

Prérequis : Rust stable, Bun 1.3+, Node 24+ (Vitest et Playwright), et sous Linux les bibliothèques de WebKitGTK 4.1 (`webkit2gtk-4.1`).

```sh
bun install
bun pdfium           # télécharge libpdfium (version épinglée) dans src-tauri/pdfium/
bun tauri dev        # lance l'app ; `bun tauri dev -- -- fichier.pdf` pour ouvrir un fichier
```

Tests :

```sh
cargo test --workspace                    # moteur + app (régénère src/bindings.ts)
bun run test                              # unitaires (Vitest)
bun run test:e2e --project=chromium       # parcours UI (Playwright, backend simulé)
```

Paquets : `bun tauri build --bundles appimage,deb,rpm` (ou `dmg`, `msi`).

Les PDF de test se régénèrent avec `fixtures/scripts/` (voir l'en-tête des scripts).
