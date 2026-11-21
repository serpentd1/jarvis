# Jarvis cherche sur Internet quand il ne sait pas

Demandé le 2026-11-18 : « quand il ne sait pas me répondre et qu'il a besoin de chercher sur Internet, il le fait ». Rien n'est codé.

## Ce que Jarvis sait déjà faire

- Il cherche sur Internet, lit une page web et lit l'onglet ouvert dans Chrome. C'est dans `modules/internet.py`, fait le 2026-11-17 et essayé en vrai.
- **Mais il ne cherche que si ta question contient certains mots** : « internet », « cherche », « aujourd'hui », « score », « prix », « match », « 2026 »… La liste est dans `besoin_d_internet`, dans `barre/src/cerveau.rs`.
- Si tu demandes « c'est qui le créateur de Minecraft ? » ou « comment on fait un nœud de cravate ? », aucun de ces mots n'y est. Alors Jarvis répond de mémoire, ou dit qu'il ne sait pas, ou se trompe.

## L'idée

Jarvis doit chercher tout seul **dès qu'il n'est pas sûr**, même sans mot spécial dans la question.

1. **Il donne toujours l'outil de recherche à l'IA**, pour qu'elle puisse s'en servir quand elle veut. On lui dit dans sa consigne : « si tu ne connais pas la réponse, ou si elle peut avoir changé, cherche au lieu de deviner ».
2. **Il rattrape les « je ne sais pas »** : si la réponse de l'IA contient « je ne sais pas », « je n'ai pas cette information » ou « je ne suis pas sûr », Jarvis ne la dit pas. Il cherche sur Internet et refait la réponse avec les résultats.
3. **Il prévient en parlant** : « Je regarde sur Internet. » Ça existe déjà avec « Je regarde ça. ».
4. **Il dit d'où ça vient** : « D'après Wikipédia… », et la grande île montre les liens trouvés.
5. **Il n'en fait pas trop** : pas de recherche pour les ordres (« ouvre Chrome », « mets un minuteur »), ni pour ce qu'il sait déjà sur toi (Pronote, tâches, mémoire).
6. **Un réglage** « Chercher sur Internet tout seul : oui / non », dans les paramètres de la barre.

## Pour celui qui codera

- Point 1 : `besoin_d_internet` décide aujourd'hui si les outils de `OUTILS_INTERNET` sont proposés. On peut proposer `web_search` pour toutes les questions qui ne sont pas des actions (`est_action`). Attention : avec plus d'outils, le modèle répond un peu plus lentement. Mesure-le avec `outils/mesurer_modeles.py`.
- Point 2 : le texte arrive phrase par phrase (`sur_phrase`). Il faut repérer le « je ne sais pas » dans la **première** phrase, avant de la dire, couper, lancer `web_search`, puis faire le 2e tour (`second_tour`) comme pour une recherche normale.
- Le rappel système placé juste après la question compte : sans lui, l'IA répond de mémoire (vu le 2026-11-17).
- À essayer en vrai à la voix : une question sans mot spécial (« qui a inventé le Rubik's Cube ? »), une question piège récente, et un ordre simple, pour vérifier qu'il ne cherche pas pour rien.
