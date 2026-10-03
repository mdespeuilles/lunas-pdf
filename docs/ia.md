# Assistant IA

Comme Lunas Mail (même architecture, code repris), Lunas PDF confie l'IA à un agent déjà
installé sur le poste, utilisé avec l'abonnement de l'utilisateur : **Claude Code** ou
**Codex**. Choix dans Préférences › Assistant IA (aucun agent : ni bouton ni raccourci).

Le panneau s'ouvre à droite du document (bouton ✦ de la barre d'outils, Ctrl/⌘ J). Une
conversation par onglet : questions (« Résume ce document », « Quelles échéances ? ») et
actions (« Remplis ce document »). Les liens `lunas://page/N` des réponses mènent à la page.

## Les outils du document

Avec Claude Code, le temps d'une demande, Lunas PDF sert un **serveur MCP local**
(`src-tauri/src/ai/mcp.rs`, repris de Lunas Mail) : HTTP sur `127.0.0.1`, port aléatoire,
clé aléatoire exigée à chaque requête. Claude Code le reçoit par `--mcp-config` et n'a
**que ces outils** (`--tools ""`, `--strict-mcp-config`, `--allowedTools mcp__lunas`).

| Outil | Rôle |
|---|---|
| `get_page_text` | Texte d'une page en segments positionnés `[x,y,w,h]` (points, origine en haut à gauche). |
| `view_page` | Image de la page telle qu'affichée (annotations et valeurs comprises), quadrillée tous les 50 pt : mise en page, lignes et cases vides, scans, vérification des ajouts. |
| `list_form_fields` | Champs AcroForm : id, libellé, type, valeur, valeurs permises, position. |
| `fill_form_fields` | Remplit des champs (texte, case, radio, liste). |
| `list_annotations` | Annotations du document. |
| `add_annotations` | Zones de texte (`text`, coin supérieur gauche de la première ligne) et coches (`check`, centrées). Taille de la zone calculée avec les métriques de l'interface. |
| `remove_annotations` | Retire des annotations par id. |
| `propose_memory` | Propose de mémoriser des informations données par l'utilisateur (carte dans le panneau). |

Chaque appel qui modifie le document passe par `Engine::edit`, comme l'interface : c'est
**un pas d'annulation** (Ctrl Z), signalé au front par `AiEditedEvent`, et **rien n'est
enregistré** avant que l'utilisateur enregistre. Pas de confirmation à chaque action
(choix validé) : le document reste à l'écran et tout s'annule.

Jamais proposés : enregistrer, exporter, caviarder, supprimer ou déplacer des pages.

Consignes (`prompts.rs`) : formulaire interactif → `fill_form_fields` uniquement ; sinon
texte positionné et image de la page, ajouts groupés par page, vérification visuelle ;
ne jamais inventer de données personnelles (les signaler comme manquantes).

## Informations mémorisées

Après un remplissage avec des informations données dans la conversation, l'agent appelle
`propose_memory` : une carte « Mémoriser pour les prochains documents ? » liste les
informations (une par ligne, préfixée par la personne : « Parent – E-mail : … ») ;
l'utilisateur décoche ce qu'il ne veut pas garder, puis « Mémoriser » ou « Non merci ».
Rien n'est mémorisé sans ce clic. Jamais proposées (sauf demande explicite) : numéros de
sécurité sociale, de pièce d'identité, coordonnées bancaires, santé, mots de passe.

Les informations sont stockées sur le poste (`memory.json` dans le dossier de config), jointes
à chaque demande (`<saved_user_info>`) et modifiables dans Préférences › Assistant IA
(oublier une information, tout oublier).

Essais réels (abonnement requis ; un essai par commande, PDFium ne se charge qu'une fois par
processus) :
`LUNAS_CLAUDE=~/.local/bin/claude cargo test -p lunas-pdf ai::essais::remplit_un_formulaire -- --ignored --nocapture`
(et `place_texte_et_coche`, `utilise_la_memoire`) : remplissage et proposition de
mémorisation sur `formulaire-acroform.pdf`, placement d'un texte et d'une coche sur
`texte-simple.pdf`, remplissage à partir des seules informations mémorisées. Vérifiés avec
Claude Code 2.1.288.

## Fonctionnement

Rust (`src-tauri/src/ai/`) lance l'agent comme un programme séparé, dans un dossier
temporaire vide, la consigne sur son entrée standard, et relaie le texte au front par
`AiChunkEvent` (appels d'outils : `AiToolEvent`). Une demande s'annule (bouton Arrêter)
et expire au bout de 15 min.

| Agent | Commande | Sûreté |
|---|---|---|
| Claude Code | `claude -p --output-format stream-json --tools "" --setting-sources "" --strict-mcp-config --no-session-persistence --system-prompt …` + serveur MCP | Seuls les outils du document. |
| Codex | `codex exec --json --ephemeral --skip-git-repo-check --sandbox read-only -` | Réponses en texte seulement (pas de serveur MCP) ; outils non désactivables, lecture seule. Averti dans les préférences. |

L'agent est trouvé comme le terminal le trouverait (shell interactif puis de connexion),
puis dans les emplacements habituels ; une version trop ancienne est signalée.

## Données envoyées

Seulement le document de l'onglet et les informations mémorisées, et seulement quand
l'utilisateur pose une question :
son texte (60 000 caractères au plus, la suite se lit par `get_page_text`), puis ce que
l'agent demande par les outils (texte, images des pages, champs). Le contenu du document est
présenté comme des données, jamais comme des instructions.
