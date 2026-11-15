# Givre — widgets de bureau en verre flou

Heure, date, météo (ville réglable), calendrier, compte à rebours des vacances scolaires et Pronote.
Écrit en Rust, en Win32 natif : un seul `Givre.exe`, sans navigateur caché.
Environ 6 Mo de RAM quand les widgets sont cachés, 0 % de processeur au repos.

## Lancer
Double-clique sur `Givre.exe`. Une icône (flocon bleu) apparaît près de l'horloge de Windows.

**Les widgets ne s'affichent que quand toutes les fenêtres sont réduites** (Win+D, ou tout réduire).
Dès qu'une fenêtre est ouverte, ils se cachent et libèrent leur mémoire.

## Utilisation
- **Glisser un widget** : le déplace (aimanté à une grille). **Ctrl + molette** : change sa taille.
  Menu clic droit › **Verrouiller la disposition** pour éviter de les bouger par erreur.
- **Clic sur l'icône** : paramètres. **Clic droit** : menu (disposition, verrou, actualiser, démarrage, quitter).
- **Double-clic sur un widget** : paramètres.
- **Calendrier** : flèches ou molette pour changer de mois ; **clic sur un jour** pour voir sa journée (cours, devoirs,
  événements), **2e clic sur ce jour** pour créer un événement Google. « Aujourd'hui » revient au mois en cours.
- **Devoirs** : clic sur un devoir pour le cocher / décocher (c'est aussi coché dans Pronote).
- **Infos Pronote** : clic pour ouvrir Pronote dans le navigateur.
- **Météo** : clic pour actualiser (sinon toutes les 30 min).
- **Musique** : boutons précédent / lecture-pause / suivant (Spotify, YouTube, Deezer…).
- **Imprimante 3D** : clic pour ouvrir le widget vidéo.

Widgets : heure, météo, calendrier (+ Google Agenda et Pronote), vacances, mails (Gmail + Hotmail, tri et résumé par IA),
musique, Discord, imprimante 3D, et pour Pronote : prochain cours, devoirs, notes, annonces et messages. Chacun s'active ou se désactive dans Paramètres › Widgets.

## Paramètres
- **Apparence** : thème sombre/clair/auto, épaisseur des chiffres, taille, flou, teinte, ombre, bordure,
  arrondi des coins (et coins « continus » style iPhone), couleur d'accent (ou tirée du fond d'écran).
- **Widgets** : afficher/masquer chaque widget, secondes, 24 h, ville de la météo, académie, date du brevet.
- **Général** : affichage seulement bureau vide, lancement au démarrage de Windows, grille, disposition par défaut.

