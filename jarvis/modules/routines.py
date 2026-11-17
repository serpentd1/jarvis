"""Routines : une phrase (ou une heure) déclenche plusieurs commandes à la suite.
Ex : « bonjour Jarvis » -> l'heure, la météo, les cours du jour. Modifiables dans la grande application."""
import logging
import threading
import uuid
from datetime import datetime

from core.stockage import Stockage
from . import Resultat, normaliser

log = logging.getLogger("routines")
EXEMPLES = [
    {"nom": "Routine du matin", "declencheur": "bonjour", "heure": "", "active": True,
     "etapes": ["Quelle heure est-il ?", "Quel temps fait-il aujourd'hui ?", "Quels cours j'ai aujourd'hui ?",
                "Qu'est-ce que j'ai à faire ?"]},
    {"nom": "Mode devoirs", "declencheur": "mode devoirs", "heure": "", "active": True,
     "etapes": ["Mets la vidéo en pause", "Quels devoirs j'ai pour demain ?", "Lance un minuteur de 25 minutes pour les devoirs"]},
]


def installer(ctx, reg):
    base = Stockage("routines", [{**r, "id": uuid.uuid4().hex[:8]} for r in EXEMPLES])

    def executer(routine) -> str:
        cerveau = getattr(ctx, "cerveau", None)
        if not cerveau:
            return "Je démarre encore."
        reponses = []
        for etape in routine.get("etapes", []):
            if etape.strip():
                reponses.append(cerveau.repondre(etape, "routine").texte)
        return " ".join(reponses) or "Cette routine est vide."

    def lancer_routine(nom: str):
        with base as d:
            n = normaliser(nom)
            r = next((r for r in d if n in (normaliser(r["nom"]), normaliser(r["declencheur"]))), None) or \
                next((r for r in d if n in normaliser(r["nom"]) or normaliser(r["nom"]) in n), None)
            r = dict(r) if r else None
        if not r:
            return Resultat(f"Je ne connais pas la routine « {nom} ».", ok=False)
        return Resultat(executer(r))

    def boucle_horaire():
        deja = set()
        while not ctx.etat.quitter.wait(20):
            maintenant = datetime.now()
            cle_jour = maintenant.strftime("%Y-%m-%d %H:%M")
            with base as d:
                prevues = [dict(r) for r in d if r.get("active") and r.get("heure") == maintenant.strftime("%H:%M")]
            for r in prevues:
                if (r["id"], cle_jour) in deja:
                    continue
                deja.add((r["id"], cle_jour))
                threading.Thread(target=lambda r=r: ctx.etat.annoncer(executer(r)), daemon=True).start()

    def api(methode, p):
        with base as d:
            if methode == "POST":
                a = p.get("action")
                if a == "enregistrer" and p.get("nom"):
                    r = next((r for r in d if r["id"] == p.get("id")), None)
                    if not r:
                        r = {"id": uuid.uuid4().hex[:8]}
                        d.append(r)
                    r.update(nom=p["nom"].strip(), declencheur=str(p.get("declencheur", "")).strip(),
                             etapes=[str(e).strip() for e in p.get("etapes", []) if str(e).strip()],
                             heure=str(p.get("heure", ""))[:5], active=bool(p.get("active", True)))
                elif a == "supprimer":
                    d[:] = [r for r in d if r["id"] != p.get("id")]
                elif a == "lancer":
                    r = next((dict(r) for r in d if r["id"] == p.get("id")), None)
                    if r:
                        threading.Thread(target=lambda: ctx.etat.annoncer(executer(r)), daemon=True).start()
            return {"ok": True, "routines": d}

    reg.api("routines", api)
    if not getattr(ctx, "simulation", False):
        reg.tache(boucle_horaire)
    reg.outil("lancer_routine", "Lance une routine (plusieurs commandes à la suite).", {"nom": "nom de la routine"},
              ["nom"], lancer_routine)

    def regle_declencheur(_m, texte):
        n = normaliser(texte)
        with base as d:
            r = next((r for r in d if r.get("active") and r.get("declencheur") and normaliser(r["declencheur"]) == n), None)
        return [("lancer_routine", {"nom": r["nom"]})] if r else None

    reg.regle(r".", regle_declencheur, priorite=8)
    reg.regle(r"^(?:lance|demarre|active|fais)(?: moi)? (?:la |ma )?routine (?P<nom>.+)$",
              lambda m, t: [("lancer_routine", {"nom": m["nom"]})], priorite=9)
