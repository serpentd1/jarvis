"""Mémoire à long terme en GRAPHE : chaque chose retenue est un nœud (toi, tes potes, tes projets, tes goûts…)
relié aux autres par des liens (« aime », « est en classe avec », « prépare »…).

- Après chaque échange, l'IA locale repère ce qui vaut la peine d'être retenu et l'ajoute au graphe.
- « Souviens-toi que… », « qu'est-ce que tu sais sur moi ? », « oublie… ».
- Avant de répondre, Jarvis relit les nœuds liés à ta question.
- La grande application affiche ce graphe en carte mentale interactive.
Tout reste sur ton PC (%APPDATA%\\Jarvis\\donnees\\memoire.json). Jamais de mots de passe ni de codes.
"""
import json
import logging
import re
import time

from core import ollama
from core.stockage import Stockage
from . import Resultat, normaliser

# Souvenirs qui parlent de notes scolaires (cachés quand l'utilisateur masque ses notes dans l'île).
NOTES = re.compile(r"moyenne|bulletin|/\s?20\b|sur 20|\bune note\b|\bmes notes\b|\bses notes\b|\bnote de\b")

log = logging.getLogger("memoire")
TYPES = ["moi", "personne", "lieu", "ecole", "projet", "preference", "habitude", "activite", "objet", "evenement", "autre"]
DONNEES_SENSIBLES = re.compile(
    r"\b(?:mot de passe|password|passphrase|code pin|code de verification|code de securite|code sms|"
    r"cles? api|api key|access token|token|jeton d acces|bearer)\b",
    re.IGNORECASE,
)
JETON_SECRET = re.compile(
    r"\b(?:gh[pousr]_[a-z0-9_]{20,}|github_pat_[a-z0-9_]{20,}|glpat-[a-z0-9_-]{20,})\b",
    re.IGNORECASE,
)
MOTS_VIDES = {"le", "la", "les", "un", "une", "des", "de", "du", "d", "l", "et", "a", "au", "aux", "en", "mon", "ma",
              "mes", "ton", "ta", "tes", "son", "sa", "ses", "je", "tu", "il", "elle", "on", "que", "qui", "quoi",
              "est", "ce", "ca", "pour", "avec", "sur", "dans", "pas", "ne", "plus", "moi", "toi"}


def contient_secret(*textes: str) -> bool:
    brut = " ".join(textes)
    return bool(DONNEES_SENSIBLES.search(normaliser(brut)) or JETON_SECRET.search(brut))
CONSIGNE = (
    "Tu construis la mémoire à long terme de l'assistant d'un collégien. À partir de l'échange ci-dessous, extrais "
    "UNIQUEMENT les informations DURABLES sur l'utilisateur : identité, goûts et préférences, personnes de son "
    "entourage (amis, famille, profs), école et matières, projets, habitudes, activités, objets à lui, lieux, "
    "événements prévus. Ignore les questions ponctuelles (météo, heure, calculs, commandes), les infos venant "
    "seulement de l'assistant, et JAMAIS de mots de passe, codes ou numéros secrets.\n"
    "Réponds en JSON : {\"faits\": [{\"sujet\": \"Moi\", \"relation\": \"aime\", \"objet\": \"la musique électro\", "
    "\"type_objet\": \"preference\"}]}. sujet = « Moi » pour l'utilisateur. type_objet parmi : "
    + ", ".join(TYPES[1:]) + ". Phrases courtes. {\"faits\": []} s'il n'y a rien à retenir."
)


def cle(nom: str) -> str:
    return normaliser(nom).replace(" ", "_")[:60] or "?"


