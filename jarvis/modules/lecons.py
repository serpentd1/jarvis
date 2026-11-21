"""Réviser une leçon avec Jarvis, à l'oral.

- « Apprends ma leçon de maths » : il garde le texte que tu as copié (sélectionne ton cours, Ctrl+C) ;
  « apprends ma leçon de maths qui est sur l'écran » : il la lit sur ton écran (photo, PDF, Pronote…).
- « Fais-moi réciter ma leçon de maths » : il pose une question à la fois, te dit si c'est juste, te corrige
  et compte tes points. « Stop » ou « j'ai fini » arrête la révision et donne ton score.
"""
import json
import logging
import re
import time
from datetime import date

from core import controle, ollama
from core.stockage import Stockage
from . import Resultat, normaliser
from .presse_papiers import lire_presse_papiers

log = logging.getLogger("lecons")
FIN = re.compile(r"^(?:(?:hey )?jarvis )?(?:stop|arrete|on arrete|j'ai fini|c'est fini|fin de la revision|"
                 r"ca suffit|c'est bon|au revoir)\b")
CONSIGNE = (
    "Tu fais réciter sa leçon à un collégien, à l'oral. Voici la leçon :\n<<<\n{lecon}\n>>>\n"
    "Pose UNE seule question courte à la fois, uniquement sur cette leçon, en variant (définitions, propriétés, "
    "formules, exemples, vocabulaire) et sans répéter une question déjà posée. Quand il répond, dis d'abord si c'est "
    "juste ; si c'est faux ou incomplet, donne gentiment la bonne réponse en une phrase. Puis pose la question "
    "suivante. Tout est lu à voix haute : phrases courtes, tutoiement, pas de markdown, formules écrites en mots "
    "(« a au carré plus b au carré »). Réponds UNIQUEMENT en JSON : "
    '{{"juste": true, false ou null (null s\'il n\'a pas encore répondu), "dire": "ce que tu dis"}}.'
)


def lecon_depuis_pronote(p: dict, matiere: str = "", maintenant: str | None = None):
    """Une « leçon » à réviser tirée de Pronote : les devoirs de la matière demandée, sinon ceux du dernier cours
    de la journée (déjà commencé). None si Pronote n'a rien d'utile."""
    if not p or not p.get("connecte"):
        return None
    maintenant = maintenant or time.strftime("%H:%M")
    m = normaliser(matiere or "")
    if not m:
        passes = [c for c in p.get("cours_aujourdhui") or [] if str(c.get("debut", "")) <= maintenant
                  and not c.get("annule") and not c.get("n_y_va_pas") and not c.get("perso")]
        if passes:
            m = normaliser(passes[-1].get("matiere", ""))
    devoirs = p.get("devoirs") or []
    if not m and devoirs:
        m = normaliser(devoirs[0].get("matiere", ""))
    if not m:
        return None
    mots = [x for x in m.split() if len(x) > 2] or [m]
    choisis = [d for d in devoirs if any(x in normaliser(d.get("matiere", "")) for x in mots)]
    nom = next((d.get("matiere") for d in choisis), None) or matiere or m
    if choisis:
        texte = "\n".join(f"- {d.get('texte', '')}" for d in choisis[:5])
        sujet = (f"Matière : {nom}. Voici ses devoirs sur Pronote (ils disent ce qu'il étudie en ce moment) :\n{texte}\n"
                 "Pose des questions de niveau collège sur CES notions (définitions, exemples, méthodes), avec tes "
                 "propres connaissances.")
    else:
        sujet = (f"Matière : {nom}. Il n'y a pas de devoir noté dans Pronote pour cette matière : pose des questions "
                 "de niveau collège sur les notions principales de cette matière au programme de son année.")
    return {"matiere": nom, "texte": sujet, "cree": date.today().isoformat(), "pronote": True}


