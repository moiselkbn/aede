# Dictionnaire personnel

**État :** code terminé, tests automatiques passés, tests à la main concluants pour le parcours principal (octobre 2026). La proposition s'affiche dans la pastille de l'encoche (tests 11 et 12 faits à la main par l'auteur le 29 octobre 2026, sans détail test par test).

Le dictionnaire corrige les mots que Whisper entend de travers et que la reformulation ne rattrape pas : un prénom (« Kamil » au lieu de « Camille »), un nom de projet (« Fynd de doc » au lieu de `FindTheDoc`), un nom de rue. Il garde des mots avec leurs variantes « mal entendu » : `Supabase` avec « super base », « soupe à base ».

## Comment on s'en sert

1. Fais une dictée. Le texte est collé.
2. Corrige à la main, dans l'application où il a été collé, le mot (ou les quelques mots) mal transcrit.
3. Appuie sur le raccourci d'apprentissage (par défaut `Option + Maj + Espace`, réglable dans la fenêtre, section « Dictionnaire »).
4. Aède lit le champ, le compare avec la dernière dictée et propose les corrections trouvées dans la pastille de l'encoche : une ligne par correction (« Kamil → Camille », les flèches alignées), un cadre opaque « ⌃⌥D Valider | Échap Annuler » et une barre blanche qui se vide en 10 secondes. La pastille sort de l'encoche et y rentre avec le même ressort que celle de la dictée ; à la réponse, elle se transforme en place en ligne d'état (« Ajouté au dictionnaire », ou le refus), et la barre s'estompe quand elle remonte.
5. Appuie une seconde fois sur le même raccourci dans les 10 secondes pour valider. `Échap` annule. Sans réponse en 10 secondes, la proposition est annulée. Rien n'est jamais ajouté sans cette seconde pression.
6. Tant que la pastille affiche « Ajouté au dictionnaire » (2 secondes), refaire le raccourci pour ces mêmes mots ne fait rien : la pastille ne change pas. Ce n'est qu'après, une fois la pastille rentrée, qu'un nouvel appui sur les mêmes mots affiche « Déjà dans le dictionnaire ». Si un autre mot a été corrigé entre-temps, il est proposé tout de suite.

À la dictée suivante, la paire est appliquée au texte juste avant le collage.

Le dictionnaire se gère dans la page Dictionnaire de la fenêtre : ajout d'un mot et de ses variantes à la main, modification, recherche, suppression. La liste montre les mots les plus récents en haut (ou de A à Z), et les boutons d'une ligne n'apparaissent qu'au survol. Un mot sans variante est accepté : il ne remplace rien, il est gardé pour un usage futur. « Tout effacer » dans l'historique vide aussi le dictionnaire.

## Ce que fait la correction

- Elle s'applique au **texte final**, après la reformulation et juste avant le collage. Le LLM et le prompt v5 ne sont pas touchés : ils ne voient jamais le dictionnaire. Le texte brut de repli reçoit la même correction.
- Un mot mal entendu est reconnu en entier, quelle que soit la casse, et sans tenir compte des espaces, des tirets ni de la ponctuation à l'intérieur : « Fynd de doc », « fynd-de-doc » et « FyndDeDoc » sont la même chose. Il peut avoir jusqu'à 6 mots. Le plus long est cherché en premier.
- La ponctuation et les espaces autour du mot remplacé restent comme ils étaient.
- Un dictionnaire illisible n'empêche jamais de dicter : le texte est collé tel quel et l'erreur s'affiche dans la fenêtre.

## Comment Aède trouve la correction

Aède lit le champ de texte actif quand tu appuies sur le raccourci (et une seule autre fois, juste après chaque collage, voir plus bas). Il ne le surveille jamais en continu. Il compare mot à mot la dernière dictée de l'historique avec ce qu'il y a maintenant dans le champ.

**Lecture du champ.** D'abord par l'API d'accessibilité de macOS (la même permission que pour le collage). Les navigateurs et les applications Electron n'exposent leur contenu qu'après qu'Aède le demande, ce qu'il fait. VS Code n'expose rien : dans ce cas Aède simule un Cmd + C (qui copie la sélection, ou la ligne entière s'il n'y a pas de sélection) et remet ton presse-papiers ensuite.

**Trouver où est la dictée.** Après chaque collage réussi, Aède relit le champ une fois, 250 ms plus tard, en tâche de fond, et garde en mémoire vive (jamais sur le disque, 15 minutes au plus) les 3 mots avant et les 3 mots après la dictée. Cela lui permet de retrouver la zone exacte même quand la correction est au tout début ou à la toute fin de la dictée. Sans ces mots (lecture par copie, ou texte qui a trop changé), il se rabat sur les mots voisins.

**Jusqu'à 3 paires.** Si plusieurs mots ont changé, jusqu'à 3 paires sont proposées d'un coup. Au-delà, Aède refuse (« 4 corrections d'un coup, c'est trop ») et n'ajoute rien.

**Deux méthodes qui se complètent.**
- *Sans sélection* : Aède détecte seul ce qui a changé.
- *Avec le mot corrigé sélectionné* : seul ce qui est dans la sélection est retenu. C'est utile quand la détection ne peut pas deviner (voir plus bas). Dans VS Code, la sélection ne peut pas servir à restreindre, car la copie ne renvoie que la sélection : ne sélectionne rien, la ligne est utilisée.

**Le garde-fou.** Quand ta correction est plus longue que le mot d'origine et touche le bord de la dictée, Aède ne peut pas savoir si les mots en plus font partie de la correction ou s'ils ont été tapés à côté (« J'aime bien les prunes. » devenu « J'aime bien les poires aussi », avec « aussi » ajouté par toi). Plutôt que de deviner, il refuse avec ce message : « Je ne sais pas si « poires aussi » est tout entier la correction de « prunes » ou s'il contient des mots ajoutés après. » Deux remèdes : garder le point final de la dictée, ou sélectionner seulement le mot corrigé avant de refaire le raccourci. Cette règle pourra être revue à l'usage.

## Ce qui est écrit sur le disque, et ce qui ne l'est pas

| Donnée | Où | Durée |
|---|---|---|
| Les paires | `dictionary.json` dans le dossier de données de l'app, droits 600 | jusqu'à suppression ou « Tout effacer » |
| Le texte du champ, lu au raccourci et une fois après chaque collage | mémoire vive seulement, jamais écrit | le temps du calcul |
| Les 3 mots avant et après une dictée | mémoire vive seulement | 15 minutes |

Rien ne quitte le Mac. Le dictionnaire n'ouvre aucune connexion réseau (voir `docs/verification-reseau.md`). Aède lit le texte du champ actif à deux moments seulement : au raccourci d'apprentissage, et une fois juste après chaque collage pour retenir les mots voisins. Il ne garde que ces mots voisins, en mémoire vive.

## Fichiers

- `src-tauri/src/dictionary.rs` : le fichier des paires (lire, ajouter, supprimer, tout effacer) et le remplacement des mots dans un texte.
- `src-tauri/src/corrections.rs` : la comparaison entre la dictée et le champ, sans aucun accès au système (le plus testé : une trentaine de tests).
- `src-tauri/src/field.rs` : la lecture du champ actif (accessibilité, sinon Cmd + C) et de la sélection.
- `src-tauri/src/learn.rs` : le raccourci d'apprentissage, la proposition, la confirmation, l'annulation, et l'envoi des réponses à la pastille (`indicator.rs`, qui les transmet à `src/LearnProposal.tsx`). Une notification macOS ne sert plus que de repli si la pastille est introuvable.
- `src-tauri/src/shortcut.rs` : le changement de raccourci, commun à la dictée et à l'apprentissage. Les deux raccourcis ne peuvent pas être identiques.
- `src/Dictionary.tsx` : la section « Dictionnaire » de la fenêtre.

## Tests à la main

Avec l'app construite par `npm run app` et les permissions accordées.

1. **Une correction.** Dicte « Installe Versel. », corrige en « Vercel », raccourci, raccourci. La pastille affiche « Ajouté au dictionnaire ». Dicte à nouveau : « Vercel » sort directement.
2. **Plusieurs mots.** « Fynd de doc » corrigé en `FindTheDoc` : la paire entière est proposée.
3. **Trois paires** dans la même phrase : les trois sont proposées. **Quatre** : refus.
4. **Annulation.** Raccourci, puis `Échap` : « Proposition annulée ». Raccourci, puis attendre 10 secondes : « Proposition expirée ».
5. **Déjà connue.** Refaire la même correction : « Déjà dans le dictionnaire ».
6. **Garde-fou.** « J'aime bien les prunes. » corrigé en « J'aime bien les poires aussi », sans sélection : message d'incertitude. Avec « poires » sélectionné : la paire `prunes -> poires`.
7. **Dans quatre applications** : Notes, Mail, une page web (champ de recherche et champ d'un site d'IA), VS Code (par copie de la ligne).
8. **Aucune dictée à corriger.** Après « Tout effacer », le raccourci répond « Aucune dictée à corriger ».
9. **Fenêtre.** Changer le raccourci d'apprentissage (il est gardé après relance) ; essayer de lui donner la combinaison de la dictée (refus) ; supprimer une paire (le mot n'est plus remplacé) ; « Tout effacer » vide la liste.
10. **Pendant une dictée.** Une proposition en attente est abandonnée sans bruit quand une dictée commence.
11. **La pastille.** Fidèle à la maquette Figma (variante C) : sans contour, fond noir qui devient presque transparent vers le bas, cadre Valider / Annuler, barre blanche qui se vide en 10 secondes. Une, deux et trois propositions : une ligne chacune, largeur adaptée (234 pt au minimum). Lisible sur fond clair et sur fond sombre. Aucun collage ne part au mauvais endroit pendant qu'elle est affichée.
12. **Résultats et refus dans la pastille.** « Ajouté au dictionnaire » (point vert), puis « Aucune dictée à corriger », « Déjà dans le dictionnaire », « Trop de corrections » et « Correction ambiguë » (point rouge pour les refus). `Échap` et l'expiration font disparaître la pastille sans texte.

