"""Tâches : des listes de choses à faire. « Ajoute payer les factures à mes tâches », « qu'est-ce que j'ai à faire ? »."""
import time
import uuid

from core.stockage import Stockage
from . import Resultat, normaliser


def installer(ctx, reg):
    base = Stockage("taches", {"listes": [{"nom": "À faire", "taches": []}]})

    def trouver_liste(d, nom: str, creer: bool = False):
        n = normaliser(nom or "")
        for liste in d["listes"]:
            if not n or normaliser(liste["nom"]) == n:
                return liste
        if creer and nom:
            d["listes"].append({"nom": nom.strip().capitalize(), "taches": []})
            return d["listes"][-1]
        return d["listes"][0]

    def ajouter(texte: str, liste: str = ""):
        with base as d:
            l = trouver_liste(d, liste, creer=True)
            l["taches"].append({"id": uuid.uuid4().hex[:8], "texte": texte.strip().rstrip("."), "fait": False,
                                "cree": time.time()})
            return l["nom"]

    def toutes(d):
        return [(l, t) for l in d["listes"] for t in l["taches"]]

    def ajouter_tache(texte: str, liste: str = ""):
        nom = ajouter(texte, liste)
        return Resultat(f"C'est noté dans « {nom} » : {texte}.")

    def terminer_tache(texte: str):
        with base as d:
            cible = next((t for _, t in toutes(d) if not t["fait"] and normaliser(texte) in normaliser(t["texte"])), None)
            if not cible:
                return Resultat(f"Je ne trouve pas la tâche « {texte} ».", ok=False)
            cible["fait"] = True
            return Resultat(f"Bravo, « {cible['texte']} » est terminé !")

    def lire_taches(liste: str = ""):
        with base as d:
            restantes = [t["texte"] for l in d["listes"] if not liste or normaliser(liste) in normaliser(l["nom"])
                         for t in l["taches"] if not t["fait"]]
        if not restantes:
            return Resultat("Tu n'as rien à faire, profite !")
        return Resultat(f"Tu as {len(restantes)} chose{'s' if len(restantes) > 1 else ''} à faire : "
                        + ", ".join(restantes[:8]) + ".")

    def contexte():
        with base as d:
            restantes = [f"{t['texte']} ({l['nom']})" for l, t in toutes(d) if not t["fait"]]
        return ("TÂCHES À FAIRE : " + " ; ".join(restantes[:10]) + ".") if restantes else ""

    def api(methode, p):
        with base as d:
            if methode == "POST":
                a = p.get("action")
                if a == "ajouter" and p.get("texte"):
                    trouver_liste(d, p.get("liste", ""), creer=True)["taches"].append(
                        {"id": uuid.uuid4().hex[:8], "texte": p["texte"].strip(), "fait": False, "cree": time.time()})
                elif a in ("cocher", "supprimer", "renommer"):
                    for l, t in toutes(d):
                        if t["id"] == p.get("id"):
                            if a == "cocher":
                                t["fait"] = not t["fait"]
                            elif a == "renommer":
                                t["texte"] = str(p.get("texte", t["texte"]))
                            else:
                                l["taches"].remove(t)
                            break
                elif a == "nouvelle_liste" and p.get("nom"):
                    trouver_liste(d, p["nom"], creer=True)
                elif a == "supprimer_liste" and len(d["listes"]) > 1:
                    d["listes"] = [l for l in d["listes"] if l["nom"] != p.get("nom")]
            return {"ok": True, "listes": d["listes"]}

    reg.api("taches", api)
    reg.contexte(contexte)
    reg.outil("ajouter_tache", "Ajoute une tâche (chose à faire) dans une liste.",
              {"texte": "la tâche", "liste": "nom de la liste (facultatif : courses, devoirs…)"}, ["texte"], ajouter_tache)
    reg.outil("terminer_tache", "Coche une tâche comme faite.", {"texte": "la tâche (ou un mot de la tâche)"},
              ["texte"], terminer_tache)
    reg.outil("lire_taches", "Dit les tâches qui restent à faire.", {"liste": "nom de la liste (facultatif)"}, [], lire_taches)
    reg.regle(r"^(?:qu'est ce que|qu est ce que|qu'est-ce que) j'ai a faire|^(?:lis|dis|donne)(?: moi)? mes taches|^mes taches$",
              lambda m, t: [("lire_taches", {})], priorite=30)
