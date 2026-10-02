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

## Phase 2 : annotation ✅ (validée)

| Fonction | État | Notes |
|---|---|---|
| Bouton Annoter (Ctrl Maj A), barre d'outils secondaire, « Terminé » | ✅ | Infobulles nom + touche |
| V H U K T N R O L A C X | ✅ | H/U/K : glisser sur le texte, ou touche sur une sélection ; surlignage de zone sur les scans. Outils de pose à usage unique (retour à la sélection) ; H/U/K restent actifs |
| S (signature) | ⬜ | Phase 4 (bouton présent, message explicatif) |
| I (tampon ou image) | ✅ | PNG (transparence conservée) et JPEG |
| Panneau contextuel : 6 couleurs, épaisseur, police, taille, symbole | ✅ | |
| Survol, sélection, 8 poignées (2 pour les lignes), déplacement, redimensionnement | ✅ | Maj : 45° pour les lignes, proportions conservées ; flèches du clavier : déplacement de 1 pt (10 avec Maj) |
| Mini-barre : couleur, taille, épaisseur, Dupliquer (Ctrl D), Supprimer (Suppr) | ✅ | |
| Copier, couper, coller (Ctrl C / X / V) | ✅ | Collage sur la page courante, aussi dans un autre onglet ; décalé en cascade sur la page d’origine. Hors marquage de texte et annotations externes |
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

## Phase 3 : formulaires ✅ (en attente de validation)

| Fonction | État | Notes |
|---|---|---|
| Lecture des champs AcroForm : texte, multiligne, mot de passe, peigne, case, radio, liste déroulante (éditable ou non), liste (choix multiple ou non) | ✅ | Attributs hérités, noms complets, widgets multiples, lecture seule et obligatoire |
| Saisie directe sur la page, contrôles superposés aux widgets | ✅ | Clic, Espace, sélection ; Échap annule la saisie en cours |
| Tab / Maj Tab, pastille « Champ n sur N » | ✅ | Ordre : page, puis lignes de haut en bas et de gauche à droite ; un arrêt par groupe radio ; boucle à la fin |
| Génération des apparences (texte et listes), `/V`, `/AS`, `/I` | ✅ | Rendu vérifié avec PDFium et Poppler ; 🟡 Acrobat et Aperçu non vérifiés |
| Bandeau : « Surligner les champs » (mémorisé), « Effacer le formulaire » (valeurs par défaut), masquer | ✅ | |
| Annuler / rétablir commun aux annotations et aux champs | ✅ | « Effacer le formulaire » = un seul pas |
| Enregistrement incrémental des valeurs | ✅ | Relu après réouverture |
| Scripts standard d'Acrobat : formats nombre, monnaie, pourcentage, date, heure, spéciaux et masques ; `AFRange_Validate` ; `AFSimple_Calculate` dans l'ordre `/CO` | ✅ | Réimplémentés, aucun code du PDF n'est exécuté. Saisie filtrée et validée (message, valeur précédente rétablie) ; apparence mise en forme ; le calcul entre dans le même pas d'annulation que la saisie. Fixture : `fixtures/formulaire-scripts.pdf` |

Écarts et limites :
- La modale de fermeture parle de « modifications » (la planche 12 dit « annotations ») : elle compte aussi les champs remplis.
- Scripts personnalisés (tout code autre que les fonctions standard, y compris la « notation simplifiée » des calculs et les scripts du document) : non exécutés, signalés dans le bandeau.
- Les calculs ne sont pas relancés à l'ouverture, seulement après une saisie.
- Pourcentages : on saisit « 15 » pour 15 % (Acrobat attend 0,15) ; la valeur enregistrée reste 0,15, comme dans Acrobat.
- Noms de mois des dates (`mmm`, `mmmm`) écrits en anglais, comme Acrobat, pour rester lisibles par les autres lecteurs ; les noms français sont acceptés à la saisie.
- Champs de signature : affichés, non remplis (phase 4). Boutons poussoirs ignorés.
- Les champs non saisissables en mode annotation : les outils prennent la main sur la page.
- Texte hors WinAnsi : « ? » dans l'apparence, comme pour les zones de texte (le texte exact reste dans `/V`).

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
