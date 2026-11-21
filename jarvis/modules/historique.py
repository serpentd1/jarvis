"""Historique des conversations avec Jarvis (ce que tu as dit, ce qu'il a répondu), gardé sur ce PC
(%APPDATA%\\Jarvis\\donnees\\historique.json, les 1000 derniers échanges) et consultable dans la grande île,
section « Historique ». Les codes reçus par SMS ou mail sont masqués."""
from datetime import datetime

from core.stockage import Stockage
from . import Resultat, masquer_codes

MAX = 1000


def installer(ctx, reg):
    base = Stockage("historique", {"echanges": []})

    def noter(question: str, reponse: str):
        if not (question or "").strip():
            return
        with base as d:
            d["echanges"] = (d["echanges"] + [{"date": datetime.now().strftime("%Y-%m-%d %H:%M"),
                                               "question": masquer_codes(question)[:600],
                                               "reponse": masquer_codes(reponse or "")[:1500]}])[-MAX:]

    def api(methode, p):
        if methode == "POST" and p.get("action") == "effacer":
            with base as d:
                d["echanges"] = []
        echanges = list(reversed(base.donnees.get("echanges", [])))       # le plus récent d'abord
        cherche = str(p.get("cherche") or "").lower().strip() if isinstance(p, dict) else ""
        if cherche:
            echanges = [e for e in echanges if cherche in (e["question"] + " " + e["reponse"]).lower()]
        return {"ok": True, "echanges": echanges[:300], "total": len(base.donnees.get("echanges", []))}

    def ouvrir_historique():
        givre = getattr(ctx, "givre", None)
        r = givre.ouvrir("grande", 0, "historique") if givre else None
        if r and r.get("ok"):
            return Resultat("J'ouvre l'historique de nos conversations.")
        return Resultat("Il faut que la barre de Jarvis soit lancée pour ça.", ok=False)

    if not getattr(ctx, "simulation", False):
        reg.apres_echange(noter)
    def repeter():
        """« Répète » : redit la dernière réponse, sans redemander à l'IA."""
        echanges = base.donnees.get("echanges", [])
        derniere = next((e["reponse"] for e in reversed(echanges) if e.get("reponse")), "")
        return Resultat(derniere or "Je n'ai encore rien dit.")

    reg.api("historique", api)
    reg.outil("repeter", "Redit la dernière réponse de Jarvis mot pour mot (« répète », « tu peux répéter ? »).", {}, [], repeter)
    reg.regle(r"^(?:repete|repete ca|tu peux repeter|peux tu repeter|redis(?: le| moi ca| ca)?|qu'est ce que tu as dit|"
              r"t'as dit quoi|tu as dit quoi)(?: jarvis| s'il te plait| stp| svp)?$", lambda m, t: [("repeter", {})], priorite=19)
    reg.outil("ouvrir_historique", "Ouvre l'historique des conversations avec Jarvis dans la grande île.", {}, [],
              ouvrir_historique)
    reg.regle(r"^(?:ouvre|affiche|montre)(?: moi)? (?:l'|l |mon |notre |ton )?historique(?: des| de nos| de mes)?"
              r"(?: conversations?| discussions?| echanges?)?$",
              lambda m, t: [("ouvrir_historique", {})], priorite=25)
