"""Changer la voix de Jarvis : « change de voix », « prends une voix d'homme », « voix Pierre »."""
import threading

from core.tts import CATALOGUE, est_telechargee
from . import Resultat, normaliser

HOMMES = ["tom", "pierre", "jarvis"]
FEMMES = ["siwis_hd", "jessica", "siwis"]


def choisir(demande: str, actuelle: str) -> str | None:
    """« homme » -> la voix d'homme suivante, « anglais » -> jarvis, « » -> la voix suivante…"""
    d = normaliser(demande or "").replace("_", " ")
    ordre = list(CATALOGUE)
    suivante = lambda liste: liste[(liste.index(actuelle) + 1) % len(liste)] if actuelle in liste else liste[0]
    if not d or d in ("suivante", "autre", "une autre", "nouvelle"):
        return suivante(ordre)
    if "anglais" in d or "british" in d or "iron man" in d:
        return "jarvis"
    if "hd" in d or "naturel" in d or "meilleure" in d:
        return "siwis_hd"
    if "homme" in d or "masculin" in d or "garcon" in d:
        return suivante(HOMMES)
    if "femme" in d or "feminin" in d or "fille" in d:
        return suivante(FEMMES)
    return next((nom for nom in ordre if nom.replace("_", " ") in d or d in nom.replace("_", " ")), None)


def installer(ctx, reg):
    def changer_voix(nom: str = ""):
        changer = getattr(ctx, "changer_voix", None)
        actuelle = ctx.voix_actuelle() if hasattr(ctx, "voix_actuelle") else None
        if not changer:
            return Resultat("Je ne peux pas changer de voix en ce moment.", ok=False)
        cible = choisir(nom, actuelle)
        if not cible:
            noms = ", ".join(v["titre"].split(" (")[0] for v in CATALOGUE.values())
            return Resultat(f"Je ne connais pas cette voix. J'ai : {noms}.", ok=False)
        if cible == actuelle:
            return Resultat("C'est déjà ma voix.")
        if not est_telechargee(cible):
            def plus_tard():
                try:
                    changer(cible)
                    ctx.etat.annoncer("Voici ma nouvelle voix. Elle te plaît ?")
                except Exception:  # noqa: BLE001
                    ctx.etat.annoncer("Je n'ai pas réussi à télécharger cette voix.")
            threading.Thread(target=plus_tard, daemon=True).start()
            return Resultat("Je télécharge cette voix, une petite minute.")
        changer(cible)
        return Resultat(f"Voici ma nouvelle voix : {CATALOGUE[cible]['titre'].split(' (')[0]}. Elle te plaît ?")

    reg.outil("changer_voix", "Change la voix de Jarvis.",
              {"nom": {"type": "string", "description": "voix voulue : " + ", ".join(CATALOGUE)
                       + ", ou « homme », « femme », « anglais » ; vide = la voix suivante"}},
              [], changer_voix)
    reg.regle(r"^(?:change|changer|change moi|prends?|mets?)(?: de| une| ta| la)? voix(?P<suite>.*)$",
              lambda m, t: [("changer_voix", {"nom": m["suite"].strip()})], priorite=15)
    reg.regle(r"^(?:une |la )?voix (?:d |de )?(?P<nom>homme|femme|anglais|anglaise|" + "|".join(
        n.replace("_", " ") for n in CATALOGUE) + r")$",
              lambda m, t: [("changer_voix", {"nom": m["nom"]})], priorite=15)
