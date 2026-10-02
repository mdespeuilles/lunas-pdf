# Architecture de Feuillet

Décision de fond : [ADR-001](ADR-001-moteur-pdf.md). PDFium sert à lire et à afficher. Un écrivain Rust (lopdf, à partir de la phase 2) est la seule source des modifications.

## Vue d'ensemble

```
┌──────────────────────── webview (Vue 3 + Pinia) ────────────────────────┐
│ TitleBar · HomeView · ReaderView ─ Toolbar / Sidebar / DocViewport       │
│                                    └ PageView (canvas + tuiles,          │
│                                       TextLayer, liens, surlignages)      │
│ stores : settings · tabs (état par onglet) · recents · ui                 │
│ lib : layout (pur) · bitmap-cache (LRU 512 Mo) · protocol · page-data     │
└───────────┬───────────────────────────────────────┬──────────────────────┘
            │ commandes typées (tauri-specta)        │ feuillet://localhost/render/…
            │ src/bindings.ts généré                 │ RGBA brut + en-têtes x-w/x-h
┌───────────▼───────────── src-tauri (app) ─────────▼──────────────────────┐
│ commands.rs · protocol.rs · store.rs (préférences, récents, miniatures)  │
│ trousseau (keyring) · instance unique · glisser-déposer · association    │
└───────────┬──────────────────────────────────────────────────────────────┘
            │ Engine (poignée clonable, messages)
┌───────────▼──────────── crates/feuillet-core ────────────────────────────┐
│ engine.rs : acteur, un fil « pdfium » qui possède la bibliothèque         │
│   file de rendu à priorités + époques · recherche incrémentale · caches    │
│ text.rs : regroupement des glyphes en mots, recherche (fonctions pures)    │
│ types.rs : types exportés en TS · error.rs : PdfError                      │
└───────────┬──────────────────────────────────────────────────────────────┘
            │ pdfium-render 0.9.4 (API 7881), chargement dynamique
        libpdfium (bblanchon/pdfium-binaries chromium/7881, embarquée)
```

## Moteur (`crates/feuillet-core`)

