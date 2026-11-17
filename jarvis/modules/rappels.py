"""Rappels : « rappelle-moi dans 20 minutes de sortir le chien », « rappelle-moi demain à 18 h d'appeler mamie »."""
import re
import time
import uuid
from datetime import datetime, timedelta

from core.stockage import Stockage
from . import Resultat, duree_en_secondes

REPETITIONS = {"": None, "jour": timedelta(days=1), "semaine": timedelta(weeks=1)}


def lire_date(texte: str):
    """« 2026-11-16 18:00 » ou « 16/11 18h » -> datetime (ou None)."""
    texte = (texte or "").strip()
    for f in ("%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S"):
        try:
            return datetime.strptime(texte[:19] if "S" in f else texte[:16], f)
        except ValueError:
            pass
    return None


def dire_quand(d: datetime) -> str:
    maintenant = datetime.now()
    jour = ("aujourd'hui" if d.date() == maintenant.date() else "demain" if d.date() == (maintenant + timedelta(days=1)).date()
            else f"le {d.day}/{d.month}")
    return f"{jour} à {d.hour}h{d.minute:02d}"


def installer(ctx, reg):
    base = Stockage("rappels", [])

    def ajouter(texte: str, quand: datetime, repetition: str = ""):
        with base as d:
            d.append({"id": uuid.uuid4().hex[:8], "texte": texte.strip().rstrip("."), "quand": quand.strftime("%Y-%m-%d %H:%M"),
                      "repetition": repetition if repetition in REPETITIONS else "", "fait": False})

    def ajouter_rappel(texte: str, date_heure: str = "", dans_minutes: float = 0, repetition: str = ""):
        quand = lire_date(date_heure) if date_heure else None
        if not quand and dans_minutes:
            quand = datetime.now() + timedelta(minutes=float(dans_minutes))
        if not quand:
            return Resultat("Pour quand ? Dis-moi l'heure ou dans combien de temps.", ok=False)
        if quand < datetime.now() - timedelta(minutes=1):
            return Resultat("Cette heure est déjà passée.", ok=False)
        ajouter(texte, quand, repetition)
        return Resultat(f"Je te le rappellerai {dire_quand(quand)} : {texte}.")

    def boucle():
        while not ctx.etat.quitter.wait(15):
            maintenant = datetime.now()
            with base as d:
                for r in d:
                    q = lire_date(r["quand"])
                    if r["fait"] or not q or q > maintenant:
                        continue
                    ctx.etat.annoncer(f"Rappel : {r['texte']}")
                    ctx.givre.notif("Rappel", r["texte"], "jarvis")
                    pas = REPETITIONS.get(r.get("repetition", ""))
                    if pas:
                        while q <= maintenant:
                            q += pas
                        r["quand"] = q.strftime("%Y-%m-%d %H:%M")
                    else:
                        r["fait"] = True

    def contexte():
        with base as d:
            futurs = sorted((r for r in d if not r["fait"]), key=lambda r: r["quand"])[:6]
        return ("RAPPELS PRÉVUS : " + " ; ".join(f"{r['quand']} {r['texte']}" for r in futurs) + ".") if futurs else ""

    def api(methode, p):
        with base as d:
            if methode == "POST":
                a = p.get("action")
                if a == "ajouter" and p.get("texte") and lire_date(p.get("quand", "")):
                    d.append({"id": uuid.uuid4().hex[:8], "texte": p["texte"].strip(), "quand": p["quand"][:16].replace("T", " "),
                              "repetition": p.get("repetition", "") if p.get("repetition", "") in REPETITIONS else "",
                              "fait": False})
                elif a == "supprimer":
                    d[:] = [r for r in d if r["id"] != p.get("id")]
                elif a == "nettoyer":
                    d[:] = [r for r in d if not r["fait"]]
            return {"ok": True, "rappels": sorted(d, key=lambda r: (r["fait"], r["quand"]))}

    reg.api("rappels", api)
    reg.contexte(contexte)
    if not getattr(ctx, "simulation", False):
        reg.tache(boucle)
    reg.outil("ajouter_rappel", "Crée un rappel : Jarvis le dira à voix haute au bon moment.",
              {"texte": "ce qu'il faut rappeler",
               "date_heure": "date et heure au format AAAA-MM-JJ HH:MM (utilise la date du jour donnée plus haut)",
               "dans_minutes": {"type": "number", "description": "ou bien : dans combien de minutes"},
               "repetition": {"type": "string", "enum": ["", "jour", "semaine"]}},
              ["texte"], ajouter_rappel)

    def regle_dans(m, texte):
        secondes = duree_en_secondes(m["d"])
        quoi = re.split(r"\b(?:de|d'|qu'|que)\s*", texte.split(m["d"].split()[-1], 1)[-1], maxsplit=1)
        if not secondes or len(quoi) < 2 or not quoi[1].strip():
            return None
        return [("ajouter_rappel", {"texte": quoi[1].strip(" .!?"), "dans_minutes": secondes / 60})]

    reg.regle(r"^rappelle(?: moi)? dans (?P<d>.+?(?:minutes?|heures?|secondes?|h|min)) (?:de |d'|qu|que )",
              regle_dans, priorite=19)