## Limites connues

- **VS Code** n'expose pas son texte : la lecture se fait par copie (la ligne du curseur), et la sélection ne restreint rien.
- **Texte collé sans espace après un point** (« bonjour.lecture ») : géré, mais c'est un cas fragile.
- **Notifications de repli** : si la pastille est introuvable, la réponse passe par `osascript` au nom d'« Éditeur de script », et un mode de concentration peut la rendre muette. Le détail reste visible dans le terminal (lignes `[learn]`).
- **Fond de la pastille** : le bas est presque transparent (21 %, choix de l'auteur), donc le texte d'une fenêtre derrière peut transparaître. Le cadre Valider / Annuler est opaque pour que cela ne brouille pas les touches. Un flou de fond n'est pas possible en CSS dans cette fenêtre ; il faudrait une vue de flou native de macOS, gros chantier laissé de côté.
- **Raccourci d'apprentissage** : une combinaison déjà prise par les applications (⇧⌘Z, « Rétablir ») leur serait volée. Le défaut est `Option + Maj + Espace`.
- **Texte court dans la pastille** : les refus y tiennent en quelques mots (« Trop de corrections », « Correction ambiguë »...). La phrase complète est dans la fenêtre principale, sauf pour les cas normaux (« Aucune dictée à corriger », « Déjà dans le dictionnaire », « Aucun mot changé »).
- **Pas de saisie manuelle** : une paire ne se crée que par une correction, pas à la main dans la fenêtre.
- **Variantes** : « Fynd 2 Doc » ou « Fynd de Doc » avec une faute ne sont pas la même paire que « Fynd de doc ». La recherche tolérante n'est pas faite.

## Ce qui n'est pas fait

- **L'écran sans encoche** : la proposition s'affiche en haut au centre, mais ce cas n'a pas été vérifié séparément. L'encoche ne doit jamais prendre le focus (voir `docs/encoche.md`), ce qui interdit une fenêtre de saisie et explique la confirmation par un second appui sur le même raccourci.
- **L'avis sur le garde-fou** : à réévaluer après la semaine d'usage.

## Idée pour une version ultérieure : un dictionnaire que le LLM comprend

**Le constat.** Aujourd'hui, le dictionnaire est un remplacement littéral : si la chaîne `Fynd de doc` (ou `FyndDoDoc`) apparaît dans le texte, elle est remplacée par `FindTheDoc`. C'est le rôle d'un snippet. Il ne marche que si Whisper écrit exactement la même chose à chaque fois. Un jour Whisper écrit « Fyyynd2Doc », la paire ne s'applique pas, et il faut apprendre une nouvelle paire pour chaque variante.

**Ce que l'auteur veut.** Que ce soit le LLM de reformulation qui comprenne, dans la phrase, que « Fyyynd2Doc » est de toute logique `FindTheDoc`, parce que ce mot existe dans le dictionnaire. Dans cette version, la notion de « souvent confondu avec » disparaît : on déclare les mots qui doivent exister, et le LLM fait le rapprochement phonétique et logique lui-même. Plus de variante à apprendre une par une.

**Pourquoi ce n'est pas fait maintenant.**
- Cela passe par le prompt du LLM, donc par une nouvelle version du prompt. La décision du 27 octobre 2026 est de garder la v5 pour la première version, sans v6, et de réévaluer après la semaine d'usage. Un nouveau prompt doit en plus être mesuré sur un jeu de validation neuf (les jeux déjà utilisés ne servent qu'une fois par prompt) et sur le jeu de référence `tests/jargon-fr.md`.
- Le risque de fidélité est réel : dire à un petit modèle « ces mots existent, utilise-les quand tu les reconnais » peut l'amener à remplacer un mot ordinaire proche, ce que la règle « ne jamais changer le sens ni ajouter » interdit. Cela se mesure, cela ne se suppose pas.
- Le remplacement littéral est sûr et prévisible. Il doit rester comme filet, même après cette évolution.

**Piste d'implémentation à mesurer.** Garder le remplacement littéral après la reformulation, et ajouter les mots du dictionnaire dans le prompt, accompagnés de leur forme mal entendue comme indice (« Whisper écrit parfois `Fynd de doc` pour `FindTheDoc` »). Les paires apprises par le raccourci deviennent alors des exemples pour le LLM, plus seulement des règles. Le prompt v5 contient déjà une liste de termes anglais que le LLM rapproche de ce que Whisper entend (« Cloud » pour `Claude`), ce qui montre que le principe marche pour des termes courants. À mesurer pour des noms propres rares et pour des variantes inventées.

**Conséquence pour le design.** La maquette Figma actuelle liste uniquement des mots. C'est la forme qui convient à cette version ultérieure. La version actuelle a besoin de deux colonnes (ce que Whisper a écrit, ce que l'utilisateur veut), que la maquette ne prévoit pas. Il faut soit adapter le design à deux colonnes pour la version actuelle, soit s'en tenir à la liste de mots à ce moment-là. La section « Dictionnaire » de la fenêtre affiche aujourd'hui `mal entendu -> bon mot` en texte simple, sans suivre la maquette.
