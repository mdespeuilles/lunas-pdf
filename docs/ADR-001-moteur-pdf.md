# ADR-001 — Moteur PDF de Lunas PDF

- **Statut** : accepté (option 3), 2026-10-02
- **Date** : 2026-10-02
- **Décideurs** : Maxence (produit), Claude (mise en œuvre)
- **Code du spike** : supprimé après validation. Résultats bruts WebKitGTK : `docs/spike-001-resultats-webkitgtk.txt`. Générateurs de fixtures : `fixtures/scripts/`

## 1. Contexte

Lunas PDF est une app Tauri 2 (Rust et webview Vue 3). Sous Linux, la webview est WebKitGTK 2.52.6 (webkit2gtk-4.1). L'app doit afficher vite des PDF longs ou scannés. Elle doit aussi écrire des annotations natives, remplir des AcroForm, enregistrer en incrémental sans casser les signatures, chiffrer et réorganiser des pages. Le tout sous licence permissive (MuPDF, AGPL, est exclu).

## 2. Options étudiées

| # | Option | Rendu | Écriture |
|---|---|---|---|
| 1 | **pdf.js dans la webview** et manipulation en Rust (lopdf) | pdf.js (canvas, worker) | lopdf, ou `saveDocument()` de pdf.js |
| 2 | **Tout PDFium** (`pdfium-render`) | PDFium, bitmaps envoyées à la webview | PDFium seul |
| 3 | **PDFium en lecture et un « écrivain Lunas PDF » en Rust (lopdf)** | PDFium, bitmaps envoyées à la webview | lopdf pour annotations, formulaires, incrémental, chiffrement, signets ; PDFium seulement pour les passes destructives (caviardage, aplatissement) |
| — | Exclues | MuPDF (AGPL-3.0, licence commerciale Artifex) ; hayro 0.7 (expérimental, sans texte ni annotations, performances non travaillées) ; pdf-rs 0.10 (écriture expérimentale) ; qpdf seul (aucune écriture incrémentale, issue #22) | |

Versions vérifiées le 2026-10-02 :
- pdfium-render 0.9.4 (MIT/Apache, API PDFium 7881) ;
- binaires PDFium bblanchon chromium/8076 (BSD-3/Apache) ;
- pdfjs-dist 6.3.289 (Apache-2.0) ;
- lopdf 0.45.0 (MIT) ;
- qpdf 12.4.2 et crate `qpdf` 0.3.7 (Apache/MIT, option `vendored`) ;
- Tauri 2.12.1.

## 3. Mesures

Machine : Ryzen (12 threads), 30 Go de RAM, Omarchy/Hyprland. GPU hybride AMD Raphael + NVIDIA RTX 3060 Ti. Le DPR mesuré dans la webview est de 1,18.

Fixtures :
- `long-320-pages.pdf` : 0,9 Mo, texte dense, vecteurs, 64 signets, liens internes ;
- `scan-lourd.pdf` : 24 pages, JPEG A4 couleur à 300 ppp, 72,7 Mo.

« Page » = page A4 rendue à 1588 px de large (100 % sur écran ×2). « Tuile » = 1024² à 400 % ×2.

### 3.1 PDFium natif (Rust, sans webview)

Commande : `bench-pdfium render`.

| Mesure (médiane) | 320 pages texte | Scan lourd |
|---|---|---|
| Ouverture du document | 0,2 ms (chargement paresseux) | 0,1 ms |
| Miniature 180 px | 0,7 ms | 46 ms |
| Page 1588 px | **5,8 ms** | **41 ms** |
| Tuile 1024² à 400 % | 28 ms | 171 ms |
| Extraction du texte d'une page | 0,6 ms | — |
| Recherche « renard » sur 320 pages | 250 ms (6651 occurrences) | — |

La première page coûte environ 235 ms à cause du chargement des polices. PDFium ne garde pas en cache l'image JPEG décodée d'un rendu à l'autre : chaque rendu d'une page scannée la décode à nouveau.

### 3.2 Dans WebKitGTK (app Tauri réelle, `bench-webview`)

| Mesure (médiane) | 320 pages texte | Scan lourd |
|---|---|---|
| **pdf.js** : page 1588 px | 31 ms | 25 ms (image déjà décodée) |
| **pdf.js** : premier rendu d'une page scannée (décodage compris) | — | **≈ 270 ms** |
| **pdf.js** : miniature 180 px | 15 ms | 267 ms (décodage) |
| **pdf.js** : `getTextContent` | 11 ms | 13 ms |
| **PDFium → protocole personnalisé** : page 1588 px de bout en bout | **37 ms**, dont 7 ms de rendu et ≈ 29 ms de transfert pour 14,3 Mo RGBA | **104 ms**, dont 74 ms de rendu |
| PDFium → protocole : miniature 180 px | 2 ms | 46 ms |
| PDFium → `invoke` (`ipc::Response`) : page | 41 ms | 108 ms |
| 12 pages 1588 px demandées d'un coup (défilement rapide) | 319 ms | 928 ms |
| Pire écart entre deux frames de l'UI | pdf.js 30 ms ; protocole 40 ms ; `invoke` 46 ms | pdf.js 26 ms ; protocole 33 ms ; **`invoke` 86 ms, 11 frames > 50 ms** |

À retenir :
- **Les deux voies sont fluides** : aucune frame au-delà de 50 ms, sauf avec `invoke` sur les scans.
- **Le goulet de PDFium dans la webview est le transfert**, pas le rendu : environ 0,5 Go/s par le protocole personnalisé. Au DPR réel de cet écran (1,18), une page pèse 5 Mo, soit environ 11 ms de transfert. Avec des tuiles et un rendu au DPR exact, le coût reste maîtrisé.
- Il faut le protocole personnalisé, pas `invoke`.
- **Sur les scans, PDFium est 3 à 6 fois plus rapide au premier affichage** (41–74 ms contre ≈ 270 ms). pdf.js redevient plus rapide une fois l'image décodée et gardée en cache.
- WebKitGTK 2.52.6 offre tout ce que pdf.js 6.x exige : `Promise.withResolvers`, `Map.groupBy`, `Uint8Array.fromBase64`, `OffscreenCanvas`, WebAssembly, `Math.sumPrecise`.

### 3.3 Capacités d'écriture testées

Les sorties sont vérifiées avec `qpdf --check`, un rendu Poppler/PDFium et l'oracle de signature pyHanko.

| Besoin | PDFium (testé) | lopdf (testé) | pdf.js (d'après la doc et le code source) |
|---|---|---|---|
| Highlight, Underline, StrikeOut, Square, Circle, Ink, Text, FreeText, Stamp (image) | ✅ création. ⚠️ **aucun `/AP` écrit dans le fichier**, sauf pour le Stamp : Poppler et PDFium les génèrent à l'affichage, Aperçu et Acrobat ne le font pas de façon garantie | ✅ tout est faisable, `/AP` écrits par nos soins (Highlight en Multiply, FreeText avec police dans les ressources) | Éditeur limité à FreeText, Highlight, Ink, Stamp et signature dessinée ; les `/AP` sont générés |
| Line avec flèche (`/LE`), Polygon, Redact | ❌ `FPDFAnnot_IsSupportedSubtype` = faux | ✅ Line + OpenArrow testé | ❌ |
| Enregistrement incrémental | ✅ avec le drapeau `FPDF_INCREMENTAL`, **non exposé par pdfium-render** (patch nécessaire). Les octets d'origine sont conservés, mais **PDFium réécrit tout objet chargé** : polices, catalogue, `/Pages`, flux de contenu (+1,8 Ko sur un document de 3 pages) | ✅ append minimal : nouveaux objets + dictionnaire de page | ✅ `saveDocument()` est toujours incrémental |
| Signature intacte après ajout d'une annotation (pyHanko) | ✅ intact, couvre sa révision. Le diff pyHanko est jugé « suspect » parce que des objets partagés (polices) sont réécrits | ✅ intact, couvre sa révision. Diff plus petit, mais la politique stricte de pyHanko le signale aussi | non testé |
| AcroForm, texte | ⚠️ les setters pdfium-render écrivent `/V` **sans régénérer l'apparence** (champ vide dans Poppler). ✅ la voie `FORM_*` (focus + `ReplaceSelection`) régénère l'apparence, accents et € compris | à écrire nous-mêmes (`/V` + apparence) | ✅ régénère les apparences Tx/Btn/Ch |
| AcroForm, cases, radios, listes | ❌ la voie `FORM_*` (clic, espace, `SetIndexSelected`) renvoie vrai **sans rien changer** (cause non trouvée, plafond de temps atteint). pdfium-render écrit `/V` en *chaîne* `"/Yes"` au lieu d'un *nom* | ✅ trivial pour cases et radios (`/V` + `/AS`, les apparences on/off existent déjà) ; listes : apparence à générer | ✅ |
| Détection XFA | ✅ `FPDF_GetFormType` | — | ✅ |
| Ouvrir un PDF chiffré (AES-256 R6) | ✅ mot de passe incorrect → `PasswordError`. ⚠️ pdfium-render renvoie `UnknownPdfSecurityHandlerRevision` pour les permissions R6 (bug du wrapper ; `FPDF_GetDocPermissions` direct à utiliser) | ✅ `decrypt` | ✅ |
| Exporter avec mot de passe et permissions | ❌ aucune API (ne sait que garder ou retirer le chiffrement) | ✅ V5/AES-256 R6, impression et copie interdites, mot de passe vérifié par qpdf. ⚠️ `/Length` absent du dictionnaire `/Encrypt` (avertissement qpdf, correctif d'une ligne) | ❌ |
| Insérer les pages d'un autre PDF | ✅ 0,1 ms, annotations et liens conservés. ❌ **signets non importés** ; destinations vers des pages non copiées perdues ; champs non rattachés à l'`/AcroForm` | ✅ faisable, copie d'objets à coder (renumérotation, fusion des signets et de l'AcroForm) | `extractPages` seulement |
| Caviardage réel | ⚠️ un objet texte entièrement couvert est bien supprimé (`pdftotext` le confirme). Un objet partiellement couvert reste intact : **découpe au glyphe à coder** (`FPDFText_SetCharcodes`/`SetPositions`) ; images avec `FPDFImageObj_SetBitmap` | à coder entièrement (analyse du flux de contenu et des métriques de police) | ❌ |
| Signatures : lecture | ✅ `FPDFSignatureObj_*` (Contents, ByteRange, motif, date) ; aucune vérification | — | — |

Vérification cryptographique des signatures : à composer en Rust avec `cms` 0.2 (analyse seulement, pas de `verify`), `x509-cert`, `rsa`, `p256`/`p384` et `sha2`. Les crates spécialisées (`underskrift` 0.1, `pdf_signer` en GPL-3) sont immatures ou mal licenciées.

## 4. Grille d'évaluation

Notes de 1 à 5, pondérées.

| Critère (poids) | 1 · pdf.js + lopdf | 2 · tout PDFium | **3 · PDFium + écrivain Rust** |
|---|---|---|---|
| Fluidité WebKitGTK, 300 pages (3) | 5 (31 ms/page, natif DOM) | 4 (37 ms, transfert) | 4 |
| Fluidité sur scans lourds (2) | 2 (≈ 270 ms par nouvelle page) | 4 (41–74 ms) | 4 |
| Annotations natives lisibles partout (3) | 2 (éditeur limité ; types manquants à écrire en Rust → deux écrivains) | 2 (pas d'`/AP`, pas de Line ni de Redact) | **5** |
| AcroForm et apparences (2) | 5 | 3 (texte OK, boutons et listes KO) | 3 (apparences à coder ; PDFium en secours pour le texte) |
| Incrémental et signatures (3) | 4 | 3 (réécritures inutiles) | **5** (append minimal et maîtrisé) |
| Chiffrement, export protégé (1) | 3 (via lopdf) | 1 | 4 |
| Pages, fusion, signets (2) | 3 | 3 | 4 |
| Caviardage réel (2) | 1 | 4 (outils objet et glyphe présents) | 4 (passe PDFium) |
| Un seul modèle de document (2) | 2 (JS lit, Rust écrit : deux analyseurs) | 5 | 3 (PDFium lit, lopdf écrit, octets comme pivot) |
| Packaging (1) | 5 (+5,6 Mo de pdf.js) | 4 (+7,5 Mo de `.so`) | 4 |
| **Total pondéré (sur 105)** | 67 | 70 | **86** |

## 5. Décision proposée

**Option 3 : PDFium pour lire, afficher, extraire et rechercher le texte, avec un « écrivain Lunas PDF » en Rust (lopdf) comme seule source des modifications.**

### Architecture du moteur
```
octets d'origine (jamais modifiés) ──► PDFium (fil dédié, lecture seule)
        │                                ├─ rendu de pages et tuiles → protocole lunas-pdf:// → <canvas>
        │                                ├─ texte, boîtes des caractères, recherche, liens, signets
        ▼                                └─ lecture des signatures et du type de formulaire
journal de commandes (annuler / rétablir)
        │  annotations, valeurs de formulaire, opérations sur les pages
        ▼
écrivain Lunas PDF (lopdf)
  ├─ aperçu : révision incrémentale en mémoire, reconstruite à chaque commande → PDFium recharge les octets (≈ 0,2 ms)
  ├─ « Enregistrer », document signé : UNE révision incrémentale ajoutée à l'original
  ├─ « Enregistrer », document non signé ou pages réorganisées : réécriture complète
  └─ export : chiffrement AES-256 R6, permissions, plages de pages
passes destructives sur les octets (enregistrement complet uniquement) : caviardage, aplatissement → PDFium
écriture atomique : fichier temporaire, fsync, rename
```

### Pourquoi
- **Rendu** : PDFium est l'option la plus robuste (moteur de Chrome) et la plus rapide sur les scans. Le surcoût du transfert vers la webview est mesuré et maîtrisable : protocole personnalisé, DPR exact, tuiles au-delà de 200 %, cache par zoom, miniatures d'abord.
- **Écriture** : aucune bibliothèque ne couvre seule les 10 types d'annotations avec apparences, l'incrémental minimal, le chiffrement et la fusion des signets. Un écrivain unique en Rust, où chaque modification est un objet qu'on maîtrise, donne des fichiers déterministes et des diffs incrémentaux minimaux (l'idéal pour les signatures). Il se teste aussi en aller-retour.
- Le pivot « octets → octets » isole les deux moteurs : PDFium ne voit jamais un état que lopdf n'aurait pas sérialisé.

### Conditions et travaux induits
1. **pdfium-render** : contribuer upstream, ou maintenir un patch léger, pour exposer `handle()`/`page_handle()`, l'enregistrement avec drapeaux et les permissions R6. Le patch du spike tient en 15 lignes. Épingler la version de l'API sur celle du binaire livré : **les valeurs de `FPDF_REMOVE_SECURITY` ont changé après 7881**.
2. **Générateur d'apparences** (phases 2 et 3) : formes, marquage de texte, FreeText, champs texte et listes, avec les polices standard 14 en WinAnsi. Hors WinAnsi, une police embarquée en sous-ensemble (crate `subsetter`) prend le relais, ou en dernier recours `NeedAppearances`. **Principal risque calendaire.** Si le générateur de champs texte s'avère trop coûteux, repli possible : faire régénérer les apparences par PDFium via `FORM_*` dans une passe dédiée, validée sur les champs texte pendant le spike.
3. **Caviardage** : passe PDFium avec découpe des objets texte au glyphe, recadrage et masquage des images, retrait des chemins ; enregistrement **complet** obligatoire. Test : `pdftotext` ne doit plus contenir le texte caviardé.
4. **Vérification des signatures** : module maison (`cms` + RustCrypto), intégrité seulement au départ, comme prévu.
5. **Impression** : `webview.print()` imprime le HTML. Il faudra une impression native du PDF (GtkPrintOperation/CUPS sous Linux, API système ailleurs). Point à part, à étudier en phase 6.

## 6. Packaging

| | Linux | macOS | Windows |
|---|---|---|---|
| Binaire Tauri (spike, sans symboles) | ≈ 12 Mo | à mesurer en CI | à mesurer en CI |
| PDFium (`.so`/`.dylib`/`.dll`) | 7,5 Mio (3,5 Mo compressé), dépend seulement de libc, libm, libgcc_s et pthread | 7,3 Mio (arm64) | 7,4 Mio |
| Distribution | Embarqué dans AppImage, deb et rpm (aucun paquet distro : seulement AUR) | Dans le bundle `.app`, à signer | À côté de l'exe, dans le msi |

- **Option XFA/V8 écartée** : 44 Mio, alors que le cahier des charges demande seulement de signaler les formulaires XFA.
- **Risque Linux identifié** : sur ce poste (GPU hybride NVIDIA, Wayland), WebKitGTK plante au démarrage avec « Error 71 (protocol error) dispatching to Wayland display ». `WEBKIT_DISABLE_DMABUF_RENDERER=1` corrige le problème. Lunas PDF devra le positionner automatiquement dans ce cas, avant de créer la webview, et le documenter.

## 7. Conséquences

- **+** Fichiers propres (`/AP` partout, révisions minimales), contrôle total sur la compatibilité avec Acrobat et Aperçu, rendu rapide et fidèle.
- **−** Plus de code maison : générateur d'apparences, copie de pages et fusion des signets, caviardage. Deux analyseurs PDF dans le binaire. Patch de pdfium-render à suivre.
- **Non vérifié ici** (pas d'Acrobat ni d'Aperçu sur la machine) : l'affichage des annotations écrites par lopdf dans Acrobat et Aperçu. À faire manuellement en fin de phase 2 sur un Mac, et à ajouter à la checklist de release.

## 8. Autres points relevés pendant le spike

- **Localisation fr/en** (nouvelle exigence) : `vue-i18n` côté UI, et détection de la langue de l'OS côté Rust (crate `sys-locale` ou `tauri-plugin-os`). Réglage « Langue : système / français / anglais » dans les préférences. Les libellés du design deviennent les clés de référence en français.
- **Dossier du design** : renommé de `feuillet-design/` en `design/` (validé le 2026-10-02).
- **Types TS** : `tauri-specta` et `specta` sont encore en 2.0.0-rc.25. Utilisables, mais à épingler.
- **Glisser des miniatures vers le gestionnaire de fichiers** : `tauri-plugin-drag` 2.1.1 annonce Linux (GTK), macOS et Windows. À valider en phase 5.
- **Trousseau** : `keyring` 4.2 passe par Secret Service (D-Bus) sous Linux. Il faut prévoir un repli silencieux si aucun service n'est lancé.
