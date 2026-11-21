"""Assistant pour connecter ses comptes (Pronote, mails, Discord), pas à pas et à la voix.

« Aide-moi à connecter mes comptes » / « comment je connecte Pronote ? » : Jarvis ouvre la page Comptes de la
grande île, dit ce qui est déjà connecté, puis donne UNE étape à la fois. « Suivant » (ou « c'est fait ») passe à la
suivante, « répète » la redit, « stop » arrête. Marche pour n'importe qui (appli partagée) : rien n'est propre à
un compte.
"""
import re
import time

from . import Resultat, normaliser

ETAPES = {
    "pronote": [
        "Pour Pronote : ouvre Pronote dans ton navigateur, en passant par ton ENT si tu en as un, et connecte-toi "
        "comme d'habitude.",
        "En haut de Pronote, clique sur l'icône du QR code, celle de l'application mobile. Choisis un code à 4 "
        "chiffres de ton choix : le QR code s'affiche.",
        "Fais Windows plus Majuscule plus S, et sélectionne le QR code pour le copier.",
        "Dans la page Comptes de l'île, clique sur « Coller le QR code », tape ton code à 4 chiffres, puis « Se "
        "connecter ». Quand le point devient vert, c'est bon !",
    ],
    "mails": [
        "Pour tes mails : dans la page Comptes de l'île, carte « Mails et agenda ».",
        "Si tu as Gmail, clique sur « Se connecter avec Google ». Pour Hotmail ou Outlook, clique sur « Ajouter "
        "Hotmail ». Une page de connexion s'ouvre dans ton navigateur.",
        "Connecte-toi et accepte. Si Google dit « Google n'a pas validé cette application », clique sur Paramètres "
        "avancés, puis Accéder à. Ensuite ton adresse apparaît dans la carte.",
    ],
    "discord": [
        "Pour Discord, il n'y a pas de mot de passe à donner : je lis les notifications Windows de Discord.",
        "Dans Discord, va dans Paramètres utilisateur, Notifications, et active les notifications sur le bureau.",
        "Dans les paramètres de Windows, Système, Notifications : vérifie que Discord est autorisé. Pour répondre "
        "à tes messages, il faut aussi l'extension Chrome de Jarvis : dis « installe l'extension ».",
    ],
}
NOMS = {"pronote": "Pronote", "mails": "tes mails", "discord": "Discord"}
SUIVANT = re.compile(r"^(?:(?:ok |d'accord |c'est bon |voila )?(?:suivant|suivante|etape suivante|apres|et apres|"
                     r"ensuite|c'est fait|j'ai fait|fait|je l'ai fait|c'est bon|ok|d'accord|voila|continue)"
                     r"(?: jarvis)?)$")
REPETE = re.compile(r"^(?:repete|tu peux repeter|redis|quoi|hein|j'ai pas compris)(?: jarvis| s'il te plait| stp)?$")
STOP = re.compile(r"^(?:stop|arrete|c'est bon merci|laisse tomber|annule|fini|termine|j'ai fini)(?: jarvis)?$")


def service_de(texte: str) -> str:
    t = normaliser(texte or "")
    if "pronote" in t:
        return "pronote"
    if re.search(r"\b(?:mail|mails|gmail|hotmail|outlook|courriel|google)\b", t):
        return "mails"
    if "discord" in t:
        return "discord"
    return ""


def deja_connectes(etat: dict) -> tuple[list[str], list[str]]:
    """(connectés, à faire) d'après l'état de la barre."""
    ok, a_faire = [], []
    p = etat.get("pronote") or {}
    if not p.get("desactive"):
        (ok if p.get("connecte") and not p.get("erreur") else a_faire).append("pronote")
    m = etat.get("mails")
    if not (isinstance(m, dict) and m.get("desactive")):
        (a_faire if isinstance(m, dict) and not m.get("connecte", True) else ok).append("mails")
    a_faire.append("discord")              # rien à « connecter », mais l'utilisateur doit vérifier les notifications
    return ok, a_faire


def installer(ctx, reg):
    guide = {}            # en cours : services (liste), i (service), etape, derniere

    def dire_etape() -> str:
        service = guide["services"][guide["i"]]
        etapes = ETAPES[service]
        n = guide["etape"]
        fin = " Dis « suivant » quand c'est fait." if n < len(etapes) - 1 or guide["i"] < len(guide["services"]) - 1 \
            else " Et voilà, c'était la dernière étape !"
        return f"Étape {n + 1} sur {len(etapes)}. {etapes[n]}{fin}"

    def aide_comptes(service: str = ""):
        service = service_de(service)
        givre = getattr(ctx, "givre", None)
        etat = (givre.etat() if givre else None) or {}
        if givre:
            givre.ouvrir("grande", 0, "comptes")
        intro = ""
        if service:
            services = [service]
        else:
            ok, a_faire = deja_connectes(etat) if etat.get("ok") else ([], ["pronote", "mails", "discord"])
            services = a_faire
            if ok:
                intro = "Déjà connectés : " + " et ".join(NOMS[s] for s in ok) + ". "
        guide.clear()
        guide.update(services=services, i=0, etape=0, derniere=time.time())
        return Resultat(f"{intro}J'ouvre la page Comptes de l'île et je te guide pour "
                        f"{' puis '.join(NOMS[s] for s in services)}. {dire_etape()}")

    def intercepter(texte: str, n: str):
        if not guide:
            return None
        if time.time() - guide["derniere"] > 600:          # abandonné depuis 10 min
            guide.clear()
            return None
        if STOP.match(n):
            guide.clear()
            return "D'accord, on arrête. Redis-moi « aide-moi à connecter mes comptes » quand tu veux."
        if REPETE.match(n):
            guide["derniere"] = time.time()
            return dire_etape()
        if not SUIVANT.match(n):
            return None                                   # autre chose : Jarvis répond normalement
        guide["derniere"] = time.time()
        guide["etape"] += 1
        if guide["etape"] >= len(ETAPES[guide["services"][guide["i"]]]):
            guide["i"] += 1
            guide["etape"] = 0
            if guide["i"] >= len(guide["services"]):
                guide.clear()
                return "C'est tout ! Regarde la page Comptes : un point vert veut dire que c'est connecté."
        return dire_etape()

    reg.intercepteur(intercepter)
    reg.outil("aide_comptes", "Guide pas à pas l'utilisateur pour connecter ses comptes à Jarvis (Pronote, mails Gmail "
              "ou Hotmail, Discord) : ouvre la page Comptes de l'île et donne une étape à la fois.",
              {"service": {"type": "string", "enum": ["", "pronote", "mails", "discord"],
                           "description": "vide = tous ceux qui ne sont pas encore connectés"}},
              [], aide_comptes)
    reg.regle(r"^(?:aide moi a |comment (?:je |on )?(?:fais |fait )?(?:pour )?|je veux |j'aimerais |ou (?:est ce que )?"
              r"(?:je |on )?(?:dois |peux )?(?:me )?)(?:connecter|connecte|relier|lier|ajouter)(?: moi)?"
              r"(?: (?:mon |ma |mes |le |la |les |a |au |sur ))?(?P<s>.*?)(?: a jarvis| sur jarvis| dans jarvis)?$",
              lambda m, t: [("aide_comptes", {"service": m["s"]})]
              if not m["s"] or service_de(m["s"]) or re.search(r"\bcomptes?\b", m["s"]) else None, priorite=22)
