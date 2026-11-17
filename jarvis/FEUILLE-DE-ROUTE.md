# Feuille de route Jarvis (idées 1 à 50 + téléphone)

Règle : quand l'utilisateur dit « fais l'étape suivante », on prend la **première case non cochée** ci-dessous, on la fait en entier (en **Rust uniquement**, voir la contrainte), on la teste, on coche `[x]`, puis on dit en une phrase ce qui a été fait et quelle est l'étape suivante. Une étape à la fois. Rien n'est commité sans demande.

Contraintes : tout en Rust (pas de nouveau Python), une seule appli/icône, actions risquées avec confirmation, `graphify update .` après chaque modif, tests `cargo test`.

## Voix et IA
- [x] 1. Montrer la réponse de Jarvis mot par mot dans l'île pendant qu'il parle. (fait le 2026-11-16 ; pas encore vérifié à l'œil)
- [ ] 2. Afficher dans la grande île le score du « Hey Jarvis » pour régler `seuil_activation` à la souris.
- [x] 3. Choisir le modèle Ollama selon la question : petit modèle rapide pour les ordres, `qwen3-vl` pour l'écran. (fait le 2026-11-16 : réglage `modele_rapide` dans config.toml, vide par défaut)
- [ ] 4. Bouton « Ce que tu ne sais pas faire » qui ouvre `a_ajouter.json` et propose de créer le module.
- [ ] 5. Plusieurs utilisateurs reconnus à la voix, chacun avec sa mémoire.
- [ ] 6. Mode « chuchote » : réponses plus courtes et volume plus bas le soir.
- [ ] 7. « Répète » : redire la dernière réponse sans rappeler l'IA.
- [ ] 8. Deuxième langue : comprendre aussi l'anglais pour les devoirs d'anglais.
- [ ] 9. Une voix différente selon le module (alarme, leçon, blague).
- [ ] 10. Résumé vocal du matin : météo, cours Pronote, devoirs, mails importants, habitudes.

## Téléphone Android (appli-telephone, liaison faite, APK fabriqué)
- [ ] T1. Mise à jour automatique de l'appli téléphone : elle demande au PC (même Wi-Fi) s'il y a une version plus récente, la télécharge et propose « Installer » (même clé de signature).
- [ ] T2. Chiffrer les échanges PC/téléphone avec le secret partagé.
- [ ] T3. Relier en scannant un QR code affiché par le PC.
- [ ] T4. Trouver le PC tout seul sur le Wi-Fi (sans taper l'adresse).
- [ ] T5. Tuile dans les réglages rapides + widget « Parler à Jarvis » sur l'écran d'accueil.
- [ ] T6. « Hey Jarvis » directement sur le téléphone.
- [ ] T7. Copier-coller partagé PC/téléphone et envoi de photos/fichiers vers le PC.
- [ ] T8. Quand on quitte la maison : alerte s'il manque des devoirs Pronote ; au retour, routine « je suis rentré ».
- [ ] T9. Rappels, tâches et minuteurs de Jarvis affichés/sonnant sur le téléphone.
- [ ] T10. Ne pas montrer deux fois la même notification (appli + « Mobile connecté »).
- [ ] T11. Musique du téléphone dans l'île du PC, avec pause/suivante.
- [ ] T12. Mode « sous la douche » : annonces regroupées en un résumé au retour.
- [ ] T13. Tableau « mon téléphone » dans la grande île (batterie, stockage, notifications, boutons sonner/silencieux/lampe).

## Mode devoirs (QCM, cartes, révisions)
- [ ] D1. Créer le mode devoirs : le prompt complet est prêt dans `PROMPT-DEVOIRS.md` (rien n'est codé).

## École et Pronote
- [ ] 11. « Fais-moi un quiz sur mon dernier cours » à partir des devoirs Pronote.
- [ ] 12. Moyenne prévisionnelle : « si j'ai 15 au prochain contrôle, ma moyenne passe à… ».
- [ ] 13. Rappel automatique la veille d'un contrôle repéré dans le cahier de textes.
- [ ] 14. Leçons apprises rangées par matière dans la grande île, avec cartes de révision.
- [ ] 15. Alerte « cours annulé » qui décale le réveil automatiquement.
- [ ] 16. Emploi du temps de la semaine dans un widget du bureau, avec les salles.
- [ ] 17. Minuteur Pomodoro de devoirs relié aux tâches (la barre a déjà `pomo_work`).
- [ ] 18. Compte à rebours du brevet (widget existant) annoncé chaque lundi.

## Île et widgets
- [ ] 19. Icône près de l'horloge qui change de couleur selon l'état de Jarvis.
- [ ] 20. Choisir en un clic quels widgets restent quand l'île est affichée.
- [ ] 21. Widget « Jarvis » sur le bureau : dernière question/réponse + bouton micro.
- [ ] 22. Widget imprimante 3D (avancement, photo de la caméra Elegoo).
- [ ] 23. Notification dans l'île à la fin d'une impression, avec la vidéo accélérée prête.
- [ ] 24. Thèmes d'île selon l'heure (sombre le soir, plus clair le matin).
- [ ] 25. Glisser un fichier sur l'île pour que Jarvis le résume.
- [ ] 26. Mini-lecteur Deezer dans l'île, avec les paroles.
- [ ] 27. Widget batteries (manette, casque, téléphone) avec alerte sous 20 %.
- [ ] 28. Raccourci clavier global (ex. Win+J) pour parler à Jarvis.

## PC et jeux
- [ ] 29. Mode jeu : couper les annonces et réduire l'île quand Steam/Roblox est en plein écran.
- [ ] 30. Surveiller la température CPU/GPU et prévenir si ça chauffe.
- [ ] 31. « Ferme tout sauf Chrome » avant de jouer.
- [ ] 32. Lancer un serveur Minecraft (MCServerManager) à la voix.
- [ ] 33. Captures d'écran à la voix, rangées par jour.
- [ ] 34. Lumières RGB (SignalRGB, Razer) selon l'état de Jarvis.
- [ ] 35. « Combien de temps j'ai joué aujourd'hui ? » via le suivi des applis.
- [ ] 36. Nettoyer les fichiers temporaires à la voix, avec confirmation.

## Téléphone et messages
- [ ] 37. Répondre aux SMS par la voix.
- [ ] 38. Afficher dans l'île qui appelle, avec boutons Décrocher / Refuser.
- [ ] 39. Résumer tous les messages Discord non lus en une phrase.
- [ ] 40. « Ne pas déranger » seulement pour certaines personnes (réglage `people`).
- [ ] 41. Brouillon de réponse aux mails importants, à relire avant envoi.

## Mémoire et organisation
- [ ] 42. Carte visuelle de la mémoire en graphe, avec recherche.
- [ ] 43. Routines déclenchées par un événement (« quand j'ouvre Roblox, lance le chrono de 1 h »).
- [ ] 44. Habitudes avec série de jours et animation de confettis dans l'île.
- [ ] 45. Rappels liés à une appli (« quand j'ouvre Chrome, rappelle-moi… »).
- [ ] 46. Export et sauvegarde automatique des données (`%APPDATA%\Jarvis\donnees`).

## Installation et fiabilité
- [ ] 47. Mise à jour automatique de `JarvisBarre.exe` avec Jarvis, en une fois.
- [ ] 48. Page « Diagnostic » dans la grande île : l'IA (LM Studio ou clé), micro, Pronote, extension Chrome.
- [ ] 49. Installeur avec choix « avec ou sans widgets » et aperçu de la nouvelle icône.
- [ ] 50. Mode « portable » sur clé USB (dossier de données à côté de l'exe).

## Fait en dehors de la liste (2026-11-16)
- [x] Son du « Hey Jarvis » remplacé par un « pou-poum » doux.
- [x] « Appelle X avec WhatsApp » : appel dans l'appli WhatsApp du PC, après confirmation. Sans préciser : appel avec le téléphone.
- [x] « Appelle X sur Discord » : après confirmation (extension Chrome 1.1.0, pas encore essayé en vrai).
- [x] Jarvis et Givre séparés à nouveau : plus de widgets dans Jarvis, ils sont dans Givre.
- [x] IA : modèle `qwen3.5:4b`, température 0,2, consigne « n'invente rien ».

## Étapes déjà prévues à part (avant ou en parallèle)
- Pouvoirs sur le PC (fichiers, applis, web, extinction, PowerShell) : faits, en Rust.
- Portage 100 % Rust : le cerveau est en Rust (`barre/src/cerveau.rs`, 2026-11-16). Restent en Python : les modules (à porter un par un), puis la voix (micro, « Hey Jarvis », Whisper, synthèse), puis suppression du Python.
