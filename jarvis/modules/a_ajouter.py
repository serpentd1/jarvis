"""« Je ne sais pas encore faire ça » : chaque demande que Jarvis n'a pas su faire est écrite, avec sa réponse
exacte, dans le fichier texte a-ajouter.txt (à côté de config.toml). Claude le lit pour ajouter ce qui manque,
puis efface les lignes réglées. « Qu'est-ce que tu ne sais pas faire ? » lit les dernières.
"""
import re
import threading
from datetime import datetime

from core import config
from core.stockage import Stockage
from . import Resultat, masquer_codes, normaliser

FICHIER = config.RACINE / "a-ajouter.txt"
ENTETE = """\
# Ce que Jarvis n'a pas su faire : ta demande, puis sa réponse exacte.
# Pour le lui apprendre, dis à Claude : « regarde a-ajouter.txt de Jarvis et ajoute ce qui manque ».
# Une ligne réglée peut être effacée (par Claude ou par toi). Jarvis ne réécrit pas une demande déjà notée :
# il augmente juste son nombre de fois.
"""
# 2026-11-17 10:34 · 1 fois · « demande » → « réponse »
LIGNE = re.compile(r"^(?P<date>\S+ \S+) · (?P<fois>\d+) fois · « (?P<demande>.*?) » → « (?P<reponse>.*) »$")

# Réponses de Jarvis qui veulent dire « je ne sais pas (encore) faire ça » (texte normalisé).
IMPOSSIBLE = re.compile(
    r"\b(?:je ne sais pas encore|je ne sais pas (?:le )?faire|je ne peux pas|je n'ai pas (?:acces|d'outil|la possibilite"
    r"|le moyen|les moyens|la capacite)|je ne suis pas (?:capable|en mesure)|aucun outil|impossible pour moi"
    r"|je n'arrive pas a)")

_verrou = threading.Lock()


def _propre(texte: str, taille: int) -> str:
    return re.sub(r"\s+", " ", masquer_codes(texte or "")).strip()[:taille].replace(" » → « ", " » -> « ")


def lire() -> list[dict]:
    """Les demandes notées, de la plus ancienne à la plus récente."""
    try:
        lignes = FICHIER.read_text(encoding="utf-8").splitlines()
    except OSError:
        return []
    return [m.groupdict() for m in map(LIGNE.match, lignes) if m]


def _ecrire(demandes: list[dict]):
    corps = "".join(f"{d['date']} · {d['fois']} fois · « {d['demande']} » → « {d['reponse']} »\n" for d in demandes)
    FICHIER.write_text(ENTETE + "\n" + corps, encoding="utf-8")


_derniere = {"question": "", "reponse": ""}      # dernier échange (pour « note-le dans ce que tu ne sais pas faire »)
DEMANDE_DE_NOTER = re.compile(r"\b(?:note|ecris|mets|ajoute)(?: le| la| ca)?\b.*\b(?:(?:ne )?(?:sais|peux) pas faire|a ajouter"
                              r"|a apprendre|fichier texte|liste)\b")


def retenir_echange(question: str, reponse: str):
    if question and not DEMANDE_DE_NOTER.search(normaliser(question)):
        _derniere.update(question=question, reponse=reponse or "")


def noter(question: str, reponse: str, force: bool = False):
    if not question or not (force or IMPOSSIBLE.search(normaliser(reponse or ""))):
        return
    maintenant = datetime.now().strftime("%Y-%m-%d %H:%M")
    cle = normaliser(question)
    with _verrou:
        demandes = lire()
        for d in demandes:
            if normaliser(d["demande"]) == cle:      # déjà notée : on compte les fois et on garde la dernière réponse
                d["date"], d["fois"], d["reponse"] = maintenant, int(d["fois"]) + 1, _propre(reponse, 600)
                break
        else:
            demandes.append({"date": maintenant, "fois": 1, "demande": _propre(question, 300),
                             "reponse": _propre(reponse, 600)})
        _ecrire(demandes[-300:])


def reprendre_ancienne_liste():
    """Une seule fois : recopie l'ancienne liste (a_ajouter.json dans AppData) dans le fichier texte."""
    if FICHIER.exists():
        return
    anciennes = Stockage("a_ajouter", {"demandes": []}).donnees.get("demandes", [])
    _ecrire([{"date": str(x.get("derniere", "")).replace("T", " ") or "?", "fois": x.get("fois", 1),
              "demande": _propre(x.get("demande", ""), 300), "reponse": _propre(x.get("reponse", ""), 600)}
             for x in anciennes if x.get("demande")])


def installer(ctx, reg):
    def choses_a_ajouter():
        demandes = lire()
        if not demandes:
            return Resultat("Pour l'instant, je n'ai rien noté qui me manque.")
        s = "s" if len(demandes) > 1 else ""
        derniers = [x["demande"] for x in demandes[-5:]][::-1]
        return Resultat(f"J'ai noté {len(demandes)} demande{s} qui me manque{'nt' if s else ''} encore. "
                        "Les dernières : " + " ; ".join(derniers) + ".")

    if not getattr(ctx, "simulation", False):            # les tests ne remplissent pas la vraie liste
        try:
            reprendre_ancienne_liste()
        except Exception:  # noqa: BLE001
            pass
        reg.apres_echange(noter)
        reg.apres_echange(retenir_echange)

    def noter_a_ajouter(demande: str = ""):
        """L'utilisateur (ou l'IA) demande de noter une chose que Jarvis ne sait pas encore faire."""
        demande = (demande or "").strip() or _derniere["question"]
        if not demande:
            return Resultat("Dis-moi ce que je dois noter.", ok=False)
        if not getattr(ctx, "simulation", False):
            noter(demande, _derniere["reponse"] if demande == _derniere["question"] else "(noté à la demande)", force=True)
        return Resultat(f"C'est noté dans la liste de ce que je dois apprendre : « {demande[:120]} ».")

    reg.outil("choses_a_ajouter", "Dit ce que Jarvis n'a pas su faire récemment (noté pour l'ajouter plus tard).",
              {}, [], choses_a_ajouter)
    reg.outil("noter_a_ajouter", "Note dans a-ajouter.txt une demande que Jarvis ne sait pas encore faire, pour qu'elle "
              "soit ajoutée plus tard. Sans demande : la dernière question de l'utilisateur.",
              {"demande": "ce que l'utilisateur voulait faire (facultatif)"}, [], noter_a_ajouter)
    reg.regle(r"^(?:ok |alors |vas y )?(?:note|ecris|mets|ajoute)(?: le| la| ca| moi ca| le moi)?"
              r" (?:dans|sur) (?:ta |la |le |un |une )?(?:liste|fichier|truc|chose)?.*"
              r"(?:(?:ne )?(?:sais|peux) pas (?:encore )?faire|a ajouter|a apprendre)$",
              lambda m, t: [("noter_a_ajouter", {})], priorite=19)
    reg.regle(r"^(?:qu'est ce que|quelles sont les choses que|dis moi ce que) tu ne (?:sais|peux) pas (?:encore )?faire$",
              lambda m, t: [("choses_a_ajouter", {})], priorite=20)