- **Acteur PDFium.** PDFium n'est pas thread-safe. Un fil dédié possède donc `Pdfium` (en `'static`) et tous les `PdfDocument`. Les autres fils lui envoient des messages ; les réponses repassent par un canal à usage unique.
- **Ordonnancement.**
  - Les commandes courtes (ouverture, sommaire, texte…) sont traitées dès réception.
  - Les rendus passent par une file `BinaryHeap` triée par priorité, puis par ordre d'arrivée (le plus récent d'abord). Priorités : page visible 200, page dans la marge 100, miniature 20.
  - La recherche avance par tranches de 16 ms entre deux rendus et pousse ses résultats page par page (`SearchEvent`, via un `Channel` Tauri).
- **Époques.** Chaque document a un compteur d'époque. L'interface l'incrémente à chaque zoom ou grand saut de défilement, et les rendus en attente d'une époque antérieure sont abandonnés (HTTP 204 côté protocole). Les miniatures utilisent une époque maximale, qui n'est jamais abandonnée.
- **Géométrie.** Les coordonnées échangées sont en points PDF, dans le repère de la page *affichée* : rotation et CropBox appliquées, origine en haut à gauche. La transformation est déduite une fois par page de `FPDF_PageToDevice` (`DisplayTransform`).
- **Texte.** Les glyphes de chaque page (caractère et boîte) sont mis en cache, 64 pages au plus par document. `build_runs` les regroupe en mots et marque les fins de ligne ; `search_page` gère la casse, et traite espaces et fins de ligne comme équivalents.

## Écriture : annotations et enregistrement (phase 2)

Modules de `feuillet-core` :

| Module | Rôle |
|---|---|
| `annot.rs` | Modèle partagé avec l'UI (`Annot`, `AnnotBody`) en points d'affichage ; opérations `Add` / `Update` / `Remove` / `SetField` ; pile annuler/rétablir commune aux annotations et aux champs (chaque entrée = opérations inverses) |
| `form_script.rs` | Fonctions standard d'Acrobat (`AFNumber_*`, `AFPercent_*`, `AFDate_*`, `AFTime_*`, `AFSpecial_*`, `AFRange_Validate`, `AFSimple_Calculate`) : analyse des scripts en appels à arguments littéraux, mise en forme, calculs. Aucun code du PDF n'est exécuté ; le reste est signalé comme personnalisé |
| `form.rs` | AcroForm : lecture de l'arbre des champs (attributs hérités, widgets situés par page), valeurs, apparences régénérées des champs texte et des listes (police standard du `/DA`, `/MK`, `/BS`, alignement, multiligne, peigne, taille automatique) |
| `writer.rs` | `Editor` : analyse lopdf des annotations existantes, diff avec l'état enregistré, production d'**une** révision incrémentale (base + objets modifiés), lecture et écriture chiffrées |
| `appearance.rs` | Flux `/AP /N` de chaque type ; `/Matrix` qui compense la rotation de page (textes et tampons droits) |
| `fonts.rs` | Largeurs AFM des polices standard 14 (WinAnsi) pour la mise en page des zones de texte |
| `redact.rs` | Caviardage réel à l'enregistrement : réécriture des flux de contenu, des images et des formulaires ; réécriture complète du fichier |
| `pdfwrite.rs` | Sérialisation d'objets PDF |
| `fsutil.rs` | Écriture atomique |

**Cycle d'une modification.**
1. L'UI envoie des opérations (`apply_annotations`), traitées en file par document.
2. L'éditeur les applique et les mémorise pour l'annulation.
3. Il reconstruit les octets : base enregistrée + une révision contenant seulement les annotations ajoutées ou modifiées, leurs apparences, et le tableau `/Annots` ou le dictionnaire des pages touchées.
4. PDFium recharge ces octets sous le même identifiant de document.
5. L'UI incrémente la révision des pages modifiées (`pageRev`), ce qui invalide leurs bitmaps.

L'original sur disque n'est jamais touché avant « Enregistrer ».

**Annotations existantes.**
- Les types gérés (marquage, formes, ligne, zone de texte, note, coches, tampons) entrent dans le modèle et sont pleinement modifiables.
- Les autres (Ink, Polygon…) peuvent être déplacés ou supprimés : seuls `/Rect` et les tableaux de coordonnées sont translatés, l'apparence d'origine est conservée.
- Liens et popups ne sont jamais modifiés. Une popup n'est retirée qu'avec son annotation parente.

**Formulaires.**
- Les champs sont lus à l'ouverture d'un document à formulaire, avec le modèle d'annotations (`EditState.fields`).
- Une saisie est une opération `SetField` : elle entre dans le même historique que les annotations.
- À l'écriture : `/V` (et `/I` pour les listes) sur le champ ; `/AS` sur les widgets des cases et radios, dont les apparences existent ; une apparence `/AP /N` régénérée pour les champs texte et les listes. Les widgets gardent leur numéro d'objet : le tableau `/Annots` n'est pas réécrit.
- Scripts : les champs exposent `format`, `range`, `calc` et `customScript`. L'interface filtre et valide la saisie (`lib/field-format.ts`) ; l'écrivain recalcule les champs `AFSimple_Calculate` dans l'ordre `/CO` à chaque saisie, dans le même pas d'annulation, et met en forme l'apparence (valeur brute dans `/V`).
- Côté interface, `FormLayer` superpose des contrôles HTML aux widgets. Hors focus, ils sont transparents et laissent voir l'apparence rendue par PDFium ; avec le focus, le champ texte devient opaque et montre la saisie.

**Enregistrer.**
- Sans caviardage : écriture atomique de base + révision. Les octets d'origine restent un préfixe exact du fichier, donc les signatures existantes restent intactes (test `signed_document_keeps_signed_bytes`).
- Avec caviardage : passe `redact::apply`, puis réécriture complète. Il ne reste ni révision antérieure ni objet orphelin, et le fichier est rechiffré s'il l'était.

**Caviardage.**
- Texte : les glyphes recouverts (≥ 50 % en largeur, ≥ 40 % en hauteur) sont retirés des `Tj`/`TJ`/`'`/`"`, remplacés par un décalage équivalent. Le calcul des largeurs gère polices simples, Type0/CID, Type3 et standard 14.
- Tracés : segments retirés pour les traits, sous-chemins entiers pour les remplissages.
- Images DCT, Flate ou sans filtre (8 bits, masque 1 bit) : pixels noircis. Autres formats (JBIG2, CCITT, JPX) : image retirée entière.
- XObjects formulaires : copie propre à la page.
- `/ActualText` et `/Alt` : retirés du contenu marqué.
- Annotations recouvertes : supprimées.

## Rendu dans la webview

- **Virtualisation.** `layout.ts` calcule la position de chaque page selon le mode (page unique, continu, double page) et le zoom. Seules les pages visibles, plus une hauteur d'écran de marge, sont montées.
- **Basse résolution d'abord.** Une page montée affiche immédiatement la meilleure bitmap déjà en cache (miniature ou zoom précédent), puis la remplace par le rendu à la bonne taille (taille CSS × `devicePixelRatio`).
- **Tuiles.** Au-delà de 12 Mpx par page, une bitmap de base d'environ 4 Mpx est complétée par des tuiles de 1024 px limitées à la zone visible.
- **Transfert.** Les bitmaps arrivent en RGBA brut par le protocole `feuillet://`, plus rapide qu'`invoke` (voir ADR-001 § 3.2), puis passent par `ImageData` et `createImageBitmap`. Un cache LRU de 512 Mo regroupe les requêtes identiques en cours.
- **Couche de texte.** Des `span` transparents sont positionnés en points, puis l'ensemble est mis à l'échelle par une transformation CSS, ce qui évite tout recalcul au zoom. Un `scaleX` par mot aligne le texte sur la largeur réelle, et des `<br>` en fin de ligne donnent une copie propre.
- **Zoom ancré.** Ctrl + molette ou pincement zoome autour du curseur ; les boutons zooment autour du haut de la vue. Les modes « Ajuster » se recalculent à chaque redimensionnement.

