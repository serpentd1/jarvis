"""« Qu'est-ce que tu sais faire ? » : la liste VRAIE de ce que Jarvis sait faire, tirée des modules chargés
(au lieu de laisser l'IA inventer)."""
from . import Resultat

# module -> ce qu'il permet, dit à l'oral (dans l'ordre où on le dit)
PHRASES = {
    "applis": "ouvrir et fermer tes applis",
    "web": "ouvrir des sites, mettre une vidéo YouTube et gérer tes onglets Chrome",
    "internet": "chercher sur Internet et lire une page",
    "givre_infos": "te dire tes devoirs, tes notes, tes cours, tes mails et la météo",
    "minuteurs": "lancer des minuteurs, l'alarme et le chrono",
    "medias": "contrôler la musique, les vidéos et le volume, même appli par appli",
    "deezer": "lancer de la musique sur Deezer",
    "discord": "lire et envoyer des messages Discord",
    "lecons": "te faire réviser tes leçons",
    "concentration": "lancer un mode concentration",
    "fichiers": "retrouver tes fichiers",
    "taches": "gérer ta liste de tâches",
    "rappels": "te faire des rappels",
    "habitudes": "suivre tes habitudes",
    "routines": "lancer tes routines",
    "memoire": "retenir des choses sur toi",
    "ecran": "regarder ton écran",
    "fenetres": "ranger tes fenêtres",
    "clavier": "taper du texte pour toi",
    "pc": "verrouiller, mettre en veille ou éteindre le PC",
    "telephone": "faire sonner ton téléphone",
    "mails": "résumer un mail",
    "personnalite": "changer ma façon de parler",
    "comptes": "t'aider à connecter Pronote, tes mails et Discord",
    "historique": "garder l'historique de nos conversations",
    "voix": "changer ma voix",
}


def modules_actifs(reg) -> list[str]:
    """Noms des modules qui ont au moins un outil chargé (modules.web -> web)."""
    vus = []
    for o in reg.outils.values():
        nom = getattr(o.fonction, "__module__", "").rsplit(".", 1)[-1]
        if nom and nom not in vus:
            vus.append(nom)
    return vus


def phrase_capacites(reg) -> str:
    actifs = set(modules_actifs(reg))
    morceaux = [p for nom, p in PHRASES.items() if nom in actifs]
    if not morceaux:
        return "Pour l'instant, je sais surtout discuter avec toi."
    if len(morceaux) > 1:
        morceaux[-1] = "et " + morceaux[-1]
    return ("Je sais " + ", ".join(morceaux) + ". Je peux aussi discuter, t'expliquer un cours et répondre à tes "
            "questions. Dis-moi ce que tu veux essayer !")


def installer(ctx, reg):
    def ce_que_je_sais_faire():
        return Resultat(phrase_capacites(reg))

    reg.outil("ce_que_je_sais_faire", "Dit la liste réelle de ce que Jarvis sait faire (quand on lui demande ce qu'il "
              "sait faire, ses fonctions, ce qu'il peut faire).", {}, [], ce_que_je_sais_faire)
    reg.regle(r"^(?:(?:et )?(?:qu'?\s?est ce que|qu est ce que) tu (?:sais|peux|peut|sait) faire"
              r"|(?:que|qu') (?:sais|peux) tu faire|tu (?:sais|peux) faire quoi|tu sers a quoi"
              r"|(?:c'est )?quoi tes (?:fonctions|capacites|pouvoirs)|(?:quelles sont )?tes (?:fonctions|capacites)"
              r"|(?:dis|montre) moi (?:tout )?ce que tu (?:sais|peux) faire)(?: jarvis)?$",
              lambda m, t: [("ce_que_je_sais_faire", {})], priorite=19)
