# Jarvis + Givre

**Un assistant vocal 100 % local et gratuit pour Windows, et une barre « Dynamic Island » en haut de l'écran.**

Dis **« Hey Jarvis »**, puis ce que tu veux : il lance tes applis, met la vidéo en pause, règle un minuteur, te dit tes devoirs Pronote, te lit tes messages Discord… Tout tourne sur ton PC : pas d'abonnement, pas de serveur, rien n'est envoyé à une IA en ligne.

## Ce que ça fait

**Jarvis (la voix)**
- « Hey Jarvis, lance Discord », « ferme Spotify », « ouvre YouTube », « cherche des tutos Blender sur YouTube »
- « Mets pause », « suivante », « avance de 30 secondes », « baisse le son », « c'est quoi la chanson qui passe ? »
- « Mets un minuteur de 5 minutes pour les pâtes », « réveille-moi à 7 h 30 », « lance le chrono »
- « J'ai quoi comme devoirs demain ? », « c'est quoi ma dernière note ? », « j'ai des mails importants ? », « quel temps fait-il ? »
- « Lis mes messages Discord », « réponds à Tom que j'arrive » (il te relit le message et attend ton « oui »)
- « Qu'est-ce qu'il y a sur mon écran ? », « regarde cette vidéo, je n'ai pas compris » : il lit ton écran avec l'IA locale
- « Clique sur le bouton S'abonner », « écris bonjour », « appuie sur entrée », « descends » : il contrôle la souris et le clavier
- Six voix au choix, et il t'écoute encore 5 secondes après sa réponse pour continuer la discussion
- Il annonce à voix haute les nouvelles notes, les mails importants et les messages privés Discord.

**Givre (la barre en haut de l'écran)**
- Une pilule qui s'ouvre au passage de la souris : heure, météo, Pronote (cours, devoirs, notes), mails importants, minuteurs, chrono, alarme, chat avec l'IA, messages Discord, notifications.
- Elle montre aussi si Jarvis t'écoute, réfléchit ou parle.
- Plus de 120 options de personnalisation : thèmes, couleurs, effets de verre, animations… et des options rigolotes (chat qui se promène, confettis pour une bonne note, mode pirate).

## Installer (pour tout le monde)

1. Il faut **Windows 10 ou 11** (64 bits), un **micro**, et environ **8 Go** d'espace libre. Une carte graphique **NVIDIA** est conseillée : sans elle, Jarvis marche aussi, mais plus lentement.
2. Télécharge **`Installer-Jarvis.exe`** dans la page **Releases** de ce dépôt.
3. Double-clique dessus, laisse les cases cochées, clique **Installer**. Il télécharge Python, les modèles de voix et le moteur d'IA [Ollama](https://ollama.com) (gratuit). Compte 10 à 30 minutes selon ta connexion.
4. Clique **Lancer Jarvis**. L'icône bleue « J » apparaît près de l'horloge, et la barre en haut de l'écran. Dis **« Hey Jarvis »** (prononcé à l'anglaise).

Pour **Discord et les vidéos dans Chrome**, clique aussi sur **Extension Chrome…** à la fin de l'installation et suis les 4 étapes.

Pour **Pronote et les mails**, ouvre les réglages de la barre (clic droit sur la barre).

**Mises à jour automatiques** : Jarvis vérifie au démarrage, puis toutes les 6 h, si une nouvelle version est sortie ici. Si oui, il te prévient, se met à jour tout seul en gardant tes réglages, et redémarre. Pour couper ça : `auto = false` dans la section `[mise_a_jour]` de `config.toml`.

Pour désinstaller : Paramètres Windows → Applications → **Jarvis** → Désinstaller.

## ⚠️ À savoir

- **Antivirus** : les programmes neufs et non signés qui tournent en arrière-plan, écoutent le micro et lancent d'autres programmes ressemblent à des logiciels espions. Certains antivirus (F-Secure, par exemple) peuvent donc bloquer Jarvis par erreur. Si ça arrive, restaure les fichiers depuis la quarantaine et autorise le dossier `%LOCALAPPDATA%\Programs\Jarvis`. Le code est entièrement lisible ici.
- **Discord** : Jarvis lit les notifications de ton onglet Discord dans Chrome et peut y écrire un message à ta place, toujours après ton « oui ». Il n'utilise ni selfbot ni jeton de compte. Mais **automatiser un compte utilisateur reste contraire aux conditions d'utilisation de Discord**, avec un risque de suspension. À utiliser avec modération, à tes risques.
- **Vie privée** : la voix, l'IA et la synthèse vocale tournent sur ton PC. Internet sert seulement pour la météo, Pronote, tes mails, et pour télécharger les modèles la première fois.

## Pour les développeurs

```
givre/            la barre (Rust) : île, widgets, Pronote, mails, API locale 127.0.0.1:47800
jarvis/           l'assistant (Python) : voix, IA, actions, API locale 127.0.0.1:47801
  main.py           boucle principale (« Hey Jarvis » -> texte -> cerveau -> voix)
  core/             micro, transcription (faster-whisper), voix (Piper), cerveau (Ollama), API
  modules/          une fonction par fichier : applis, médias, minuteurs, Discord… (facile d'en ajouter)
  extension-chrome/ l'extension Chrome (vidéos + Discord)
  lanceur/          Jarvis.exe (Rust) : lance Jarvis sans fenêtre
  installeur/       Installer-Jarvis.exe (Rust)
  outils/           construire.ps1 (fabrique l'installeur), téléchargement des modèles
```

- Construire l'installeur : `powershell -ExecutionPolicy Bypass -File jarvis\outils\construire.ps1` (il faut [Rust](https://rustup.rs) et [uv](https://docs.astral.sh/uv/)).
- Publier une nouvelle version (tout le monde la reçoit automatiquement) : `powershell -ExecutionPolicy Bypass -File jarvis\outils\publier.ps1 -Version 1.0.1 -Notes "Ce qui change"` (il faut être connecté avec `gh auth login`).
- Lancer depuis les sources : `jarvis\Installer-Jarvis.bat` (prépare Python et les modèles), puis `jarvis\Lancer-Jarvis.bat` (avec une console pour voir ce qui se passe).
- Tests : `jarvis\.venv\Scripts\python.exe -m pytest jarvis\tests` et `jarvis\.venv\Scripts\python.exe jarvis\main.py --test` (15 commandes, actions simulées) ; `cargo test` dans `givre`.
- Ajouter une fonction à Jarvis : crée `jarvis/modules/mon_module.py` avec une fonction `installer(ctx, reg)` (exemple en haut de `modules/__init__.py`).