## État de l'interface

- `tabs` : un état par onglet (`DocTab`). Il comprend :
  - le statut : chargement, verrouillé, prêt ou erreur ;
  - la vue : mode, zoom, ajustement, page ;
  - la barre latérale et la recherche ;
  - les demandes de navigation (`nav`), que `DocViewport` exécute.
- Les onglets restent montés (`v-show`), ce qui conserve le défilement et les bitmaps de chacun.
- `settings` : thème (système, clair ou sombre, suivi de `prefers-color-scheme`), accent (`--accent` sur `:root`), langue, boutons de fenêtre. Ces préférences sont persistées par le backend.
- **Localisation.** `vue-i18n` avec deux langues, `fr` (source) et `en` ; le type `Messages` garantit qu'elles ont les mêmes clés. La langue suit la préférence, sinon celle de l'OS (`sys-locale`), sinon l'anglais.

## Fenêtre et intégration au système

- **Fenêtre.**
  - Sans décorations natives : barre de titre personnalisée, avec `data-tauri-drag-region` et les boutons Réduire, Agrandir et Fermer, masquables dans les préférences.
  - La fenêtre est créée invisible et affichée une fois le thème appliqué (pas de flash blanc).
- **Ouverture des fichiers.**
  - Par glisser-déposer n'importe où (`onDragDropEvent`).
  - Par la ligne de commande : les fichiers sont lus au démarrage, puis par `take_pending_files`.
  - Si Feuillet tourne déjà, `tauri-plugin-single-instance` transmet les fichiers à l'instance en cours par l'événement `open-files-event`.
  - Par l'association `.pdf` : `fileAssociations`, `.desktop` avec `%F`, `RunEvent::Opened` sous macOS.
- **Linux, NVIDIA et Wayland.** `WEBKIT_DISABLE_DMABUF_RENDERER=1` est positionné automatiquement si le pilote NVIDIA est présent (voir ADR-001 § 6).
- **Persistance.** Préférences et récents sont stockés en JSON dans le dossier de config, et les miniatures des récents dans le dossier de cache. Toutes les écritures sont atomiques : fichier temporaire, fsync, rename, fsync du dossier.
- **Mots de passe.** Ils sont mémorisés dans le trousseau du système (`keyring` 4, Secret Service sous Linux), sous une clé dérivée du chemin du fichier (SHA-256 tronqué). Un mot de passe mémorisé devenu faux est effacé.

## Tests

| Niveau | Outil | Contenu |
|---|---|---|
| Annotations | `cargo test -p feuillet-core --test annotations` | 12 types en aller-retour (rendu PDFium, relecture), préfixe d'origine intact, pages tournées et recadrées, xref en flux, chiffrement, annotations existantes, document signé, enregistrer sous |
| Caviardage | `cargo test -p feuillet-core --test redaction` | Texte partiel retiré (extraction PDFium et `pdftotext`), pixels d'un scan, tracés, chiffré, annotations recouvertes |
| Moteur | `cargo test -p feuillet-core` | Fonctions pures (mots, recherche) ; intégration sur `fixtures/` : rendu et tuiles identiques à la page, texte et géométrie, recherche, chiffrement, liens, formulaires, signatures, annulation par époque |
| App | `cargo test -p feuillet` | Récents (déduplication, plafond, persistance), préférences, arguments de ligne de commande, export des types TS |
| UI unitaire | `pnpm test` (Vitest) | Mise en page, ajustements, tuiles, dates relatives, langue |
| Parcours | `pnpm test:e2e` (Playwright) | Lecture (15 parcours) et annotation (8) : outils au clavier, poignées, mini-barre, annuler/rétablir, surlignage, zone de texte, note, coches, duplication, fermeture d'un document modifié, caviardage, tampon, enregistrer sous. Backend simulé : `vite --mode e2e`, `e2e/mock/bindings.ts` |
| Visuel | `VITE_SELFTEST=1` ou `2` + `FEUILLET_NO_SINGLE_INSTANCE=1` | Scénarios pilotés par les stores (`src/dev/selftest.ts`) sur l'app réelle, instance isolée (XDG_* temporaires) |

## Arborescence

```
crates/feuillet-core/   moteur PDF (Rust, sans Tauri)
src-tauri/              app Tauri : commandes, protocole, persistance, intégration OS
src/                    interface Vue (bindings.ts généré — ne pas modifier)
e2e/                    Playwright + backend simulé
fixtures/               PDF de test (+ scripts/ pour les régénérer)
design/                 référence de design figée
scripts/fetch-pdfium.mjs  télécharge libpdfium (version épinglée) dans src-tauri/pdfium/
```
