# Jarvis — mode d'emploi

Dis **« Hey Jarvis »** (prononcé à l'anglaise : « Hé Djarvisse »), attends le petit « ding », puis parle. Tu peux aussi enchaîner directement : « Hey Jarvis, lance Discord ».

- Tu as **6 secondes** pour commencer à parler, et tu peux faire de petites pauses : il attend 1,4 s de silence avant de considérer que tu as fini.
- **Après sa réponse, il t'écoute encore 5 secondes** : tu peux continuer la discussion sans redire « Hey Jarvis » (« et demain ? », « réponds-lui que oui »…).

Jarvis tourne **en arrière-plan, sans fenêtre** : son icône « J » est près de l'horloge, et la barre Givre en haut de l'écran montre son état.

| Couleur de l'icône | Jarvis… |
|---|---|
| bleu | attend « Hey Jarvis » |
| vert | t'écoute |
| orange | réfléchit |
| violet | parle |
| rouge | a le micro coupé |

## Ce que tu peux dire

| Pour… | Exemples |
|---|---|
| Applis et sites | « lance Discord », « ouvre Chrome », « ferme Spotify », « ouvre YouTube », « cherche des chats sur YouTube » |
| Vidéo et musique | « pause », « mets lecture », « suivante », « précédente », « avance de 30 secondes », « recule de 10 secondes », « c'est quoi la chanson qui passe ? » |
| Deezer | « mets la playlist Electro Chill », « joue la chanson Bande organisée », « mets l'album … », « lance Jul sur Deezer » |
| Volume | « monte le son », « baisse le son », « mets le son à 40 », « coupe le son », « remets le son » |
| Minuteurs | « minuteur de 5 minutes pour les pâtes », « minuteur d'une heure et demie », « annule le minuteur » |
| Alarme et chrono | « réveille-moi à 7 h 30 », « lance le chrono », « arrête le chrono », « remets le chrono à zéro » |
| Pronote | « j'ai quoi comme devoirs demain ? », « c'est quoi ma dernière note ? », « j'ai quel cours après ? », « ouvre Pronote » |
| Mails | « j'ai des mails importants ? », « ouvre mes mails », « résume-moi le mail d'AliExpress », et après avoir cliqué sur un mail dans la barre : « fais-moi un résumé », « qu'est-ce que je réponds ? » |
| Météo | « quel temps fait-il ? », « et demain ? » |
| Discord | « lis mes messages Discord », « réponds à Tom que j'arrive », « envoie un message à Léa qui dit salut », « envoie salut à Matt », « réponds-lui que oui », « installe l'extension » |
| Ton écran | « qu'est-ce qu'il y a sur mon écran ? », « regarde cette vidéo, je n'ai pas compris », « explique-moi cet exercice », « lis ce message d'erreur » |
| Souris et clavier | « clique sur le bouton S'abonner », « double-clique sur le dossier Photos », « écris bonjour tout le monde », « appuie sur entrée », « fais contrôle C », « descends », « remonte » |
| PC | « quelle heure est-il ? », « verrouille l'ordinateur », « coupe ton micro », « éteins-toi » |
| Voix | « change de voix », « prends une voix d'homme », « voix anglaise », « voix Pierre » |
| Tout le reste | pose ta question normalement : il se souvient des 3 derniers échanges (« et demain ? ») |
| Repartir de zéro | « oublie tout » |

- Pour **couper Jarvis pendant qu'il parle**, redis « Hey Jarvis ».
- Avant d'envoyer un message, de forcer la fermeture d'une appli ou de cliquer sur un bouton risqué (supprimer, payer, envoyer…), il pose la question : réponds **« oui »** (ou « très bien », « vas-y »…) ou **« non »**.
- **Ton écran** : Jarvis fait une capture d'écran et l'IA locale la lit. L'image ne quitte pas ton PC. Pour cliquer, il trouve tout seul où est le bouton (en environ 0,5 s).
- Tu peux aussi **écrire** à Jarvis dans le chat de la barre Givre : c'est le même cerveau, avec la même mémoire, et il peut aussi agir.

## Le menu de l'icône (clic droit sur le « J »)

- **Parler maintenant** : comme « Hey Jarvis » (clic gauche aussi).
- **Micro activé** : coupe ou rallume l'écoute. Micro coupé, Windows n'affiche plus « micro utilisé ».
- **Voix** : choisis la voix de Jarvis. Une voix pas encore téléchargée l'est la première fois (Kokoro ≈ 340 Mo, Piper ≈ 60 Mo).
  - Siwis HD : femme, la plus naturelle (par défaut).
  - Jarvis : homme à l'accent anglais, façon Iron Man (expérimental).
  - Tom, Pierre : hommes.
  - Jessica, Siwis : femmes.
- **Démarrer avec Windows**.
- **Ouvrir la barre Givre**, **Réglages (config.toml)**, **Journal**.
- **Redémarrer Jarvis** (après avoir changé un réglage), **Quitter Jarvis**.

## Réglages (config.toml)

Ouvre-les depuis le menu de l'icône, modifie, enregistre, puis choisis **Redémarrer Jarvis**. Les réglages principaux :

- `[micro]` : quel micro, sensibilité du « Hey Jarvis » (`seuil_activation`), silence qui termine ta phrase (`silence_fin`), temps pour commencer à parler (`attente_debut`), écoute après sa réponse (`conversation`, 0 = désactivée).
- `[voix]` : la voix (`siwis_hd`, `jarvis`, `tom`, `pierre`, `jessica`, `siwis`), sa vitesse (`lenteur`) et son volume.
- `[cerveau]` : le modèle d'IA (`qwen3.5:4b`), combien d'échanges il retient, combien de temps il reste en mémoire graphique (`"0"` pour la libérer tout de suite quand tu joues).
- `[annonces]` : ce que Jarvis annonce à voix haute (Pronote, mails, Discord, minuteurs), et un créneau « ne pas déranger » (ex. `"22:00-07:00"`).
- `[applis]` et `[sites]` : tes surnoms (« navigateur » = Google Chrome…) et les sites que « ouvre … » connaît.
- `[general] lancer_givre` : lancer ou non la barre Givre avec Jarvis.
- `[deezer] utilisateur` : ton numéro de profil Deezer (dans l'adresse de ton profil), pour que « mets la playlist … » trouve d'abord TES playlists. Ton profil doit être public.

## Extension Chrome (vidéos + Discord)

1. Ouvre `chrome://extensions` et active le **Mode développeur** (en haut à droite).
2. Clique sur **Charger l'extension non empaquetée**, puis choisis le dossier `extension-chrome` de Jarvis.
3. Recharge ton onglet Discord (F5) et garde-le ouvert, même en arrière-plan.

L'icône de l'extension affiche « off » quand Jarvis n'est pas lancé.

> ⚠️ Automatiser son compte Discord est contraire à ses conditions d'utilisation, même sans selfbot ni jeton. Jarvis demande toujours ton « oui » avant d'envoyer, mais utilise-le avec modération. Les messages vocaux Discord ne peuvent pas s'envoyer depuis le web : Jarvis écrit ce que tu dictes.

## Si ça ne marche pas

- **Il ne réagit pas à « Hey Jarvis »** : baisse `seuil_activation` (0.35). **Il réagit tout seul** : monte-le (0.6–0.7).
- **Mauvais micro** : mets un morceau de son nom dans `[micro] appareil` (ex. `"Razer"`).
- **« Je n'arrive pas à joindre Ollama »** : lance Ollama (l'icône du lama).
- **Jarvis ne démarre plus, ou un fichier a disparu** : ton antivirus l'a peut-être mis en quarantaine. Restaure-le, autorise le dossier de Jarvis, puis relance l'installeur. Ce qui est déjà installé n'est pas retéléchargé.
- **Pour comprendre ce qui se passe** : menu de l'icône → **Journal** (`%APPDATA%\Jarvis\jarvis.log`).

## Comment c'est fait

| Pièce | Outil | Où ça tourne |
|---|---|---|
| « Hey Jarvis » | openWakeWord | processeur, très léger |
| Ta voix → texte | faster-whisper `large-v3-turbo` | carte graphique (0,2 s) |
| Réflexion et actions | Ollama `qwen3.5:4b` + raccourcis instantanés | carte graphique (0,3 à 0,8 s) |
| Texte → voix | Kokoro (voix HD) ou Piper (voix rapides) | processeur |

Jarvis utilise environ 5 Go de mémoire graphique. Les commandes simples (« pause », « lance Discord », « minuteur 5 minutes ») passent par des **raccourcis**, sans même réveiller l'IA. Pour le reste, l'IA choisit les **outils** à appeler, et Jarvis les exécute vraiment.

Pour les développeurs :
- `main.py` : la boucle (écoute → texte → cerveau → voix) ;
- `core/` : micro, voix, cerveau, API locale `127.0.0.1:47801`, icône, lien avec Givre ;
- `modules/` : une fonction par fichier. Pour en ajouter une, crée un fichier avec `installer(ctx, reg)` (exemple en haut de `modules/__init__.py`) ;
- `extension-chrome/`, `lanceur/` (Jarvis.exe), `installeur/` (Installer-Jarvis.exe), `outils/construire.ps1` (fabrique l'installeur), `tests/`.