Réglages et journal : `%APPDATA%\Givre\` (`config.json`, `givre.log`).

## Recompiler
```
cargo build --release
```
Puis copier `target\release\givre.exe` ici sous le nom `Givre.exe`.
`givre.exe --apercu DOSSIER` dessine les widgets dans des images PNG, sans rien afficher.

---

## À préparer pour la prochaine étape : Google (Agenda + Gmail), environ 10 min

1. Va sur <https://console.cloud.google.com> avec ton compte Gmail principal.
2. En haut : **Sélectionner un projet › Nouveau projet**, nom `Givre`, puis **Créer**.
3. Menu **API et services › Bibliothèque** : active **Google Calendar API**, puis **Gmail API**.
4. Menu **Google Auth Platform** (ou « Écran de consentement OAuth ») › **Commencer** :
   nom de l'appli `Givre`, ton adresse mail, audience **Externe**, puis termine l'assistant.
5. **Audience › Utilisateurs test › Ajouter** : ajoute **chacune de tes adresses Gmail**.
6. **Clients › Créer un client** : type **Application de bureau**, nom `Givre`, puis **Créer**.
   Clique sur **Télécharger le JSON**.
7. Renomme ce fichier `google.json` et mets-le dans `%APPDATA%\Givre\`.
   Pour y aller : tape `%APPDATA%\Givre` dans la barre d'adresse de l'Explorateur.

Si Google refuse de créer le projet (compte d'un mineur), un parent peut le créer avec son compte
et t'ajouter comme utilisateur test : ça marche pareil.

8. Dans Givre : **Paramètres › Comptes › Google › Ajouter**. Ton navigateur s'ouvre : choisis ton compte,
   accepte (Google prévient que l'appli n'est pas vérifiée : c'est normal, c'est la tienne).
   Recommence pour chaque adresse Gmail.

## Hotmail (environ 10 min)
1. Va sur <https://entra.microsoft.com> avec ton adresse Hotmail
   (si Microsoft demande de créer un compte Azure gratuit, un parent peut le faire).
2. **Applications › Inscriptions d'applications › Nouvelle inscription** : nom `Givre`,
   type de comptes **« Comptes Microsoft personnels uniquement »**, puis **S'inscrire**.
3. **Authentification › Paramètres avancés › Autoriser les flux de clients publics : Oui**, puis **Enregistrer**.
4. Copie l'**ID d'application (client)** dans Givre : **Paramètres › Comptes › ID d'application Microsoft**,
   puis **Connecter Hotmail**. Une page Microsoft s'ouvre : tape le code affiché dans Givre.

## Discord (environ 5 min)
1. Va sur <https://discord.com/developers/applications>, **New Application**, nom `Givre`.
2. Onglet **OAuth2** : copie le **Client ID**, puis **Reset Secret** et copie le **Client Secret**.
3. Toujours dans OAuth2, **Redirects › Add Redirect** : `http://localhost`, puis **Save Changes**.
4. Dans Givre : **Paramètres › Comptes › Discord** : colle l'ID et le secret, puis **Autoriser**.
   Une fenêtre s'ouvre dans l'appli Discord : accepte.
   Si Givre affiche une erreur d'autorisation, Discord bloque encore cette fonction : dis-le-moi.

## Résumé des mails par IA (Ollama, gratuit, sur ton PC)
1. Télécharge et installe Ollama depuis <https://ollama.com>.
2. Ouvre un terminal et tape : `ollama pull gemma3:4b` (environ 3 Go).
3. C'est tout : Givre résume et trie tes mails non lus. Le modèle est déchargé après chaque résumé,
   donc il ne garde pas de mémoire. Sans Ollama, Givre trie avec des règles simples.

## Pronote (2 min)
1. Ouvre Pronote dans ton navigateur et affiche le **QR code pour l'appli mobile** (icône QR code en haut de la page).
   Pronote te demande de choisir un **code à 4 chiffres** : retiens-le.
2. Fais **Win + Maj + S** et capture le QR code (en entier, bien net).
3. Dans Givre : **Paramètres › Comptes › Pronote › Coller** (ou Ctrl+V sur cette page), tape ton code,
   puis **Se connecter**. Le QR code n'est valable que quelques minutes : si c'est refusé, refais-en un.

Givre garde seulement le **jeton** de l'appli mobile (il change à chaque connexion), jamais ton mot de passe EduConnect.
Mise à jour toutes les 30 min (et au réveil du PC). Notifications Windows : nouvelle note, nouveau devoir,
cours annulé ou prof absent (réglables dans Paramètres › Widgets › Pronote).

**Copie dans Google Agenda** : Paramètres › Comptes › Pronote › « Copier cours et devoirs dans Google Agenda ».
Givre crée l'agenda « Pronote (Givre) » dans ton premier compte Google et y copie tes cours (3 semaines) et devoirs.
Il faut avoir connecté Google *avec cette version* de Givre (elle demande en plus le droit de créer un agenda).

Pour vérifier que Pronote répond : `givre.exe --pronote-test DOSSIER` (sans compte, écrit `pronote_test.txt`).

## Dynamic Island (barre en haut de l'écran)