def installer(ctx, reg):
    base = Stockage("lecons", {"lecons": []})
    revision = {}        # révision en cours : lecon, messages, juste, total, derniere

    def trouver(matiere: str):
        lecons = base.donnees["lecons"]
        m = normaliser(matiere or "")
        for lecon in reversed(lecons):
            nom = normaliser(lecon["matiere"])
            if m and nom and (m in nom or nom in m):
                return lecon
        return lecons[-1] if lecons and not m else None

    def lecon_pronote(matiere: str = ""):
        """Pas de leçon enregistrée : on révise d'après Pronote (devoirs de la matière, ou du dernier cours)."""
        givre = getattr(ctx, "givre", None)
        e = (givre.etat() if givre else None) or {}
        return lecon_depuis_pronote(e.get("pronote") or {}, matiere)

    def demander_ia(messages):
        brut = ollama.discuter(ctx.cfg["cerveau"], messages, temperature=0.4, max_mots=120, json_seulement=True)
        try:
            d = json.loads(brut or "{}")
            return d.get("juste"), str(d.get("dire") or "").strip()
        except (ValueError, AttributeError):
            return None, str(brut or "").strip()

    def apprendre_lecon(matiere: str = "", source: str = "presse_papiers"):
        matiere = (matiere or "").strip()
        if source == "ecran":
            voir = getattr(ctx, "voir", None)
            texte = voir("Recopie fidèlement tout le texte de la leçon visible sur cette capture d'écran (titres, "
                         "définitions, propriétés, formules, exemples). Rien d'autre.",
                         controle.capture(), 0.1, 1500) if voir else None
            if not texte:
                return Resultat("Je n'arrive pas à lire la leçon sur ton écran.", ok=False)
        else:
            texte = (lire_presse_papiers() or "").strip()
            if len(texte) < 40:
                return Resultat("Copie d'abord ta leçon : sélectionne-la puis fais Contrôle C, et redis-le-moi. "
                                "Ou dis : apprends ma leçon qui est sur l'écran.", ok=False)
        with base as d:
            d["lecons"] = [x for x in d["lecons"] if normaliser(x["matiere"]) != normaliser(matiere)]
            d["lecons"] = (d["lecons"] + [{"matiere": matiere, "texte": texte[:8000],
                                           "cree": date.today().isoformat()}])[-20:]
        de = f" de {matiere}" if matiere else ""
        return Resultat(f"C'est noté, j'ai ta leçon{de}. Dis « fais-moi réciter ma leçon{de} » quand tu veux.")

    def reciter_lecon(matiere: str = ""):
        lecon = trouver(matiere) or lecon_pronote(matiere)
        de = f" de {matiere}" if matiere else ""
        if not lecon:
            return Resultat(f"Je n'ai pas encore ta leçon{de}, et je ne trouve rien sur Pronote. Copie-la, puis dis : "
                            f"apprends ma leçon{de}.", ok=False)
        revision.clear()
        revision.update(lecon=lecon, juste=0, total=0, derniere=time.time(), messages=[
            {"role": "system", "content": CONSIGNE.format(lecon=lecon["texte"])},
            {"role": "user", "content": "Je suis prêt, pose-moi la première question."}])
        _, dire = demander_ia(revision["messages"])
        revision["messages"].append({"role": "assistant", "content": dire})
        de = f" de {lecon['matiere']}" if lecon["matiere"] else ""
        if lecon.get("pronote"):
            return Resultat(f"Je n'ai pas ta leçon{de} enregistrée, alors je t'interroge d'après Pronote. Dis « stop » "
                            f"pour arrêter. {dire}")
        return Resultat(f"C'est parti pour ta leçon{de} ! Dis « stop » pour arrêter. {dire}")

    def bilan():
        if not revision:
            return None
        j, t = revision.get("juste", 0), revision.get("total", 0)
        revision.clear()
        if not t:
            return "D'accord, on arrête la révision."
        s = "s" if j > 1 else ""
        fin = ("Bravo, tu la connais très bien !" if j >= 0.8 * t else "Pas mal, continue comme ça !" if j >= 0.5 * t
               else "Relis-la encore un peu, et on recommence.")
        return f"Fin de la révision : {j} bonne{s} réponse{s} sur {t}. {fin}"

    def intercepter(texte: str, n: str):
        """Pendant une révision, ce que tu dis est ta réponse à la question."""
        if not revision:
            return None
        if time.time() - revision["derniere"] > 300:        # révision abandonnée depuis 5 min
            revision.clear()
            return None
        if FIN.match(n):
            return bilan()
        revision["derniere"] = time.time()
        msgs = revision["messages"]
        msgs.append({"role": "user", "content": texte})
        juste, dire = demander_ia(msgs if len(msgs) <= 13 else msgs[:1] + msgs[-12:])
        if juste is not None:
            revision["total"] += 1
            revision["juste"] += bool(juste)
        msgs.append({"role": "assistant", "content": dire})
        return dire or "Je n'ai pas compris, tu peux répéter ?"

    reg.intercepteur(intercepter)
    reg.fin_conversation(bilan)
    reg.outil("apprendre_lecon", "Enregistre une leçon pour la faire réciter plus tard : le texte copié, ou ce qui "
              "est affiché à l'écran.", {"matiere": "la matière ou le titre (ex : maths)",
                                          "source": {"type": "string", "enum": ["presse_papiers", "ecran"]}},
              [], apprendre_lecon)
    reg.outil("reciter_lecon", "Fait réviser l'utilisateur à l'oral (« fais-moi réviser ») : pose des questions une par "
              "une et corrige. Prend la leçon enregistrée, sinon ce qu'il étudie d'après Pronote (devoirs de la "
              "matière ou du dernier cours).", {"matiere": "la matière (ex : maths), vide = la dernière leçon"},
              [], reciter_lecon)
    reg.regle(r"^(?:(?:vas y |allez |bon )?(?:fais|fait|faire)(?: moi)? reviser|(?:on|je veux|j'aimerais|je voudrais) "
              r"(?:reviser|reviser un peu)|interroge moi|teste moi|aide moi a reviser|on revise)"
              r"(?: (?:ma |la |mon |mes |les )?(?:derniere )?(?:lecons?|cours))?"
              r"(?: (?:de |d'|en |sur (?:le |la |les |l')?)(?P<m>[a-z' ]+?))?(?: s'il te plait| stp)?$",
              lambda m, t: [("reciter_lecon", {"matiere": (m["m"] or "").strip()})], priorite=21)
    reg.regle(r"^(?:apprends|retiens|enregistre|garde|memorise)(?: moi)? (?:ma|la|cette|mon) (?:lecon|cours)"
              r"(?: d[e'] ?(?P<m>[a-z' ]+?))?(?P<e> (?:qui est )?(?:sur|a) (?:l'|l |mon )?ecran)?$",
              lambda m, t: [("apprendre_lecon", {"matiere": (m["m"] or "").strip(),
                                                 "source": "ecran" if m["e"] else "presse_papiers"})], priorite=20)
    reg.regle(r"^(?:fais|fait|faire)(?: moi)? (?:reciter|reviser) (?:ma|la|mon) (?:lecon|cours)"
              r"(?: d[e'] ?(?P<m>[a-z' ]+))?$|^(?:interroge|teste)(?: moi)? (?:sur )?(?:ma|la|mon) (?:lecon|cours)"
              r"(?: d[e'] ?(?P<m2>[a-z' ]+))?$",
              lambda m, t: [("reciter_lecon", {"matiere": (m["m"] or m["m2"] or "").strip()})], priorite=20)
