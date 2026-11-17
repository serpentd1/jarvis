"""Annonces à voix haute de ce qui arrive dans la barre Givre : nouvelle note, mail important,
message Discord, SMS ou appel sur le téléphone, fin de minuteur… (réglable dans config.toml, section [annonces]).

Les codes de vérification (suites de chiffres) ne sont jamais lus à voix haute, et le contenu des messages
n'est pas écrit dans le journal.
"""
import logging
import re
import time
from datetime import datetime

from . import masquer_codes

log = logging.getLogger("annonces")
MAX_PAR_TOUR = 3          # au-delà, on résume (« Et 4 autres notifications. ») au lieu de tout lire


def dans_creneau(creneau: str, maintenant=None) -> bool:
    """« 22:00-07:00 » -> True si l'heure actuelle est dans ce créneau."""
    try:
        debut, fin = (datetime.strptime(x.strip(), "%H:%M").time() for x in creneau.split("-"))
    except ValueError:
        return False
    h = (maintenant or datetime.now()).time()
    return debut <= h < fin if debut <= fin else h >= debut or h < fin


def _court(texte: str, n: int = 180) -> str:
    return texte[:n] + ("…" if len(texte) > n else "")


def phrase_telephone(genre: str, titre: str, texte: str) -> str:
    """Notification de « Mobile connecté » (Givre la classe : sms, appel, appli, batterie)."""
    if genre == "appel":
        return f"Appel manqué de {titre}." if "manqu" in texte.lower() else f"{titre} t'appelle sur ton téléphone."
    if genre == "batterie":
        pourcent = re.search(r"\d+\s*%", texte)
        return (f"Ton téléphone n'a plus que {pourcent.group()} de batterie." if pourcent
                else "Ton téléphone n'a presque plus de batterie.")
    if genre == "appli":
        appli, _, de = titre.partition(" — ")       # Givre l'écrit « WhatsApp — Tom »
        if de:
            return f"{appli} sur ton téléphone, de {masquer_codes(de)} : {texte}"
        return f"Notification sur ton téléphone : {masquer_codes(appli)}. {texte}".strip()
    return f"SMS de {titre} : {texte}"


def phrase(n: dict) -> str:
    source, titre = n.get("source", ""), (n.get("titre") or "").strip()
    texte = _court(masquer_codes((n.get("texte") or "").strip()))
    if source == "pronote":
        return f"Pronote : {titre}. {texte}".strip()
    if source == "mail":
        titre = masquer_codes(titre)
        return f"Mail important de {titre} : {texte}".strip() if texte else f"Nouveau mail important : {titre}."
    if source == "discord":
        return f"Message Discord de {titre} : {texte}"
    if source == "telephone":
        return phrase_telephone(n.get("type", ""), titre, texte)
    return f"{titre}. {texte}".strip(" .") + "."


def a_annoncer(cfg, n: dict) -> bool:
    source = n.get("source", "")
    if source == "jarvis" or not cfg.get(source, False):
        return False
    if source == "telephone" and n.get("type") == "appli":      # Instagram, jeux… : seulement si demandé
        return bool(cfg.get("telephone_applis", False))
    return True


def installer(ctx, reg):
    cfg = ctx.cfg["annonces"]

    def boucle():
        dernier = None
        while not ctx.etat.quitter.is_set():
            time.sleep(5)
            r = ctx.givre.notifs(dernier or 0)
            if not r:
                continue
            if dernier is None:            # au démarrage : on n'annonce pas ce qui est déjà arrivé
                dernier = r.get("dernier", 0)
                continue
            nouvelles = r.get("notifs", [])
            for n in nouvelles:
                dernier = max(dernier, n.get("id", 0))
            dernier = max(dernier, r.get("dernier", dernier))
            if not cfg["actif"] or dans_creneau(cfg["ne_pas_deranger"]):
                continue
            a_dire = [n for n in nouvelles if a_annoncer(cfg, n)]
            for n in a_dire[:MAX_PAR_TOUR]:
                log.info("annonce : %s %s", n.get("source", ""), n.get("type", ""))   # sans le contenu
                ctx.etat.annoncer(phrase(n))
            reste = len(a_dire) - MAX_PAR_TOUR
            if reste > 0:
                ctx.etat.annoncer(f"Et {reste} autre{'s' if reste > 1 else ''} notification{'s' if reste > 1 else ''}.")

    if not getattr(ctx, "simulation", False):
        reg.tache(boucle)
