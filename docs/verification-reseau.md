# Vérification de l'absence de requête réseau

**État :** méthode écrite et appliquée le 27 octobre 2026, concluante, puis refaite le 3 novembre 2026 (couches 1 et 2) sur la version debug du lot 8 (premier lancement refait, dictionnaire inclus), concluante. À refaire sur la version release avant la première version, et à chaque nouvelle dépendance.

## Le critère

`PROJET.md`, section 4 : aucune requête réseau après l'installation des modèles. Le seul accès réseau de l'app est le téléchargement des deux modèles (`src-tauri/src/download.rs`), lancé uniquement par un clic de l'utilisateur.

## La méthode, en trois couches

### 1. Ce que le code peut faire (statique)

À refaire quand on ajoute une dépendance, et avant la première version.

| Contrôle | Commande (depuis la racine du dépôt) | Attendu |
|---|---|---|
| Aucune bibliothèque cliente HTTP ou websocket parmi les dépendances | `cd src-tauri && cargo tree --target aarch64-apple-darwin -e normal --prefix none \| sed 's/ (.*//' \| sort -u \| grep -i -E "^(reqwest\|hyper\|ureq\|h2\|rustls\|native-tls\|openssl\|tungstenite\|tokio-tungstenite\|isahc\|attohttpc\|tauri-plugin-(http\|updater\|opener\|shell\|upload\|websocket))( \|$)"` | aucune ligne (la crate `http` seule ne fait que décrire des requêtes) |
| Aucun accès réseau dans les sources Rust en dehors du téléchargement | `grep -rln -i -E "TcpStream\|UdpSocket\|TcpListener\|std::net\|reqwest\|ureq\|https?://\|/usr/bin/curl" src-tauri/src` | `download.rs` (appel à `/usr/bin/curl`) et `models.rs` (les deux adresses des modèles), rien d'autre |
| Le dictionnaire n'ajoute aucun accès réseau : il ne lit que le champ actif (accessibilité de macOS ou Cmd + C) et écrit un fichier local | `grep -n -i -E "TcpStream\|reqwest\|https?://\|curl" src-tauri/src/dictionary.rs src-tauri/src/corrections.rs src-tauri/src/field.rs src-tauri/src/learn.rs` | aucune ligne (`learn.rs` lance `/usr/bin/osascript` pour une notification locale, pas d'accès réseau) |
| Aucune adresse externe chargée par les pages de la fenêtre | `grep -rn -E "https?://\|wss?://\|fetch\(\|XMLHttpRequest\|WebSocket" src index.html indicator.html` | seulement la déclaration d'espace de noms du logo SVG |
| Aucune capacité réseau donnée aux fenêtres | `cat src-tauri/capabilities/default.json` | `core:default` et `core:window:allow-start-dragging` (déplacement de la fenêtre par sa bande du haut, ajouté au lot 8) |
| Le téléchargement ne part que d'un clic | `grep -rn -E "download_models\|models\.download\(\)\|run: models\.download" src` | trois lignes : `Setup.tsx` lance `models.download()` seulement après le clic sur « Commencer » (état `begun`) et propose « Reprendre » après une erreur ; `useModels.ts` appelle la commande. De plus `download_missing` saute les modèles déjà présents, donc le clic ne fait rien quand tout est installé |

### 2. Ce que fait le processus (dynamique)

```bash
bash tools/check-network.sh 90
```

Lance Aède d'abord (de préférence juste avant, pour couvrir le démarrage et le chargement des modèles), puis le script, puis fais quelques dictées pendant les 90 s. Le script liste environ cinq fois par seconde les sockets réseau (TCP et UDP) du processus `aede` et de ses sous-processus (le `curl` du téléchargement en serait un) avec `lsof`. Codes de sortie :

- 0 : aucune connexion, et au moins une dictée a eu lieu pendant la mesure (comptée dans `history.jsonl`) ;
- 1 : une connexion a été vue, elle est affichée ;
- 2 : Aède ne tourne pas ;
- 3 : aucune connexion mais aucune dictée, la mesure ne prouve rien.

Le script a été éprouvé avec un faux processus `aede` qui écoute sur un port local : il l'a détecté (code 1).

Pour voir aussi si un téléchargement est tenté, lancer l'app avec sa sortie d'erreur dans un fichier (`open --stderr /chemin/aede.log "…/Aède.app"`) et chercher les lignes `[download]`.

### 3. Hors ligne (à la main, facultatif)

Wi-Fi coupé, faire une dictée : elle doit être transcrite, reformulée et collée comme d'habitude. Cela prouve que l'app n'a pas besoin du réseau, pas qu'elle n'essaie pas de l'utiliser : c'est un complément, pas un remplacement de la couche 2.

## Résultat du 27 octobre 2026

Version release, Wi-Fi allumé, modèles déjà installés.

| Couche | Résultat |
|---|---|
| 1. Statique | Aucune bibliothèque cliente dans l'arbre de dépendances (seule la crate `http`, qui décrit des requêtes sans rien envoyer). Les sources Rust n'ont d'accès réseau que dans `download.rs`. Les pages de la fenêtre ne chargent rien d'externe. `download_models` n'est appelé que par un bouton. |
| 2. Dynamique | 90 s depuis le lancement : 575 échantillons, 0 connexion réseau, 2 dictées pendant la mesure (15,2 s et 11,7 s d'audio). 0 ligne `[download]`. Code de sortie 0. |
| 3. Hors ligne | Non fait. |

## Résultat du 3 novembre 2026

Version debug de la branche du lot 8 (nouveau premier lancement, pastille vide au démarrage, démo animée), Wi-Fi allumé, modèles déjà installés.

| Couche | Résultat |
|---|---|
| 1. Statique | Les six contrôles sont conformes. Aucune bibliothèque cliente dans l'arbre de dépendances. Seuls `download.rs` et `models.rs` mentionnent le réseau dans les sources Rust. Le dictionnaire n'en mentionne aucun. Les pages ne chargent rien d'externe (seule ligne : le `xmlns` du logo SVG, et aucune `url(http…)` dans le CSS construit). Les trois appels du téléchargement sont les mêmes qu'avant. Les dépendances JS ajoutées au lot (`motion`, `lucide-react`, `@radix-ui/react-select`) sont embarquées dans la page. |
| 2. Dynamique | 90 s lancées juste après le relancement de l'app : 588 échantillons, 0 connexion réseau, 4 dictées pendant la mesure. Code de sortie 0. Les lignes `[download]` n'ont pas été cherchées (sortie d'erreur non redirigée). |
| 3. Hors ligne | Non fait. |

## Limites de la méthode

- Une connexion de moins d'un cinquième de seconde peut passer entre deux échantillons.
- Une résolution de nom (DNS) sans connexion ensuite est faite par un service système (`mDNSResponder`) pour le compte du processus : `lsof` ne la voit pas. La couche 1 la couvre : aucun code de l'app ne résout un nom en dehors de `curl`, qui ne part que d'un clic.
- Les fenêtres utilisent le moteur web de macOS, dont les processus sont des services système séparés que le script ne suit pas. Les pages sont locales (couche 1), donc ils n'ont rien à charger.
- Elle ne contrôle ni le contenu des paquets ni ce que font les bibliothèques système. Pour une preuve plus forte (tous les paquets de la machine), il faudrait un pare-feu applicatif comme Little Snitch, ou `tcpdump` avec les droits administrateur : non fait.
