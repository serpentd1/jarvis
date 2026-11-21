"""Mode concentration : « lance le mode concentration (pendant 45 minutes) ».

Ferme les distractions (onglets YouTube, Discord… et l'appli Discord), active Ne pas déranger dans l'île (plus de
bulles ni de sons) et coupe les annonces de Jarvis, puis lance un minuteur « concentration ». À la fin (ou
« arrête le mode concentration »), tout redevient normal.
"""
import logging
import threading
import time

from . import Resultat, duree_en_secondes
from .minuteurs import dire_duree

log = logging.getLogger("concentration")


def installer(ctx, reg):
    cfg = ctx.cfg.get("concentration", {}) if isinstance(getattr(ctx, "cfg", None), dict) else {}
    etat = {"fin": 0.0, "minuterie": None}
    ctx.concentration_jusqua = 0.0          # lu par annonces.py : pas d'annonce pendant la concentration

    def finir(annoncer: bool):
        t = etat["minuterie"]
        if t:
            t.cancel()
        etat.update(fin=0.0, minuterie=None)
        ctx.concentration_jusqua = 0.0
        givre = getattr(ctx, "givre", None)
        if givre and hasattr(givre, "ne_pas_deranger"):
            givre.ne_pas_deranger(False)
        if annoncer and getattr(ctx, "etat", None):
            ctx.etat.annoncer("Fin du mode concentration, bravo ! Tu peux faire une pause.")

    def mode_concentration(action: str = "demarrer", minutes: float = 0):
        if action == "arreter":
            if not etat["fin"]:
                return Resultat("Le mode concentration n'est pas lancé.")
            reste = max(0, etat["fin"] - time.time())
            finir(False)
            reg.executer("annuler_minuteur", {"nom": "concentration"})
            return Resultat(f"Mode concentration arrêté, il restait {dire_duree(int(reste))}.")
        if action == "etat":
            if not etat["fin"]:
                return Resultat("Le mode concentration n'est pas lancé.")
            return Resultat(f"Encore {dire_duree(int(etat['fin'] - time.time()))} de concentration.")

        minutes = float(minutes or cfg.get("minutes", 25))
        minutes = max(1.0, min(minutes, 240.0))
        secondes = int(minutes * 60)
        fermes = []
        for site in cfg.get("fermer_sites", ["youtube", "discord"]):
            r = reg.executer("fermer_onglet", {"cherche": site})
            if r.ok and "fermé" in r.dire:
                fermes.append(site.capitalize())
        for appli in cfg.get("fermer_applis", ["discord"]):
            r = reg.executer("fermer_application", {"nom": appli})
            if r.ok and r.confirmer is None and appli.capitalize() not in fermes:
                fermes.append(appli.capitalize())
        givre = getattr(ctx, "givre", None)
        dnd = bool(givre and hasattr(givre, "ne_pas_deranger") and givre.ne_pas_deranger(True))
        reg.executer("minuteur", {"duree_secondes": secondes, "nom": "concentration"})
        t = etat["minuterie"]
        if t:
            t.cancel()
        t = threading.Timer(secondes, finir, args=(True,))
        t.daemon = True
        t.start()
        etat.update(fin=time.time() + secondes, minuterie=t)
        ctx.concentration_jusqua = etat["fin"]
        log.info("concentration : %d min, fermés %s, ne pas déranger %s", minutes, fermes, dnd)
        phrase = f"Mode concentration pour {dire_duree(secondes)}."
        if fermes:
            phrase += " J'ai fermé " + ", ".join(fermes) + "."
        phrase += (" Ne pas déranger est activé." if dnd else " Je ne dirai plus rien jusqu'à la fin.")
        return Resultat(phrase + " Bon courage !")

    reg.outil("mode_concentration", "Mode concentration pour travailler : ferme YouTube, Discord et les autres "
              "distractions, active Ne pas déranger et lance un minuteur. Aussi pour l'arrêter ou savoir combien de "
              "temps il reste.",
              {"action": {"type": "string", "enum": ["demarrer", "arreter", "etat"]},
               "minutes": {"type": "number", "description": "durée en minutes (25 par défaut)"}},
              [], mode_concentration)

    def regle_demarrer(m, texte):
        s = duree_en_secondes(texte) if m["d"] else None
        return [("mode_concentration", {"action": "demarrer", **({"minutes": s / 60} if s else {})})]

    mode = r"(?:le |un )?mode (?:concentration|concentre|focus)"
    reg.regle(rf"^(?:lance|active|demarre|mets?|met|passe en|on passe en)(?: moi)? {mode}(?P<d> (?:de|pour|pendant) .+)?$"
              rf"|^(?:{mode}|je (?:dois|veux) (?:me concentrer|travailler))(?P<d2> (?:de|pour|pendant) .+)?$",
              lambda m, t: regle_demarrer({"d": m["d"] or m["d2"]}, t), priorite=14)
    reg.regle(rf"^(?:arrete|desactive|stop|stoppe|termine|quitte|fin du|enleve)(?: le)? {mode}$",
              lambda m, t: [("mode_concentration", {"action": "arreter"})], priorite=14)