Test : double-clic sur `Lancer-Ile.bat` (ou `givre.exe --ile` ; `givre.exe --widgets` pour revenir aux widgets).
Survole la pilule en haut de l'écran pour l'ouvrir. Clic droit sur l'île : menu (mode île, réglages de l'île).

- **Clic** sur un devoir, une note, un mail ou un message Discord : la page de détail s'ouvre dans l'île.
  **Double-clic** : ouvre la vraie page (Pronote, le mail dans Gmail, Discord dans le navigateur).
- Dans la page Devoirs, cliquer le rond coche / décoche le devoir (comme dans Pronote).
- **Chrono et minuteurs** : boutons +1/+5/+10 min (réglables), pause, suppression ; le minuteur en cours reste visible dans la pilule.
- **Alarme** : interrupteur, heures et minutes avec − / +, jours (clic sur « tous les jours »).
- **Assistant** : clique le champ, écris, Entrée (Échap : annuler, Ctrl+V : coller). Il se souvient des derniers échanges
  (3 par défaut), connaît Pronote, les mails, la météo et les minuteurs, et peut lancer un minuteur, régler l'alarme,
  piloter le chrono, ouvrir un mail ou Pronote. Modèle par défaut : `qwen2.5:7b` (sinon celui des réglages mails).
- **Discord (lecture seule)** : les MP sont lus dans les notifications Windows (appli Discord ou Discord dans Chrome/Edge),
  et le nombre de non-lus dans le titre de l'onglet Chrome « (3) Discord ». Aucun jeton, aucun pilotage du compte.

### Réglages de l'île
Engrenage en haut à droite de l'île ouverte (ou clic droit › « Réglages de l'île… »). Plus de 120 options en 13 onglets :
Apparence, Effets (verre, flou, lueur, ombre, fondu), Taille et position, Texte (police, taille, gras, mode pirate),
Heure et date, Météo (unités, icônes animées, fond coloré, petite phrase), Modules (affichés + ordre), Minuteurs et alarme,
Pronote et mails (fréquence de vérification des mails, ce qui compte comme important…), IA et Jarvis, Notifications
(source par source, dans l'île et/ou Windows, ne pas déranger), Animations, Options rigolotes (arc-en-ciel, disco, confettis,
petit animal, yeux, neige, compliments). Clic gauche : valeur suivante ; clic droit : précédente. Tout est enregistré dans
`%APPDATA%\Givre\config.json` (bloc `"ile"`, chaque option est commentée dans `src/config.rs`).

### API locale pour Jarvis
`http://127.0.0.1:47800` (seulement ce PC ; port réglable, désactivable). JSON en UTF-8.
- `GET /etat` → `{"ok":true, "heure", "date", "meteo", "pronote", "mails", "minuteurs", "chrono", "alarme", "discord", "jarvis"}`
- `POST /demander` `{"texte":"…","source":"jarvis"}` → `{"ok":true,"reponse":"…"}` (même IA et même mémoire que l'île, jusqu'à ~2 min)
- `POST /jarvis` `{"etat":"repos|ecoute|reflechit|parle","texte":"…"}` → `{"ok":true}` (indicateur dans l'île)
- `POST /notif` `{"titre":"…","texte":"…","source":"jarvis|pronote|mail|discord|minuteur"}` → `{"ok":true}`
Les requêtes envoyées par une page web (en-tête `Origin`) sont refusées.

Profil de test séparé : `set GIVRE_PROFIL=C:\un\dossier` avant de lancer `givre.exe` (réglages et comptes à part,
peut tourner à côté du Givre normal).

## Confidentialité
Tes connexions (Google, Hotmail, Discord, Pronote) sont dans `%APPDATA%\Givre\comptes.bin`, chiffré par Windows :
seule ta session peut le lire. Les données Pronote (cours, notes…) sont gardées dans `pronote.bin`, chiffré de la même façon. Aucun mot de passe n'est enregistré.
