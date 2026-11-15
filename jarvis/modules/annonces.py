"""Annonces à voix haute de ce qui arrive dans la barre Givre : nouvelle note, mail important,
message Discord, fin de minuteur… (réglable dans config.toml, section [annonces])."""
import logging
import time
from datetime import datetime

log = logging.getLogger("annonces")


def dans_creneau(creneau: str, maintenant=None) -> bool:
    """« 22:00-07:00 » -> True si l'heure actuelle est dans ce créneau."""
    try:
        debut, fin = (datetime.strptime(x.strip(), "%H:%M").time() for x in creneau.split("-"))
    except ValueError:
        return False
    h = (maintenant or datetime.now()).time()
    return debut <= h < fin if debut <= fin else h >= debut or h < fin


def phrase(n: dict) -> str:
    source, titre, texte = n.get("source", ""), (n.get("titre") or "").strip(), (n.get("texte") or "").strip()
    texte = texte[:180] + ("…" if len(texte) > 180 else "")
    if source == "pronote":
        return f"Pronote : {titre}. {texte}".strip()
    if source == "mail":
        return f"Mail important de {titre} : {texte}".strip() if texte else f"Nouveau mail important : {titre}."
    if source == "discord":
        return f"Message Discord de {titre} : {texte}"
    return f"{titre}. {texte}".strip(" .") + "."


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
            for n in r.get("notifs", []):
                dernier = max(dernier, n.get("id", 0))
                source = n.get("source", "")
                if (not cfg["actif"] or source in ("jarvis",) or not cfg.get(source, False)
                        or dans_creneau(cfg["ne_pas_deranger"])):
                    continue
                log.info("annonce : %s", n)
                ctx.etat.annoncer(phrase(n))
            dernier = max(dernier, r.get("dernier", dernier))

    if not getattr(ctx, "simulation", False):
        reg.tache(boucle)
