"""Habitudes : « j'ai fait mes 20 pompes », « crée l'habitude lire 10 pages », « où j'en suis de mes habitudes ? »."""
import uuid
from datetime import date, timedelta

from core.stockage import Stockage
from . import Resultat, normaliser


def serie(jours: list, aujourdhui: date = None) -> int:
    """Nombre de jours d'affilée (jusqu'à aujourd'hui, ou hier si pas encore fait aujourd'hui)."""
    aujourdhui = aujourdhui or date.today()
    faits = set(jours)
    j = aujourdhui if aujourdhui.isoformat() in faits else aujourdhui - timedelta(days=1)
    n = 0
    while j.isoformat() in faits:
        n += 1
        j -= timedelta(days=1)
    return n


def installer(ctx, reg):
    base = Stockage("habitudes", [])

    def trouver(d, nom):
        n = normaliser(nom)
        return next((h for h in d if normaliser(h["nom"]) == n), None) or \
            next((h for h in d if n in normaliser(h["nom"]) or normaliser(h["nom"]) in n), None)

    def cocher_habitude(nom: str):
        with base as d:
            h = trouver(d, nom)
            if not h:
                h = {"id": uuid.uuid4().hex[:8], "nom": nom.strip().capitalize(), "jours": []}
                d.append(h)
            jour = date.today().isoformat()
            if jour not in h["jours"]:
                h["jours"].append(jour)
            s = serie(h["jours"])
        return Resultat(f"Bien joué pour « {h['nom']} » ! Ça fait {s} jour{'s' if s > 1 else ''} d'affilée.")

    def creer_habitude(nom: str):
        with base as d:
            if not trouver(d, nom):
                d.append({"id": uuid.uuid4().hex[:8], "nom": nom.strip().capitalize(), "jours": []})
        return Resultat(f"Nouvelle habitude : {nom}. Dis-moi quand tu l'as faite.")

    def etat_habitudes():
        with base as d:
            if not d:
                return Resultat("Tu n'as pas encore d'habitude. Dis par exemple : crée l'habitude lire 10 pages.")
            jour = date.today().isoformat()
            faites = [h["nom"] for h in d if jour in h["jours"]]
            restent = [h["nom"] for h in d if jour not in h["jours"]]
        texte = f"Aujourd'hui tu as fait : {', '.join(faites)}. " if faites else ""
        return Resultat(texte + (f"Il te reste : {', '.join(restent)}." if restent else "Tout est fait, bravo !"))

    def contexte():
        with base as d:
            jour = date.today().isoformat()
            return ("HABITUDES : " + " ; ".join(f"{h['nom']} (série {serie(h['jours'])} j, aujourd'hui : "
                                                 f"{'fait' if jour in h['jours'] else 'pas encore'})" for h in d)) if d else ""

    def api(methode, p):
        with base as d:
            if methode == "POST":
                a = p.get("action")
                if a == "ajouter" and p.get("nom") and not trouver(d, p["nom"]):
                    d.append({"id": uuid.uuid4().hex[:8], "nom": p["nom"].strip(), "jours": []})
                for h in d:
                    if h["id"] != p.get("id"):
                        continue
                    jour = p.get("date") or date.today().isoformat()
                    if a == "cocher":
                        h["jours"].remove(jour) if jour in h["jours"] else h["jours"].append(jour)
                    elif a == "supprimer":
                        d.remove(h)
                    break
            jour = date.today().isoformat()
            return {"ok": True, "habitudes": [{**h, "serie": serie(h["jours"]), "aujourdhui": jour in h["jours"]} for h in d]}

    reg.api("habitudes", api)
    reg.contexte(contexte)
    reg.outil("cocher_habitude", "Note qu'une habitude a été faite aujourd'hui (la crée si besoin).",
              {"nom": "l'habitude (ex : pompes, lecture, réviser)"}, ["nom"], cocher_habitude)
    reg.outil("creer_habitude", "Crée une nouvelle habitude à suivre chaque jour.", {"nom": "l'habitude"}, ["nom"], creer_habitude)
    reg.outil("etat_habitudes", "Dit où en sont les habitudes du jour.", {}, [], etat_habitudes)
    reg.regle(r"\b(mes habitudes|ou j'en suis de mes habitudes|ou j en suis de mes habitudes)\b",
              lambda m, t: [("etat_habitudes", {})], priorite=30)
