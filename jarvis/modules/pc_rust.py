"""Pont vers les POUVOIRS SUR LE PC, écrits en Rust dans JarvisBarre.exe (barre/src/pouvoirs.rs).

Aucune logique ici : la voix demande à la barre (API locale 127.0.0.1:47802) la liste des outils
(GET /outils), les appelle (POST /outil) et transmet ton « oui » (POST /outil/confirmer).
Fichiers, dossiers, code, pages web, PowerShell, processus : sécurité et confirmations sont côté Rust.
Ce fichier disparaîtra quand le cerveau de la voix sera lui aussi passé en Rust.
"""
import logging
import re
import time

import requests

from . import Outil, Regle, Resultat

log = logging.getLogger("pc_rust")
# la voix a déjà ses propres outils pour ça (applis.py, pc.py) : on ne les double pas
DEJA_LA = {"lancer_programme": "lancer_application", "alimentation_pc": "action_pc"}


def installer(ctx, reg):
    url = ctx.cfg["givre"]["url"].rstrip("/")

    def confirmer(id_, oui=True):
        try:
            r = requests.post(f"{url}/outil/confirmer", json={"id": id_, "oui": oui}, timeout=330).json()
        except Exception:  # noqa: BLE001
            return Resultat("La barre de Jarvis ne répond pas.", ok=False)
        return _resultat(r)

    def _resultat(r: dict) -> Resultat:
        dire = r.get("dire") or ("C'est fait." if r.get("ok") else "Ça n'a pas marché.")
        if r.get("confirmer"):
            id_ = int(r["confirmer"])
            return Resultat(dire, confirmer=lambda: confirmer(id_, True), annuler="D'accord, je ne touche à rien.")
        return Resultat(dire, ok=bool(r.get("ok")), pour_ia=str(r.get("pour_ia") or ""))

    def appeler(nom, args):
        try:
            r = requests.post(f"{url}/outil", json={"nom": nom, "args": args}, timeout=330).json()
        except Exception:  # noqa: BLE001
            return Resultat("La barre de Jarvis ne répond pas : je ne peux pas toucher aux fichiers.", ok=False)
        return _resultat(r)

    def charger():
        """La barre démarre en même temps que la voix : on attend qu'elle réponde (2 min au plus)."""
        fin = time.time() + 120
        while time.time() < fin:
            try:
                donnees = requests.get(f"{url}/outils", timeout=3).json()
                outils = donnees.get("outils") or []
                break
            except Exception:  # noqa: BLE001
                time.sleep(3)
        else:
            log.warning("barre absente : pas de pouvoirs sur le PC")
            return
        nouveaux = {}
        for o in outils:
            f = o.get("function") or {}
            nom = f.get("name")
            if not nom or nom in reg.outils or DEJA_LA.get(nom) in reg.outils:
                continue
            p = f.get("parameters") or {}
            nouveaux[nom] = Outil(nom, f.get("description", ""), p.get("properties") or {}, p.get("required") or [],
                                  lambda _n=nom, **a: appeler(_n, a))
        reg.outils = {**reg.outils, **nouveaux}      # remplacé d'un coup (le cerveau peut lire en même temps)
        # raccourcis vocaux définis côté Rust : "$x" = groupe x du motif, "$$" = toute la phrase
        regles = []
        for r in donnees.get("regles") or []:
            try:
                regles.append(Regle(re.compile(r["motif"], re.IGNORECASE), _raccourci(r), int(r.get("priorite", 10))))
            except (KeyError, re.error):
                log.warning("raccourci Rust invalide : %s", r)
        reg.regles = sorted(reg.regles + regles, key=lambda x: x.priorite)
        log.info("pouvoirs sur le PC (Rust) : %d outils, %d raccourcis", len(nouveaux), len(regles))

    def _raccourci(r):
        def fonction(m, texte):
            args = {}
            for k, v in (r.get("args") or {}).items():
                if v == "$$":
                    v = texte
                elif isinstance(v, str) and v.startswith("$"):
                    v = m.group(v[1:]) or ""
                args[k] = v
            return [(r["outil"], args)]
        return fonction

    reg.tache(charger)
    reg.regle(r"^(?:eteins|arrete|coupe) (?:le pc|l'ordi|l'ordinateur|mon pc|mon ordi|mon ordinateur)$",
              lambda m, t: [("action_pc", {"action": "eteindre"})], priorite=20)
    reg.regle(r"^(?:redemarre|relance) (?:le pc|l'ordi|l'ordinateur|mon pc|mon ordi|mon ordinateur)$",
              lambda m, t: [("action_pc", {"action": "redemarrer"})], priorite=20)
    reg.regle(r"^mets? (?:le pc|l'ordi|l'ordinateur|mon pc|mon ordi) en veille$",
              lambda m, t: [("action_pc", {"action": "veille"})], priorite=20)
    reg.regle(r"^(?:qu'est ce qu'il y a|montre moi ce qu'il y a|liste ce qu'il y a|liste les fichiers) "
              r"(?:sur|dans) (?:le |mon |mes )?(bureau|documents|telechargements|images|musique|videos)$",
              lambda m, t: [("lister_dossier", {"chemin_dossier": m.group(1)})], priorite=20)
    reg.regle(r"^(?:tue|force la fermeture de|ferme de force) (?:le processus )?(?P<n>[\w.-]+)$",
              lambda m, t: [("fermer_processus", {"nom": m["n"]})], priorite=20)
