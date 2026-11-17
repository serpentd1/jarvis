# Jarvis — mode d'emploi

Dis **« Hey Jarvis »** (prononcé à l'anglaise : « Hé Djarvisse »), attends le petit « ding », puis parle. Tu peux aussi enchaîner directement : « Hey Jarvis, lance Discord ».

- Tu as **6 secondes** pour commencer à parler, et tu peux faire de petites pauses : il attend 1,4 s de silence avant de considérer que tu as fini.
- Après sa réponse, Jarvis **continue de t'écouter sans « ding »** : tu peux poursuivre sans redire « Hey Jarvis ». Dis « stop », « Jarvis stop », « c'est bon » ou « au revoir » pour terminer ; après 60 secondes sans voix, il revient tout seul en attente.
- Tu peux dire « Hey Jarvis, mets pause » d'une traite ou « Mets pause, Jarvis » : Jarvis garde les 2 secondes qui précèdent le mot d'activation.
- Pour réviser, copie ta leçon puis dis « apprends ma leçon de maths » (ou « apprends ma leçon de maths qui est sur l'écran »). Ensuite, dis « fais-moi réciter ma leçon de maths » ; il pose une question à la fois, te corrige et te donne ton score quand tu dis « stop ».

Jarvis est **une seule appli** : la voix, l'île en haut de l'écran et les widgets du bureau (l'ancien Givre). Il n'y a plus qu'**une seule icône** près de l'horloge : le carré bleu en verre avec un « J » et un flocon. Un seul raccourci aussi : `Jarvis.exe`.

- `Jarvis.exe` démarre la voix (Python), qui démarre `JarvisBarre.exe` (l'île + les widgets + l'icône). Si on lance `JarvisBarre.exe` en premier, il démarre la voix tout seul.
- L'ancien `Givre.exe` n'est plus lancé à part : au premier démarrage, Jarvis le ferme, reprend la disposition de ses widgets et remplace son démarrage avec Windows par celui de Jarvis. Si on relance `Givre.exe` pendant que Jarvis tourne, il ouvre juste les réglages de Jarvis.
- L'île montre l'état de Jarvis (écoute, réfléchit, parle). L'ancienne icône « J » de couleur ne sert plus que si `JarvisBarre.exe` est introuvable.

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
| Tâches | « ajoute payer les factures à mes tâches », « ajoute du pain à la liste courses », « qu'est-ce que j'ai à faire ? », « j'ai fini les factures » |
| Rappels | « rappelle-moi dans 20 minutes de sortir le chien », « rappelle-moi demain à 18 h d'appeler mamie » |
| Habitudes | « crée l'habitude lire 10 pages », « j'ai fait mes pompes », « où j'en suis de mes habitudes ? » |
| Leçons | Copie ta leçon puis « apprends ma leçon de maths », ou « … qui est sur l'écran » ; puis « fais-moi réciter ma leçon de maths » |
| Routines | « lance la routine du matin », « mode devoirs », ou « bonjour » (modifiables dans la grande île) |
| Mémoire | « souviens-toi que mon meilleur ami s'appelle Tom », « qu'est-ce que tu sais sur moi ? », « oublie Tom »  ; dans la grande île, ajoute des notes et relie-les sur la carte |
| Grande île | « ouvre l'application Jarvis », « montre mes tâches », « ouvre mes rappels », « montre ta mémoire », « montre mes batteries » (l'île se déplie en grand) |
| Micro | « règle mon micro » : il mesure le bruit de la pièce puis ta voix, et règle tout seul le volume de la conversation continue |
| PC et batteries | « comment va mon PC ? », « éteins l'écran », « c'est quoi la batterie de ma manette ? » |
| Claude | « qu'est-ce que Claude fait ? » (et tes limites d'utilisation si le relais de barre d'état est branché) |
| Téléphone | « fais sonner mon téléphone », « où est mon portable ? », « j'ai reçu des SMS ? », « qui m'a appelé ? » (il annonce aussi tout seul les SMS et les appels) |
| Tout le reste | pose ta question normalement : il se souvient des 3 derniers échanges (« et demain ? ») et de tout ce qu'il a appris sur toi |
| Repartir de zéro | « oublie tout » |

- Pour **couper Jarvis pendant qu'il parle**, parle-lui simplement (2 ou 3 mots suffisent) ou redis « Hey Jarvis » : il s'arrête net et t'écoute. S'il se coupe tout seul à cause du son de ses haut-parleurs, mets `couper_en_parlant = false` dans `[micro]`.
- Quand Jarvis **ne sait pas faire** quelque chose, il note ta demande dans `%APPDATA%\Jarvis\donnees\a_ajouter.json`, pour qu'on la lui ajoute plus tard. « Qu'est-ce que tu ne sais pas faire ? » te lit les dernières.
- Pour **arrêter la conversation continue**, dis « stop », « Jarvis stop », « c'est bon » ou « au revoir ».
- Avant d'envoyer un message, de forcer la fermeture d'une appli ou de cliquer sur un bouton risqué (supprimer, payer, envoyer…), il pose la question : réponds **« oui »** (ou « très bien », « vas-y »…) ou **« non »**.
- **Ton écran** : Jarvis fait une capture d'écran et l'IA locale la lit. L'image ne quitte pas ton PC. Pour cliquer, il trouve tout seul où est le bouton (en environ 0,5 s).
- Tu peux aussi **écrire** à Jarvis dans le chat de son île : c'est le même cerveau, avec la même mémoire, et il peut aussi agir.
- **Mémoire** : Jarvis essaie de retenir les faits durables des échanges, mais pas chaque phrase. Dans la grande île, tu peux ajouter une note, relier deux notes avec une relation, retirer un lien ou oublier une note. La mémoire est enregistrée localement dans `%APPDATA%\Jarvis\donnees\memoire.json` et n'est pas chiffrée : n'y mets pas de mot de passe, code, clé API ou jeton. Les secrets connus sont refusés.

## Le menu de l'icône (clic droit sur l'icône bleue près de l'horloge)

- **Parler à Jarvis** : comme « Hey Jarvis ».
- **Micro activé** : coupe ou rallume l'écoute. Micro coupé, Windows n'affiche plus « micro utilisé ».
- **Ouvrir l'application Jarvis** (la grande île), **Mes comptes** (Pronote, mails…). Tu n'utilises pas Pronote ou les mails ? Dans **Mes comptes**, clique sur **Désactiver** : plus d'alerte « pas connecté », et Jarvis n'en parle plus.
- Le raccourci **Jarvis** du bureau lance Jarvis s'il ne tourne pas, et ouvre la grande île s'il tourne déjà.
- **Île** : afficher l'île, grande île, cacher automatiquement, notes Pronote, réglages de l'île.
- **Widgets du bureau** : les afficher ou non, modifier / verrouiller la disposition, actualiser.
- **Paramètres…** (clic gauche aussi), **Réglages de la voix (config.toml)**, **Journal de Jarvis**.
- **Lancer au démarrage de Windows** : une seule entrée « Jarvis » (elle remplace les anciennes « JarvisBarre » et « Givre »).
- **Redémarrer Jarvis** (après avoir changé un réglage : l'île reste ouverte), **Quitter Jarvis** (tout s'arrête : voix, île, widgets).
- **Voix** : dis « change de voix » ou choisis-la dans les paramètres de la grande île. Une voix pas encore téléchargée l'est la première fois (Kokoro ≈ 340 Mo, Piper ≈ 60 Mo).

## Pouvoirs sur le PC (écrits en Rust)

Jarvis peut agir sur ton PC, à la voix ou dans le chat de l'île :

| Pour… | Exemples |
|---|---|
| Fichiers | « crée un fichier notes.txt sur le bureau qui dit … », « lis le fichier … », « ajoute … à la fin de … », « remplace … par … dans … », « déplace / copie / renomme … », « supprime … » |
| Dossiers | « crée un dossier Exposé sur le bureau », « qu'est-ce qu'il y a sur le bureau ? » |
| Code | « modifie mon fichier documents\projet\main.py pour ajouter un bouton » (l'IA réécrit, tu confirmes) |
| Pages web | « crée une page web sur les dinosaures » (ouverte dans le navigateur), « modifie la page dinosaures : fond noir » |
| Programmes | « lance Discord », « ouvre le fichier … », « tue le processus chrome » |
| PowerShell | « lance la commande PowerShell Get-Process » (le résultat est lu) |
| PC | « éteins le PC », « redémarre l'ordi », « mets le PC en veille », « verrouille le PC », « annule l'extinction » |

Sécurité :
- Lire, lister, créer un **nouveau** fichier ou dossier, lancer une appli : tout de suite.
- Écraser, modifier, supprimer, ouvrir un programme (.exe, .bat…), tuer un processus, éteindre / redémarrer / veille, PowerShell qui change quelque chose : Jarvis **demande « tu confirmes ? »** et attend ton « oui » (3 minutes au plus).
- Supprimer = **corbeille**. Avant chaque écrasement ou modification, une **copie** va dans `%APPDATA%\Jarvis\donnees\sauvegardes`.
- Windows, Program Files, ProgramData, la racine des disques… sont **interdits** en écriture.
- PowerShell est **coupé après 30 s** ; les commandes dangereuses (formater, couper l'antivirus, télécharger, toucher au registre système…) sont **refusées**.
- Tout est noté dans `%APPDATA%\Jarvis\donnees\actions.log`.
- Fichiers sans chemin : dans `Documents\Jarvis` (les pages web dans `Documents\Jarvis\pages web`).
- Pour tout couper : menu de l'icône → **Pouvoirs sur le PC**. Réglages fins dans `%APPDATA%\Jarvis\barre\config.json`, section `"pouvoirs"` : `actif`, `powershell`, `delai_powershell`, `dossier_travail`, `corbeille`, `chemins_interdits`.

Tout cela est dans `barre\src\pouvoirs.rs` (Rust). La voix (encore en Python) passe par `modules\pc_rust.py`, un simple pont vers l'API de la barre (`GET /outils`, `POST /outil`, `POST /outil/confirmer`).

## Ton téléphone Android relié à Jarvis (Wi-Fi local)

Avec la petite appli **Jarvis Téléphone** (dossier `appli-telephone`, voir son LISEZMOI pour l'installer) :

| Pour… | Dis au PC (ou écris dans l'île) |
|---|---|
| Alarme / minuteur sur le téléphone | « mets une alarme à 7 h 30 sur mon téléphone », « lance un minuteur de 10 minutes sur mon téléphone » |
| Le retrouver | « fais sonner mon téléphone » (à fond même en silencieux, vibreur, flash), « arrête de sonner », « où est mon téléphone ? » |
| Son état | « quelle est la batterie de mon téléphone ? » |
| Messages | « est-ce que j'ai eu un message important ? », « lis mes derniers SMS », « réponds à Léa sur mon téléphone que j'arrive » (confirmation) |
| Appels | « appelle Maman » (confirmation) |
| Réglages | « mets mon téléphone en silencieux / vibreur / sonnerie », « allume la lampe de mon téléphone », « prends une photo » |

- Les notifications du téléphone arrivent dans l'île (comme avec « Mobile connecté »). « Message important » = un appel, une personne de ta liste (réglage « personnes importantes » de Discord dans les paramètres), ou un mot comme urgent, vite, rappelle, code, vérification, maman, papa, collège, prof…
- Depuis le téléphone, le bouton **Parler à Jarvis** pose ta question à Jarvis sur le PC (même cerveau, même mémoire) et le téléphone lit la réponse.
- Relier : menu de l'icône → **Relier un téléphone…** (adresse + code à 6 chiffres). Délier : **Délier le téléphone**. Le PC n'écoute (port 47803) que si un téléphone est relié, et seulement le réseau local, avec un jeton secret.
- Sans appli : « fais sonner mon téléphone » ouvre « Localiser mon appareil » de Google.

### Appeler quelqu'un (toujours avec confirmation)

- « Appelle maman avec WhatsApp » (ou « depuis WhatsApp », ou un numéro) : Jarvis ouvre la discussion dans l'appli WhatsApp du PC, te demande « Tu es sûr de vouloir appeler Maman avec WhatsApp ? » avec le vrai nom du contact, puis clique sur « Appel vocal » après ton « oui ». Il faut l'appli WhatsApp installée et connectée sur le PC.
- « Appelle maman avec WhatsApp sur mon téléphone » : appel vocal WhatsApp lancé par le téléphone (appli Jarvis Téléphone 1.1, autorisation Contacts).
- « Appelle maman » : appel normal avec le téléphone, après ton « oui ».
- « Appelle Tom sur Discord » : après ton « oui », l'extension Chrome (version 1.1.0) ouvre la conversation privée dans ton onglet Discord et clique sur le bouton d'appel vocal. Il faut un onglet Discord ouvert dans Chrome, et que Chrome ait le droit d'utiliser le micro sur discord.com.
- Si tu réponds « non », rien ne part.

## Jarvis et Givre : deux applis séparées

Jarvis (l'assistant, son île en haut de l'écran) n'affiche plus de widgets sur le bureau. Les widgets sont dans Givre (`givre\target\release\givre.exe`), une appli à part avec sa propre icône et ses propres réglages.

## Passage au 100 % Rust (en cours)

Déjà en Rust : l'île, les widgets, Pronote, mails, météo, minuteurs, l'icône et son menu, le lanceur `Jarvis.exe`, l'installeur, les pouvoirs sur le PC, la liaison avec le téléphone et **le cerveau** (`barre\src\cerveau.rs`) : un seul pour la voix, le chat de l'île et le téléphone.

Ce que fait le cerveau Rust :
- il ne propose à l'IA que les outils utiles à ta phrase (16 au plus, sur 75) : avant, la liste entière ne tenait pas dans la mémoire de travail du modèle ;
- quand il n'y a pas d'action à faire, la réponse arrive phrase par phrase : Jarvis commence à parler dès la première phrase prête ;
- il garde les appels d'outils dans la mémoire de la conversation et refuse que l'IA dise « c'est fait » ou « c'est supprimé » sans avoir appelé d'outil ;
- si tu dis « sur le bureau » et que l'IA oublie le dossier, il le remet ;
- pendant que Jarvis parle, l'île montre sa phrase mot par mot, au rythme de sa voix.

Encore en Python (`core\`, `modules\`, `main.py`) : la voix (micro, « Hey Jarvis », Whisper, voix Kokoro/Piper), les modules (applis, médias, Discord, Deezer, écran, clavier, mémoire, tâches, rappels, habitudes, routines, leçons…), l'API 47801 et l'extension Chrome. Le cerveau Rust lit leurs outils, raccourcis et données par les routes `POST :47801/cerveau/…`. `core\cerveau.py` ne sert plus que de registre, et de cerveau de secours si la barre ne répond pas. Suite prévue : porter les modules un par un, puis la voix, puis supprimer le Python.

L'icône est dessinée par `outils\icone_jarvis.py` (`.venv\Scripts\python.exe outils\icone_jarvis.py`) : elle écrit `lanceur\jarvis.ico`, `barre\jarvis.ico` (tailles 16 à 256) et les images de l'extension Chrome.

## Réglages (config.toml)

Ouvre-les depuis le menu de l'icône, modifie, enregistre, puis choisis **Redémarrer Jarvis**. Les réglages principaux :

- `[micro]` : quel micro, sensibilité du « Hey Jarvis » (`seuil_activation`), silence qui termine ta phrase (`silence_fin`), temps pour commencer à parler (`attente_debut`), écoute après sa réponse (`conversation_continue`, activée par défaut), délai de fin (`fin_conversation`, 60 s) et seuil de voix en conversation (`volume_conversation`, 350). Si Jarvis réagit tout seul, augmente ce seuil (par ex. 500) ; s'il ne t'entend pas, baisse-le (par ex. 200).
- `[voix]` : la voix (`siwis_hd`, `jarvis`, `tom`, `pierre`, `jessica`, `siwis`), sa vitesse (`lenteur`) et son volume.
- `[cerveau]` : le modèle d'IA (`qwen3.5:4b`), un petit modèle rapide facultatif pour les ordres (`modele_rapide`, vide par défaut), combien d'échanges il retient, combien de temps il reste en mémoire graphique (`"0"` pour la libérer tout de suite quand tu joues).
- `[annonces]` : ce que Jarvis annonce à voix haute (Pronote, mails, Discord, minuteurs), et un créneau « ne pas déranger » (ex. `"22:00-07:00"`).
- `[applis]` et `[sites]` : tes surnoms (« navigateur » = Google Chrome…) et les sites que « ouvre … » connaît.
- `[general] lancer_givre` : lancer ou non l'île Jarvis avec l'assistant vocal. Le nom de cette clé reste historique.
- `[deezer] utilisateur` : ton numéro de profil Deezer (dans l'adresse de ton profil), pour que « mets la playlist … » trouve d'abord TES playlists. Ton profil doit être public.

## Extension Chrome (vidéos + Discord)

1. Ouvre `chrome://extensions` et active le **Mode développeur** (en haut à droite).
2. Clique sur **Charger l'extension non empaquetée**, puis choisis le dossier `extension-chrome` de Jarvis.
3. Recharge ton onglet Discord (F5) et garde-le ouvert, même en arrière-plan.

L'icône de l'extension affiche « off » quand Jarvis n'est pas lancé.

> ⚠️ Automatiser son compte Discord est contraire à ses conditions d'utilisation, même sans selfbot ni jeton. Jarvis demande toujours ton « oui » avant d'envoyer, mais utilise-le avec modération. Les messages vocaux Discord ne peuvent pas s'envoyer depuis le web : Jarvis écrit ce que tu dictes.

## Ton téléphone Android (SMS, appels)

1. Sur le PC, ouvre l'appli **Mobile connecté** de Windows et relie ton téléphone (elle te fait installer « Lien avec Windows » dessus).
2. Dans Mobile connecté, termine la **configuration des notifications** : sur le téléphone, ouvre les réglages qu'elle t'indique et autorise « Lien avec Windows ».
3. C'est tout. L'île Jarvis reçoit les SMS, les appels et les notifications du téléphone (onglet Téléphone), et Jarvis annonce les SMS et les appels.

- Les **codes de vérification** (suites de chiffres) ne sont jamais lus à voix haute, ni donnés à l'IA, ni écrits dans le journal.
- Réglages dans `[annonces]` : `telephone` (SMS et appels) et `telephone_applis` (aussi WhatsApp, Instagram…, coupé au départ).
- « Fais sonner mon téléphone » ouvre la page Google **Localiser mon appareil** : clique sur « Faire sonner ».
- Le presse-papiers partagé et l'envoi de fichiers font partie de Mobile connecté lui-même : ça dépend de ton téléphone.

## Choisir son IA

L'IA se choisit dans les réglages de la barre : clic droit sur la barre, **Paramètres**, **IA**. Jarvis essaie toujours la version gratuite en premier.

1. **Gratuit sur le PC** : **LM Studio** (recommandé, modèle `qwen3-4b` téléchargé par l'installeur), Jan, Ollama.
2. **Gratuit en ligne, avec une clé** : Google Gemini, Groq, OpenRouter (modèles « :free »), Mistral, Cerebras.
3. **Payant, avec une clé** : OpenAI, Anthropic, xAI, DeepSeek…

Tu peux coller n'importe quelle clé d'API : Jarvis devine le service d'après la clé. Pour un service inconnu, colle aussi son adresse. Les clés sont chiffrées dans la barre. Dis « quelle IA ? » pour savoir laquelle Jarvis utilise.

## Si ça ne marche pas

- **Il ne réagit pas à « Hey Jarvis »** : baisse `seuil_activation` (0.35). **Il réagit tout seul** : monte-le (0.6–0.7).
- **Mauvais micro** : mets un morceau de son nom dans `[micro] appareil` (ex. `"Razer"`).
- **« Aucune IA ne répond »** : ouvre LM Studio (ou ajoute une clé gratuite : clic droit sur la barre, Paramètres, IA).
- **Jarvis ne démarre plus, ou un fichier a disparu** : ton antivirus l'a peut-être mis en quarantaine. Restaure-le, autorise le dossier de Jarvis, puis relance l'installeur. Ce qui est déjà installé n'est pas retéléchargé.
- **Pour comprendre ce qui se passe** : menu de l'icône → **Journal de Jarvis** (`%APPDATA%\Jarvis\jarvis.log`).

## Comment c'est fait

| Pièce | Outil | Où ça tourne |
|---|---|---|
| « Hey Jarvis » | openWakeWord | processeur, très léger |
| Ta voix → texte | faster-whisper `large-v3-turbo` | carte graphique (0,2 s) |
| Réflexion et actions | LM Studio `qwen3-4b` (ou la clé d'API choisie) + raccourcis instantanés | carte graphique (0,3 à 0,8 s) |
| Texte → voix | Kokoro (voix HD) | GPU CUDA si disponible, sinon processeur |
| Texte → voix | Piper (voix rapides) | processeur |

Jarvis utilise environ 5 Go de mémoire graphique. Les commandes simples (« pause », « lance Discord », « minuteur 5 minutes ») passent par des **raccourcis**, sans même réveiller l'IA. Pour le reste, l'IA choisit les **outils** à appeler, et Jarvis les exécute vraiment.

Pour les développeurs :
- `main.py` : la boucle (écoute → texte → cerveau → voix) ;
- `core/` : micro, voix, cerveau, API locale `127.0.0.1:47801`, icône et lien avec le moteur natif de l'île ;
- `modules/` : une fonction par fichier. Pour en ajouter une, crée un fichier avec `installer(ctx, reg)` (exemple en haut de `modules/__init__.py`) ;
- `extension-chrome/`, `lanceur/` (Jarvis.exe), `installeur/` (Installer-Jarvis.exe), `outils/construire.ps1` (fabrique l'installeur), `tests/`.
