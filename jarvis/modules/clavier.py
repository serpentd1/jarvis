"""Clavier et défilement : « écris bonjour », « appuie sur entrée », « fais contrôle C », « descends »."""
import re

from core import controle
from . import Resultat, nombre, normaliser


def installer(ctx, reg):
    def taper_texte(texte: str):
        controle.taper(texte)
        return Resultat("C'est écrit.")

    def appuyer_touches(touches: str):
        noms = [normaliser(t) for t in re.split(r"\s*\+\s*|\s+plus\s+|\s+et\s+|\s+", touches.strip()) if t.strip()]
        noms = [{"controle": "ctrl", "control": "ctrl", "majuscule": "maj"}.get(n, n) for n in noms]
        if not controle.raccourci(noms):
            return Resultat(f"Je ne connais pas la touche « {touches} ».", ok=False)
        return Resultat("C'est fait.")

    def defiler(direction: str = "bas", quantite: int = 5):
        crans = max(1, min(30, int(quantite or 5)))
        controle.molette(crans if normaliser(direction).startswith(("haut", "monte")) else -crans)
        return Resultat("Voilà.")

    reg.outil("taper_texte", "Tape du texte au clavier dans la fenêtre ouverte (là où est le curseur).",
              {"texte": "le texte exact à taper"}, ["texte"], taper_texte)
    reg.outil("appuyer_touches", "Appuie sur une touche ou un raccourci clavier (entrée, échap, ctrl+c, alt+tab…).",
              {"touches": "touches séparées par +, ex : ctrl+c"}, ["touches"], appuyer_touches)
    reg.outil("defiler", "Fait défiler la page ou la fenêtre (molette de la souris).",
              {"direction": {"type": "string", "enum": ["haut", "bas"]},
               "quantite": {"type": "integer", "description": "nombre de crans (5 par défaut)"}}, [], defiler)

    def regle_taper(_m, texte):
        m = re.match(r"^\W*(?:[ée]cris|tape|saisis)(?:[- ]moi)?\s+(?P<t>.+)$", texte, re.I)
        return [("taper_texte", {"texte": m["t"].strip()})] if m else None

    # « écris bonjour » (mais pas « écris à Tom que… », qui est pour Discord)
    reg.regle(r"^(?:ecris|tape|saisis)(?! a )(?! lui)(?! leur)(?: moi)? \S", regle_taper, priorite=20)
    reg.regle(r"^(?:appuie|appuyer|presse|tape) (?:sur )?(?:la touche )?(?P<k>(?:ctrl|control|controle|alt|maj|shift|win|"
              r"entree|entrer|echap|tab|tabulation|espace|retour|effacer|suppr|supprimer|haut|bas|gauche|droite|"
              r"f\d{1,2}|[a-z0-9])(?: (?:plus |\+ ?)?\S+)*)$",
              lambda m, t: [("appuyer_touches", {"touches": m["k"]})], priorite=18)
    reg.regle(r"^(?:fais|faire) (?P<k>(?:ctrl|control|controle|alt|maj|shift|win)(?: (?:plus |\+ ?)?\S+)+)$",
              lambda m, t: [("appuyer_touches", {"touches": m["k"]})], priorite=18)

    def regle_defiler(m, _t):
        haut = bool(re.match(r"^(?:remonte|monte|va en haut|scroll(?:e)? (?:vers le )?haut)", m.group(0)))
        n = nombre(m["n"]) if m["n"] else None
        return [("defiler", {"direction": "haut" if haut else "bas", "quantite": int(n) if n else 5})]

    reg.regle(r"^(?:descends|descend|va en bas|remonte|va en haut|monte la page|fais defiler|defile|scrolle?)"
              r"(?: (?:la page|un peu|vers le (?:bas|haut)|en (?:bas|haut)))?(?: de (?P<n>\S+)(?: crans?)?)?$",
              regle_defiler, priorite=18)
