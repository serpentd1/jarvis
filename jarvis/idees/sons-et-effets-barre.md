# Sons et effets visuels de la barre de Jarvis

Demandé le 2026-11-18 : un petit bruit à chaque bouton, « un bon bruit », pas un bruit qui agace au bout de 2 ou 3 clics. **Fait le 2026-11-21 : les idées 1, 2, 3, 4, 5, 8, 10, 11, 14 et 15 (+ la 12 en bonus).** Réglages : grande application › Paramètres (sons, volume, onde et lueur).

## Choix déjà faits

- **Appli** : la barre de Jarvis (l'île et la grande île), pas Givre.
- **Style de son** : doux, comme du verre. Des petits « tic » feutrés et cristallins, très courts et pas forts.
- **Effets visuels** : une onde de lumière quand on clique, et une lueur quand la souris passe sur un bouton.
- Pas choisis pour l'instant : le rebond à l'ouverture et les paillettes.

## Pour celui qui codera

- `barre/src/sys.rs` a déjà `wav()`, qui fabrique un son en mémoire, et `soft_sound(kind)`, qui le joue avec PlaySoundW (0 = ouverture de l'île, 1 = notification).
- PlaySoundW coupe le son d'avant. Ça va bien pour des clics courts.
- Pour que ça ne lasse pas, il faut changer un tout petit peu la hauteur de chaque clic (idée 1).
- Il faut aussi un réglage pour baisser ou couper les sons (idée 11).

## Les 50 idées

### Sons
1. Un « tic » de verre à chaque bouton, un tout petit peu plus aigu ou plus grave à chaque fois : jamais exactement le même, donc pas lassant. **(fait)**
2. Des sons différents selon le bouton : deux notes qui montent pour allumer, qui descendent pour éteindre, une note qui descend pour fermer. **(fait)**
3. Un petit « fffft » feutré quand on change d'onglet. **(fait)**
4. Un son de tiroir doux quand la grande île se déplie, et l'inverse quand elle se replie. **(fait)**
5. Un « ding » cristallin quand on coche une tâche, et un petit accord de 3 notes quand toutes les tâches du jour sont faites. **(fait)**
6. Un son pour « Jarvis t'écoute » et un autre pour « Jarvis a fini d'écouter ».
7. Un tic très léger quand on fait défiler une liste, comme une molette crantée.
8. Un son d'erreur gentil (deux notes graves) au lieu du « dong » de Windows. **(fait)**
9. Un son différent pour chaque notification : mail, Pronote, Discord, minuteur.
10. Une fin de minuteur avec un carillon qui commence bas et monte doucement. **(fait)**
11. Un curseur de volume des sons, et un bouton « couper tous les sons ». **(fait)**
12. Des sons plus bas tout seuls après 21 h. **(fait)**
13. Des packs de sons au choix : Verre, Bois, Futuriste, Rétro 8-bit.

### Effets visuels
14. Une onde de lumière là où on clique *(choisie)*. **(fait)**
15. Une lueur quand la souris passe sur un bouton *(choisie)*. **(fait)**
16. Le bouton qui s'enfonce un peu quand on appuie.
17. Les pages qui glissent sur le côté quand on change d'onglet, au lieu d'apparaître d'un coup.
18. Un reflet de lumière qui traverse le verre de l'île de temps en temps, très discret.
19. Le bord de l'île qui « respire » doucement pendant que Jarvis écoute.
20. Une onde qui bouge avec la voix pendant qu'on parle.
21. Des chiffres qui roulent comme un compteur quand ils changent (heure, notes, nombre de mails).
22. Des barres de progression avec un reflet qui avance (minuteur, impression 3D).
23. Des notifications qui tombent avec un petit rebond et repartent en fondu.
24. Des cartes qui apparaissent une par une, en cascade, quand on ouvre une page.
25. Une île qui prend la couleur de la pochette de la musique en cours.
26. Du givre dans les coins de l'île quand il fait froid dehors, des gouttes quand il pleut.
27. Une ombre douce sous l'île, qui grandit quand elle se déplie.

### L'île
28. Un mode pastille : l'île devient une petite pastille et ressort quand on passe la souris.
29. La molette sur l'île : vers le bas pour la déplier, vers le haut pour la replier.
30. Clic du milieu sur l'île pour mettre la musique en pause ou la relancer.
31. 4 boutons raccourcis qu'on choisit soi-même (Roblox, Pronote, Discord…).
32. Une barre de recherche dans la grande île : une tâche, un souvenir, un mail.
33. Une page « Aujourd'hui » avec tout au même endroit : cours, devoirs, météo, mails, minuteurs.
34. Un fond de grande île au choix : le fond d'écran flouté, un dégradé, une image.
35. Une taille de texte réglable.
36. Un petit tutoriel la première fois, avec des bulles qui montrent les boutons.
37. Un bandeau « Annuler » pendant quelques secondes après avoir supprimé quelque chose.

### École
38. L'emploi du temps du jour en frise, avec un trait « maintenant » qui avance.
39. Une nouvelle note Pronote qui arrive sous une carte qui se retourne (petit suspense).
40. Une couleur par matière partout (cours, devoirs, notes).
41. Un compte à rebours jusqu'à la fin du cours et jusqu'à la fin de la journée.
42. La liste du sac pour demain, faite à partir de l'emploi du temps.
43. Les stats de la semaine : tâches faites, temps de travail, petit graphique.
44. Une météo animée : soleil qui brille, nuages qui passent, éclairs.

### Fun
45. Des succès à débloquer (« 7 jours de suite tous les devoirs faits ») avec un petit son de victoire.
46. Un secret : cliquer 10 fois sur l'île la fait tourner ou sort une blague.
47. Une petite mascotte flocon : elle dort la nuit et sourit quand les tâches sont finies.
48. Des thèmes de saison tout seuls : neige à Noël, orange à Halloween, couleurs d'été.
49. Un fond animé très lent dans la grande île, comme une aurore boréale.
50. Un petit jingle de quelques notes douces quand le PC démarre, comme une console.
