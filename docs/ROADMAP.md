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
| S (signature) | ✅ | Phase 4 |
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

## Phase 3 : formulaires ✅ (validée)

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

## Phase 4 : signatures ✅ (validée)

| Fonction | État | Notes |
|---|---|---|
| Vérification des signatures numériques : `/ByteRange`, empreinte, signature CMS (RSA PKCS#1 v1.5, RSA-PSS, ECDSA P-256 / P-384 ; SHA-1 à SHA-512) | ✅ | RustCrypto (`cms`, `x509-cert`, `rsa`, `p256`, `p384`) ; vérifié sur les fixtures pyHanko (valide, altéré, annoté après signature) ; 🟡 RSA-PSS et ECDSA sans fixture |
| Chaîne de certificats jusqu'au magasin du système, validité à la date de signature | ✅ | `rustls-native-certs` ; pas de contrôle de révocation (CRL, OCSP) |
| CMS encodé en BER (longueurs indéfinies : Dropbox Sign…) | ✅ | Réencodé en DER avant analyse |
| Autorités de signature approuvées par l'utilisateur (panneau, après confirmation de l'empreinte ; retrait dans les préférences) | ✅ | Le magasin du système ne contient que des autorités web : Notarius (Dropbox Sign), par exemple, n'y figure pas. La liste d'Adobe (AATL) n'est pas redistribuable |
| Listes de confiance : listes officielles de l'UE (eIDAS : autorités qualifiées et horodatage qualifié, périodes d'agrément à la date de signature) et racines du programme Microsoft (CCADB : signature de documents, courriel, authentification) | ✅ | Instantané embarqué (`crates/feuillet-core/data`, régénéré par l'exemple `trust_lists`), rafraîchi chaque semaine en arrière-plan ; origine affichée dans le panneau. Vérifié sur des documents réels : Adobe Acrobat Sign (liste italienne), Dropbox Sign et sceau DocuSign (Microsoft) |
| Échec de lecture ou algorithme inconnu : « non vérifiable », pas « invalide » | ✅ | « Invalide » est réservé au contenu modifié ou à une plage signée incohérente |
| Horodatage RFC 3161 : empreinte et signature du jeton | ✅ | 🟡 sans fixture horodatée |
| 3 bandeaux (valide, invalide, non vérifiable), panneau de détails (planche 05), certificat, copie des détails | ✅ | « Valide, des ajouts ont été faits depuis » pour une révision ajoutée après signature |
| Avertissement avant d'annoter ou de remplir un document signé (planche 06) : annuler, quand même, sur une copie ; « Ne plus demander » | ✅ | Mémorisé par chemin dans les préférences |
| Enregistrement incrémental : la partie signée reste intacte | ✅ | Déjà le cas depuis la phase 2 (sauf caviardage, qui réécrit le fichier et le signale) |
| Signature manuscrite (planche 07) : Dessiner (encre, épaisseur, pression du stylet), Importer (PNG, JPG, SVG, fond blanc retiré), Taper (3 polices embarquées) | ✅ | Tampon image qui suit le curseur jusqu’au clic (Échap annule), puis déplaçable et redimensionnable ; même pose pour l’outil Image (I) |
| Signatures enregistrées : menu de l'outil S, réutilisation, suppression | ✅ | PNG dans le dossier de configuration (`signatures/`) |
| Champs de signature des formulaires : « Cliquer pour signer » | ✅ | La signature manuscrite est ajustée dans le champ |

Écarts et limites :
- Le texte de l'avertissement diffère de la planche 06 : celle-ci annonce que la modification « invalidera » la signature. Comme Feuillet enregistre en incrémental, la version signée reste vérifiable ; le texte le dit.
- Une signature manuscrite est une image, pas une signature numérique : créer une signature numérique (certificat, PAdES) n'est pas prévu.
- Le type de signature enregistrée (« Signature » / « Paraphe » sur la planche) n'est pas distingué : toutes s'appellent « Signature ».
- Révocation des certificats (CRL, OCSP) non consultée.
- Les signatures XML des listes de confiance ne sont pas vérifiées : elles sont téléchargées en HTTPS depuis les sites officiels (Commission européenne, autorités nationales, CCADB).
- La liste d'Adobe (AATL) n'est pas redistribuable : les autorités qu'elle seule contient restent « non vérifiables » sans approbation manuelle. Exemple : les signataires DocuSign standard (« DocuSign Cloud Signing CA - SI1 », racine « OpenTrust Root CA G1 ») ; la variante qualifiée « Premium » figure, elle, dans la liste française.
- Racines Microsoft retenues sur leurs usages déclarés (signature de documents, courriel, authentification de personnes) : plus large que la seule signature de documents, comme Acrobat lorsqu'il s'appuie sur le magasin Windows.
- Pas de détail des modifications faites après la signature (Acrobat distingue les modifications autorisées).

## Phase 5 : organisation des pages ✅ (validée)

| Fonction | État | Notes |
|---|---|---|
| Mode « Organiser les pages » (Ctrl Maj O) : barre de la planche 08, « Lecture », « Terminé » | ✅ | Double-clic ou Entrée sur une page : retour à la lecture sur cette page |
| Grille de miniatures, taille réglable, sélection multiple (clic, Ctrl, Maj, Ctrl A, flèches, Espace) | ✅ | Miniatures chargées à l'approche de la zone visible |
| Glisser-déposer : repère d'insertion, aperçu, « Déplacer avant la page n » | ✅ | Glisser à la souris (pas le glisser HTML, capté par la fenêtre native), défilement automatique |
| Pivoter (Ctrl R, Ctrl Maj R), dupliquer (Ctrl D), page blanche (Ctrl Maj N), supprimer (Suppr) | ✅ | Les annotations suivent leur page ; un document garde au moins une page |
| Extraire en PDF | ✅ | Nouveau fichier avec les pages, leurs annotations, leurs champs et les signets qui y mènent |
| Côte à côte : un onglet ouvert ou un autre fichier ; copie par glisser (« Copier avant la page n ») | ✅ | Champs de formulaire ajoutés à l'AcroForm (noms en double renommés « _2 »), polices du formulaire reprises, signets ajoutés à la suite |
| Annuler / rétablir commun avec les annotations et les champs | ✅ | Chaque opération sur les pages est un pas (instantané de l'état) |
| Enregistrement incrémental | ✅ | Révisions ajoutées : la partie signée d'un document signé reste intacte ; avertissement avant de réorganiser un document signé |
| Barre latérale : glisser une miniature pour réordonner, vers un autre onglet pour copier (survoler l'onglet l'affiche), boutons de rotation au survol | ✅ | Ctrl C / X / V, Ctrl D, Ctrl R, Suppr sur la miniature qui a le focus |
| Fichiers PDF déposés sur les miniatures : pages insérées à cet endroit | ✅ | Ailleurs dans la fenêtre, le dépôt ouvre toujours le fichier ; document protégé : message |
| Copier, couper, coller des pages (Ctrl C / X / V) dans la grille et la barre latérale, entre documents | ✅ | Copie figée au moment du Ctrl C (document en mémoire) ; collage après la sélection |
| Glisser des pages vers le gestionnaire de fichiers | ⬜ | `tauri-plugin-drag` non intégré : « Extraire en PDF » en attendant |

Écarts et limites :
- Le badge « pivotée » de la planche n'est pas affiché : la géométrie des pages ne distingue pas une page pivotée d'une page paysage d'origine.
- Signets copiés à plat (sans hiérarchie), destinations explicites seulement (pas les destinations nommées).
- Pages dupliquées : sans les champs de formulaire de l'original (un champ ne peut figurer qu'une fois par nom sans être lié).
- Chaque opération relit et réécrit l'arbre des pages : quelques centaines de millisecondes sur un document lourd (scans de plusieurs dizaines de Mo).
- Une extraction depuis un document chiffré produit un fichier non chiffré.
- Volet de droite : les actions de la barre visent le volet utilisé en dernier ; Ctrl Z annule dans le document du volet qui a le focus.

## Phase 6 : enregistrement et export ✅

| Fonction | État | Notes |
|---|---|---|
| Ctrl S, Ctrl Maj S, modale à la fermeture d'un document modifié | ✅ | Faits en phase 2 |
| Fenêtre « Exporter » (Ctrl Maj E, bouton « Partager et exporter », menu) : planche 10 | ✅ | Nom du fichier proposé, estimation de taille mise à jour en direct |
| PDF : annotations et champs modifiables | ✅ | Fichier réécrit en entier : objets inutilisés retirés, flux compressés ; caviardage en attente appliqué |
| PDF aplati | ✅ | Aplatissement PDFium ; AcroForm retiré ; valeurs saisies conservées dans la page |
| Images : PNG ou JPG, 72 / 150 / 300 ppp, une image par page | ✅ | « nom-01.png »… ; rendu avec annotations et champs |
| Qualité des images (léger, équilibré, maximal) | ✅ | Images RVB ou en niveaux de gris réduites (1 400 / 2 200 px) et réencodées en JPEG si plus légères ; qualité JPEG d'un export en images |
| Pages : toutes, page actuelle, plage (« 1-3, 5, 8- ») | ✅ | |
| Mot de passe et autorisations (impression, copie) | ✅ | AES-256 (révision 6), mot de passe propriétaire aléatoire ; vérifié avec qpdf et Poppler |
| Impression native (Ctrl P, menu, fenêtre d'export) | 🟡 | Linux : dialogue d'impression GTK, pages rendues par PDFium (300 ppp au plus). Non vérifiée automatiquement (dialogue du système). macOS et Windows : message « non disponible » |

Écarts et limites :
- Un document chiffré s'exporte en clair si « Protéger par mot de passe » n'est pas coché.
- Recompression : images CMJN, à palette, masques, JBIG2 et JPX laissées telles quelles.
- La taille « actuel » est celle d'un export sans recompression (pas celle du fichier sur le disque).
- Export d'une partie des pages : document reconstruit (signets à plat, métadonnées et étiquettes de page non reprises).

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