def installer(ctx, reg):
    base = Stockage("memoire", {"noeuds": [{"id": "moi", "nom": "Moi", "type": "moi", "cree": time.time()}], "liens": []})

    def noeud(d, nom: str, type_: str = "autre"):
        if normaliser(nom) in ("moi", "je", "l'utilisateur", "utilisateur"):
            return next(n for n in d["noeuds"] if n["id"] == "moi")
        id_ = cle(nom)
        n = next((n for n in d["noeuds"] if n["id"] == id_), None)
        if not n:
            n = {"id": id_, "nom": nom.strip()[:60], "type": type_ if type_ in TYPES else "autre", "cree": time.time()}
            d["noeuds"].append(n)
        n["vu"] = time.time()
        return n

    def ajouter_faits(faits: list) -> int:
        ajoutes = 0
        with base as d:
            for f in faits:
                s, r, o = (str(f.get(k, "")).strip() for k in ("sujet", "relation", "objet"))
                if not s or not r or not o or len(o) > 80 or contient_secret(s, r, o):
                    continue
                a = noeud(d, s, "personne")
                b = noeud(d, o, str(f.get("type_objet", "autre")))
                if a["id"] == b["id"]:
                    continue
                lien = next((l for l in d["liens"] if l["a"] == a["id"] and l["b"] == b["id"]), None)
                if lien:
                    lien["relation"], lien["maj"] = r[:40], time.time()
                else:
                    d["liens"].append({"a": a["id"], "b": b["id"], "relation": r[:40], "cree": time.time()})
                    ajoutes += 1
        return ajoutes

    def extraire(texte: str) -> list:
        reponse = ollama.discuter(ctx.cfg["cerveau"], [{"role": "system", "content": CONSIGNE},
                                                        {"role": "user", "content": texte}],
                                  temperature=0.0, max_mots=400, json_seulement=True)
        try:
            faits = json.loads(reponse or "{}").get("faits", [])
            return [f for f in faits if isinstance(f, dict)]
        except ValueError:
            return []

    def apres_echange(question: str, reponse: str):
        if len(question) < 12 or re.match(r"^\(?simul", reponse or ""):
            return
        n = ajouter_faits(extraire(f"L'utilisateur : {question}\nL'assistant : {reponse}"))
        if n:
            log.info("mémoire : %d nouveau(x) lien(s)", n)

    def notes_masquees() -> bool:
        givre = getattr(ctx, "givre", None)
        e = (givre.etat() if givre else None) or {}
        return bool((e.get("pronote") or {}).get("notes_masquees"))

    def contexte(question: str = ""):
        mots = {m for m in normaliser(question).split() if len(m) > 2 and m not in MOTS_VIDES}
        with base as d:
            noms = {n["id"]: n["nom"] for n in d["noeuds"]}
            touches = {n["id"] for n in d["noeuds"] if mots & set(normaliser(n["nom"]).split())}
            liens = [l for l in d["liens"] if l["a"] == "moi" or l["a"] in touches or l["b"] in touches]
        if not liens:
            return ""
        liens.sort(key=lambda l: (l["a"] in touches or l["b"] in touches, l.get("maj", l["cree"])), reverse=True)
        lignes = [f"{noms.get(l['a'], l['a'])} {l['relation']} {noms.get(l['b'], l['b'])}" for l in liens[:14]]
        if notes_masquees():          # notes cachées dans l'île : la mémoire n'en parle pas non plus
            lignes = [x for x in lignes if not NOTES.search(x.lower())]
        if not lignes:
            return ""
        return "CE QUE TU SAIS SUR L'UTILISATEUR (ta mémoire, « Moi » = lui) : " + " ; ".join(lignes) + "."

    def retenir(fait: str):
        if contient_secret(fait):
            return Resultat("Je ne conserve pas les mots de passe, codes ou clés secrètes.", ok=False)
        faits = extraire(f"L'utilisateur : Souviens-toi de ça : {fait}")
        if not faits:
            faits = [{"sujet": "Moi", "relation": "a dit", "objet": fait[:80], "type_objet": "autre"}]
        ajouter_faits(faits)
        return Resultat("C'est noté, je m'en souviendrai.")

    def ce_que_tu_sais(sujet: str = ""):
        with base as d:
            noms = {n["id"]: n["nom"] for n in d["noeuds"]}
            cible = cle(sujet) if sujet and normaliser(sujet) not in ("moi", "toi") else "moi"
            liens = [l for l in d["liens"] if cible in (l["a"], l["b"])]
        if not liens:
            return Resultat("Je ne sais encore rien là-dessus." if sujet else "Je ne sais encore presque rien sur toi.")
        phrases = [f"{noms.get(l['a'], '?')} {l['relation']} {noms.get(l['b'], '?')}".replace("Moi ", "tu ", 1)
                   for l in liens]
        if notes_masquees():
            phrases = [p for p in phrases if not NOTES.search(p.lower())]
        if not phrases:
            return Resultat("Je ne sais encore presque rien sur toi.")
        return Resultat("Je sais que " + ", ".join(phrases[-8:]) + ".")

    def oublier_souvenir(sujet: str):
        id_ = cle(sujet)
        with base as d:
            avant = len(d["noeuds"])
            d["noeuds"] = [n for n in d["noeuds"] if n["id"] != id_ or n["id"] == "moi"]
            d["liens"] = [l for l in d["liens"] if id_ not in (l["a"], l["b"])]
            fait = len(d["noeuds"]) < avant
        return Resultat(f"J'ai oublié « {sujet} »." if fait else f"Je n'avais rien sur « {sujet} ».")

    def ajouter_note(nom: str, type_: str = "autre"):
        nom = str(nom).strip()
        if not nom or len(nom) > 60:
            return {"ok": False, "erreur": "Une note doit contenir entre 1 et 60 caractères."}
        if contient_secret(nom):
            return {"ok": False, "erreur": "Les mots de passe, codes et clés secrètes ne sont pas mémorisés."}
        with base as d:
            if any(n["id"] == cle(nom) for n in d["noeuds"]):
                return {"ok": False, "erreur": "Cette note existe déjà."}
            note = noeud(d, nom, type_)
            d["liens"].append({"a": "moi", "b": note["id"], "relation": "a noté", "cree": time.time()})
        return {"ok": True}

    def ajouter_lien(a: str, b: str, relation: str):
        a, b, relation = str(a).strip(), str(b).strip(), str(relation).strip()
        if not a or not b or a == b:
            return {"ok": False, "erreur": "Choisis deux notes différentes à relier."}
        if not relation or len(relation) > 40:
            return {"ok": False, "erreur": "La relation doit contenir entre 1 et 40 caractères."}
        with base as d:
            ids = {n["id"] for n in d["noeuds"]}
            if a not in ids or b not in ids:
                return {"ok": False, "erreur": "Une des notes choisies n'existe plus."}
            lien = next((l for l in d["liens"] if l["a"] == a and l["b"] == b), None)
            if lien:
                lien["relation"], lien["maj"] = relation, time.time()
            else:
                d["liens"].append({"a": a, "b": b, "relation": relation, "cree": time.time()})
        return {"ok": True}

    def api(methode, p):
        if methode not in ("GET", "POST"):
            return {"ok": False, "erreur": "Méthode non prise en charge."}
        if methode == "POST":
            a = p.get("action")
            if a == "ajouter":
                texte = str(p.get("texte", "")).strip()
                if not texte:
                    return {"ok": False, "erreur": "Écris d’abord le contenu du souvenir."}
                resultat = retenir(texte)
                if not resultat.ok:
                    return {"ok": False, "erreur": resultat.dire}
            elif a == "ajouter_note":
                resultat = ajouter_note(str(p.get("nom", "")), str(p.get("type", "autre")))
                if not resultat["ok"]:
                    return resultat
            elif a == "lier":
                resultat = ajouter_lien(str(p.get("a", "")), str(p.get("b", "")), str(p.get("relation", "")))
                if not resultat["ok"]:
                    return resultat
            elif a == "supprimer":
                id_ = str(p.get("id", ""))
                if not id_ or id_ == "moi":
                    return {"ok": False, "erreur": "Ce souvenir ne peut pas être supprimé."}
                with base as d:
                    if not any(n["id"] == id_ for n in d["noeuds"]):
                        return {"ok": False, "erreur": "Ce souvenir n'existe plus."}
                    d["noeuds"] = [n for n in d["noeuds"] if n["id"] != id_]
                    d["liens"] = [l for l in d["liens"] if id_ not in (l["a"], l["b"])]
            elif a == "supprimer_lien":
                source, cible = str(p.get("a", "")), str(p.get("b", ""))
                if not source or not cible:
                    return {"ok": False, "erreur": "Le lien à supprimer est incomplet."}
                with base as d:
                    avant = len(d["liens"])
                    d["liens"] = [l for l in d["liens"] if not (l["a"] == source and l["b"] == cible)]
                    if len(d["liens"]) == avant:
                        return {"ok": False, "erreur": "Ce lien n'existe plus."}
            else:
                return {"ok": False, "erreur": "Action mémoire inconnue."}
        with base as d:
            noeuds, liens = d["noeuds"], d["liens"]
            if notes_masquees():          # notes cachées dans l'île : pas de « moyenne de 12 » sur la carte non plus
                caches = {n["id"] for n in noeuds if n["id"] != "moi" and NOTES.search(n["nom"].lower())}
                noeuds = [n for n in noeuds if n["id"] not in caches]
                liens = [l for l in liens if l["a"] not in caches and l["b"] not in caches]
            return {"ok": True, "noeuds": noeuds, "liens": liens}

    reg.api("memoire", api)
    reg.contexte(contexte)
    if not getattr(ctx, "simulation", False):
        reg.apres_echange(apres_echange)
    reg.outil("retenir", "Retient durablement une information sur l'utilisateur (« souviens-toi que… »).",
              {"fait": "ce qu'il faut retenir, en une phrase"}, ["fait"], retenir)
    reg.outil("ce_que_tu_sais", "Dit ce que Jarvis sait (sa mémoire) sur l'utilisateur ou sur un sujet.",
              {"sujet": "personne ou sujet (vide = l'utilisateur)"}, [], ce_que_tu_sais)
    reg.outil("oublier_souvenir", "Oublie tout ce qui concerne un sujet dans la mémoire.", {"sujet": "le sujet"},
              ["sujet"], oublier_souvenir)
    reg.regle(r"^(?:souviens toi|rappelle toi|retiens|n'oublie pas|n oublie pas)(?: bien)? (?:que |qu'|de |d')?(?P<f>.+)$",
              lambda m, t: [("retenir", {"fait": re.sub(r"(?i)^\W*(souviens[- ]toi|rappelle[- ]toi|retiens|n'oublie pas)"
                                                         r"(?: bien)?\s*(?:que |qu'|de |d')?", "", t).strip()})], priorite=21)
    reg.regle(r"^(?:qu'est ce que|qu est ce que|que) tu sais (?:sur|de) (?P<s>.+?)$",
              lambda m, t: [("ce_que_tu_sais", {"sujet": m["s"]})], priorite=21)
