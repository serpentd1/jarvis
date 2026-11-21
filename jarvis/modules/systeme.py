"""Petites commandes du PC : heure, date, verrouiller, couper le micro, éteindre Jarvis."""
import ctypes
import threading
from datetime import datetime

from . import Resultat

JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
MOIS = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
        "novembre", "décembre"]


def installer(ctx, reg):
    def dire_heure():
        m = datetime.now()
        return Resultat(f"Il est {m.hour} heure{'s' if m.hour > 1 else ''}{f' {m.minute:02d}' if m.minute else ''}.")

    def dire_date():
        m = datetime.now()
        return Resultat(f"Nous sommes le {JOURS[m.weekday()]} {m.day if m.day > 1 else '1er'} {MOIS[m.month - 1]} {m.year}.")

    def verrouiller_pc():
        ctypes.windll.user32.LockWorkStation()
        return Resultat("Je verrouille l'ordinateur.")

    def couper_micro():
        ctx.etat.micro = False
        return Resultat("Je coupe mon micro. Rallume-le depuis l'île ou l'icône près de l'horloge.")

    def eteindre_jarvis():
        def oui():
            ctx.etat.quitter.set()
            return Resultat("À plus tard !")
        return Resultat("Tu veux vraiment que je m'éteigne ?", confirmer=oui, annuler="D'accord, je reste là.")

    reg.outil("dire_heure", "Donne l'heure qu'il est.", {}, [], dire_heure)
    reg.outil("dire_date", "Donne la date du jour.", {}, [], dire_date)
    reg.outil("verrouiller_pc", "Verrouille l'ordinateur (écran de connexion).", {}, [], verrouiller_pc)
    reg.outil("couper_micro", "Coupe le micro de Jarvis (il n'écoute plus « Hey Jarvis »).", {}, [], couper_micro)
    reg.outil("eteindre_jarvis", "Éteint complètement Jarvis.", {}, [], eteindre_jarvis)

    reg.regle(r"^(?:quelle heure (?:est il|il est)|il est quelle heure|l'heure|donne moi l'heure|t'as l'heure)$",
              lambda m, t: [("dire_heure", {})], priorite=5)
    reg.regle(r"^(?:on est (?:le )?quel jour|quel jour (?:on est|sommes nous|est on)|c'est quoi la date|quelle est la date|la date)$",
              lambda m, t: [("dire_date", {})], priorite=5)
    reg.regle(r"^(?:verrouille|bloque)(?: l'ordinateur| le pc| l'ordi| mon pc| la session)$",
              lambda m, t: [("verrouiller_pc", {})], priorite=5)
    reg.regle(r"^(?:coupe|desactive|eteins) (?:ton|le) micro$", lambda m, t: [("couper_micro", {})], priorite=5)
    reg.regle(r"^(?:(?:non|bon|ok|alors|allez) )?(?:eteins toi|desactive toi|arrete toi|quitte|va te coucher|"
              r"au revoir jarvis)(?: jarvis)?$", lambda m, t: [("eteindre_jarvis", {})], priorite=5)

    def pause_ecoute(minutes: int = 10):
        """« Mets-toi en pause » : il n'écoute plus « Hey Jarvis » pendant quelques minutes, puis revient tout seul."""
        try:
            n = max(1, min(240, int(minutes)))
        except (TypeError, ValueError):
            n = 10
        ctx.etat.micro = False

        def reprendre():
            ctx.etat.micro = True
        t = threading.Timer(n * 60, reprendre)
        t.daemon = True
        t.start()
        return Resultat(f"Je me mets en pause {n} minute{'s' if n > 1 else ''}. Je réécoute ensuite tout seul.")

    reg.outil("pause_ecoute", "Met Jarvis en pause : il n'écoute plus « Hey Jarvis » pendant N minutes (10 par défaut).",
              {"minutes": {"type": "integer", "description": "durée de la pause en minutes"}}, [], pause_ecoute)
    reg.regle(r"^(?:(?:non|bon|ok|alors) )?(?:mets toi en pause|fais une pause|arrete d'ecouter|arrete d ecouter|"
              r"n'ecoute plus|n ecoute plus|ecoute plus)(?: jarvis)?(?: (?:pendant|pour) (?P<n>\d+) minutes?)?$",
              lambda m, t: [("pause_ecoute", {"minutes": int(m["n"]) if m["n"] else 10})], priorite=5)
