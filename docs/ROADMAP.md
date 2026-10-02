# Feuille de route

Légende : ✅ fait · 🟡 partiel ou non vérifié · ⬜ à faire

## Phase 0 : spike du moteur PDF ✅

- ✅ ADR-001 accepté (option 3 : PDFium + écrivain Rust).
- ✅ Fixtures et scripts de génération (`fixtures/scripts/`).

## Phase 1 : lecture ✅ (validée)

| Fonction | État | Notes |
|---|---|---|
| Accueil : zone de dépôt, récents en grille ou en liste, « Effacer la liste » | ✅ | Miniatures réelles ; récents protégés floutés, sans miniature |
| Onglets, point « modifié », Ctrl W, Ctrl Tab, clic du milieu | ✅ | Le point « modifié » servira à partir de la phase 2 |
| Barre de titre personnalisée, boutons de fenêtre masquables | ✅ | |
| Barre d'outils : navigation, aller à la page (Ctrl G), zoom 50–400 %, W, F, Ctrl 0, Ctrl ±, Ctrl + molette | ✅ | |
| Modes page unique, continu, double page (Ctrl 1/2/3) | ✅ | |
| Barre latérale (F9) : miniatures, sommaire et signets, annotations | ✅ | Miniatures virtualisées |
| Recherche (Ctrl F) : surlignage, Entrée / Maj Entrée, Alt C, résultats groupés par page | ✅ | Recherche incrémentale côté Rust |
| Sélection et copie du texte | ✅ | Copie bloquée si le document l'interdit |
| Liens internes et externes | ✅ | |
| Document protégé : déverrouillage, erreur, trousseau | 🟡 | Trousseau non testé contre un vrai Secret Service |
| Glisser-déposer, ligne de commande, association `.pdf`, instance unique | 🟡 | Ligne de commande vérifiée ; le reste est câblé mais non testé de bout en bout |
| Thème système, clair ou sombre, accent ; localisation fr/en | ✅ | |
| Rendu virtualisé, cache, basse résolution d'abord, tuiles, abandon des rendus périmés | ✅ | |
| Formulaire XFA : message clair | ✅ | |
| CI GitHub Actions (lint, tests, builds Linux, macOS, Windows) | 🟡 | Écrite, jamais exécutée (pas de dépôt distant) |

## Phase 2 : annotation ✅ (en attente de validation)

| Fonction | État | Notes |
|---|---|---|
| Bouton Annoter (Ctrl Maj A), barre d'outils secondaire, « Terminé » | ✅ | Infobulles nom + touche |
| V H U K T N R O L A C X | ✅ | H/U/K : glisser sur le texte, ou touche sur une sélection ; surlignage de zone sur les scans |
| S (signature) | ⬜ | Phase 4 (bouton présent, message explicatif) |
| I (tampon ou image) | ✅ | PNG (transparence conservée) et JPEG |
| Panneau contextuel : 6 couleurs, épaisseur, police, taille, symbole | ✅ | |
| Survol, sélection, 8 poignées (2 pour les lignes), déplacement, redimensionnement | ✅ | Maj : 45° pour les lignes, proportions conservées ; flèches du clavier : déplacement de 1 pt (10 avec Maj) |
| Mini-barre : couleur, taille, épaisseur, Dupliquer (Ctrl D), Supprimer (Suppr) | ✅ | |
| Édition en place des zones de texte, notes | ✅ | Double-clic ; Ctrl Entrée ou clic ailleurs pour valider, Échap pour annuler |
| Liste des annotations par page (planche 03), clic pour s'y rendre | ✅ | Extrait du texte marqué, « Vous · heure » |
| Annotations PDF natives avec apparences, lisibles par PDFium et Poppler | ✅ | 🟡 Acrobat et Aperçu non vérifiés (pas de Mac ici) |
| Annuler / rétablir global par document | ✅ | Annotations ; formulaires et pages viendront s'y ajouter |
| Caviardage réel (texte, images, tracés), vérifié par extraction | ✅ | Images JBIG2/CCITT/JPX : retirées entières |
| Enregistrer (Ctrl S), Enregistrer sous (Ctrl Maj S), modale de la planche 12 | ✅ | Avancé de la phase 6 pour ne pas perdre d'annotations |
| Nom d'auteur dans les préférences | ✅ | Par défaut : utilisateur du système |

Écarts avec le design :
- La mini-barre propose les 6 couleurs (la planche en montre 3).
- Le surlignage utilise des teintes claires de la même palette, pour garder le texte lisible.
- La confirmation du caviardage n'a pas de planche dédiée : elle reprend la modale de la planche 12.
- L'historique d'annulation est vidé après « Enregistrer ».

## Phase 3 : formulaires ⬜

- Saisie AcroForm, Tab / Maj Tab, « Champ n sur N », génération des apparences.
- Bandeau « Surligner les champs » et « Effacer le formulaire ».

## Phase 4 : signatures ⬜

- Signature manuscrite : Dessiner, Importer, Taper ; signatures enregistrées.
- Vérification de l'intégrité des signatures numériques (`/ByteRange` + CMS), 3 bandeaux, panneau de détails.
- Avertissement avant d'annoter un document signé, enregistrement incrémental.

## Phase 5 : organisation des pages ⬜

- Grille, sélection multiple, glisser-déposer, rotation, duplication, suppression, page blanche, extraction.
- Côte à côte, copie entre documents (fusion des signets et de l'AcroForm).
- Glisser vers le gestionnaire de fichiers (`tauri-plugin-drag`, à valider).

## Phase 6 : enregistrement et export ⬜

- ✅ Ctrl S, Ctrl Maj S, modale à la fermeture d'un document modifié (faits en phase 2).
- Export PDF, PDF aplati, PNG ou JPG ; compression ; plages ; mot de passe et permissions (lopdf V5).
- Impression native du PDF (pas `webview.print()`).

## Dette et points ouverts

- Zones de texte : caractères hors WinAnsi (CJK, cyrillique…) affichés « ? » dans l'apparence ; le texte exact reste dans `/Contents`. Prévoir l'intégration d'une police en sous-ensemble.
- Caviardage : les textes en écriture verticale et les motifs (`sh`, motifs de remplissage) ne sont pas traités ; les signets, métadonnées et le texte des champs de formulaire hors zone ne sont pas nettoyés.
- Un enregistrement avec caviardage invalide les signatures numériques (comportement attendu, signalé dans la confirmation).
- pdfium-render 0.9.4 :
  - permissions R6 non décodées (on autorise par défaut en lecture) ;
  - premier lien renvoyé en double (dédupliqué) ;
  - enregistrement incrémental non exposé : sans objet, l'écrivain Feuillet produit lui-même la révision (pas de patch nécessaire).
- Playwright WebKit ne démarre pas sur Arch (bibliothèques Ubuntu) : il tourne en CI, Chromium sert en local.
- AppImage : 103 Mio, car elle embarque WebKitGTK (comportement standard de Tauri).
