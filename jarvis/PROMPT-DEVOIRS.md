# Prompt à donner plus tard : le « mode devoirs » de Jarvis

> Rien n'est codé pour l'instant. Quand tu veux le faire, copie tout ce qui est sous la ligne dans une
> nouvelle conversation Claude Code ouverte dans le dossier du projet (celui qui contient `jarvis` et `givre`).

---

Crée un **mode devoirs** dans Jarvis (dossier `jarvis/`), pour un collégien français.

## Contexte du projet (à vérifier dans le code avant de coder)

- Jarvis est un assistant vocal local. Son cerveau est en Rust : `jarvis/barre/src/cerveau.rs` (IA locale
  l'IA via la barre, route `/ia/discuter` ; outils, confirmations). Les outils Rust sont dans `jarvis/barre/src/pouvoirs.rs` (tableau `OUTILS`,
  fonction `appeler`, raccourcis vocaux dans `regles()`).
- Les devoirs, les notes et les cours viennent de Pronote : `jarvis/barre/src/pronote.rs`, exposés par
  `GET http://127.0.0.1:47802/etat`.
- L'île (barre en haut de l'écran) et la grande île sont dessinées dans `jarvis/barre/src/island*.rs`.
- Il existe déjà un module Python de leçons, `jarvis/modules/lecons.py` (révision à l'oral). Lis-le et reprends
  ses bonnes idées, mais **écris tout le nouveau code en Rust** (règle du projet : plus de nouveau Python).
- Règles du projet : lancer `graphify query "<question>"` avant de chercher dans le code, `graphify update .`
  après les modifications, ne rien commiter sans qu'on le demande, expliquer en français simple.
- Pour lancer l'appli hors de Claude : `explorer.exe`, jamais `Start-Process`.

## Ce que je veux

1. **QCM à partir d'un devoir ou d'une leçon.** Je dis « fais-moi un QCM sur mon devoir de maths » ou « un QCM
   sur la leçon que je viens de te dicter ». Jarvis crée 5 à 10 questions, avec 4 choix chacune et une seule
   bonne réponse. Il pose les questions une par une à la voix. Je réponds par « A », « B », « C », « D » ou par
   le texte de la réponse. À la fin, il donne ma note sur 20, les questions ratées, et la bonne réponse
   expliquée en une phrase.
2. **Autres exercices** : vrai ou faux, questions ouvertes courtes, cartes de révision (question au recto,
   réponse au verso), « explique-moi cette leçon comme à un élève de 6e », dictée de mots, tables et calcul
   mental, conjugaison, vocabulaire d'anglais dans les deux sens.
3. **D'où vient le contenu** : le texte du devoir Pronote, une leçon que je dicte ou que je colle, ou un
   fichier (texte, PDF, photo du cahier lue par le modèle de vision s'il est installé).
4. **Jamais d'invention.** Les questions et les corrigés doivent venir du contenu fourni. Si le contenu est
   trop court pour faire un QCM sérieux, Jarvis le dit et demande la leçon. Chaque question garde la phrase
   source dont elle vient, pour qu'on puisse vérifier.
5. **Affichage dans la grande île** : une page « Devoirs » avec la liste des devoirs Pronote par jour, un
   bouton « Réviser » par devoir, le QCM en cours (question, 4 boutons, progression) et l'historique de mes
   notes par matière.
6. **Suivi** : garder les résultats dans `%APPDATA%\Jarvis\donnees\revisions.json` (date, matière, note,
   questions ratées). Reproposer les questions ratées les jours suivants (répétition espacée simple : 1 jour,
   3 jours, 7 jours). « Qu'est-ce que je dois réviser aujourd'hui ? » doit répondre avec ça.
7. **Rappels** : la veille d'un contrôle repéré dans Pronote, Jarvis propose une révision de 10 minutes.
8. **Sans tricher** : Jarvis ne donne pas la réponse d'un exercice noté à rendre. Il explique la méthode,
   donne un exemple différent, puis vérifie ma réponse.

## Comment le faire

- Commence par me poser tes questions (une liste courte), puis propose un plan en étapes avant de coder.
- Fais une étape à la fois, avec des tests `cargo test` : création d'un QCM à partir d'une fausse réponse
  d'IA, correction des réponses « A/B/C/D » et en toutes lettres, calcul de la note, répétition espacée.
- Demande le QCM à l'IA en JSON strict (`json: true` sur la route `/ia/discuter` de la barre), puis vérifie-le : 4 choix différents, une bonne
  réponse qui existe, pas de question en double. Un QCM invalide est redemandé une fois ; sinon Jarvis dit
  qu'il n'y arrive pas.
- À la fin de chaque étape, dis honnêtement ce qui est testé et ce qui ne l'est pas, et mets à jour
  `jarvis/LISEZMOI.md` et `jarvis/FEUILLE-DE-ROUTE.md`.

Terminé = je peux dire « Hey Jarvis, fais-moi un QCM sur mon devoir de maths », répondre à la voix, entendre
ma note, et retrouver le QCM et mes résultats dans la grande île.
