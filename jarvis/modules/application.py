"""« Ouvre l'application Jarvis » : l'île Givre se déplie en grand (tâches, rappels, habitudes, mémoire, musique,
Mon PC…). L'île lit et modifie les données de Jarvis par son API locale (/api/<nom>)."""
from . import Resultat, masquer_codes

SECTIONS = ["accueil", "taches", "rappels", "habitudes", "copies", "routines", "memoire", "musique", "appareils",
            "pc", "telephone", "pronote", "mails", "discord", "minuteurs", "parametres", "comptes", "historique"]
# ce qu'on dit -> section de la grande île
MOTS = {"taches": "taches", "rappels": "rappels", "habitudes": "habitudes", "copies": "copies",
        "presse papiers": "copies", "routines": "routines", "memoire": "memoire", "souvenirs": "memoire",
        "carte mentale": "memoire", "batteries": "appareils"}
NOMS = {"taches": "tes tâches", "rappels": "tes rappels", "habitudes": "tes habitudes", "copies": "tes copiés",
        "routines": "tes routines", "memoire": "ma mémoire", "appareils": "tes batteries",
         "comptes": "tes comptes", "historique": "l'historique de nos conversations"}


def telephone_affiche(tel):
    """Derniers SMS et appels pour l'accueil, codes de vérification cachés (l'écran peut être vu par d'autres)."""
    if not isinstance(tel, list):
        return None
    return [{**t, "texte": masquer_codes(t.get("texte", ""), "••••")[:90],
             "de": masquer_codes(t.get("de", ""), "••••") if t.get("type") == "appli" else t.get("de", "")}
            for t in tel[:5]]


def installer(ctx, reg):
    def ouvrir_application_jarvis(section: str = ""):
        section = section if section in SECTIONS else ""
        r = ctx.givre.ouvrir("grande", 0, section)
        if r and r.get("ok"):
            return Resultat(f"J'ouvre {NOMS.get(section, 'l’île en grand')}.")
        return Resultat("Il faut que la barre Givre soit lancée pour ça.", ok=False)

    def tableau(_methode, _p):
        """Résumé pour l'accueil : météo, cours, devoirs, mails, téléphone (venant de Givre) + état de Jarvis."""
        from core.mise_a_jour import version_locale
        g = ctx.givre.etat() or {}
        pronote = g.get("pronote") or {}
        mails = g.get("mails") if isinstance(g.get("mails"), list) else []
        return {"ok": True, "version": version_locale(), "givre": bool(g.get("ok")),
                "meteo": (g.get("meteo") or {}).get("actuelle"), "ville": (g.get("meteo") or {}).get("ville"),
                "phrase_meteo": (g.get("meteo") or {}).get("phrase"), "prochain_cours": pronote.get("prochain_cours"),
                "devoirs": [d for d in pronote.get("devoirs", []) if not d.get("fait")][:5], "mails": mails[:5],
                "telephone": telephone_affiche(g.get("telephone")),
                "jarvis": {"etat": ctx.etat.etat, "micro": ctx.etat.micro}}

    reg.api("tableau", tableau)
    reg.outil("ouvrir_application_jarvis", "Ouvre l'île de Jarvis en grand, en haut de l'écran (tâches, rappels, "
              "habitudes, mémoire, musique, batteries, Mon PC, paramètres), éventuellement sur une section.",
              {"section": {"type": "string", "enum": SECTIONS}}, [], ouvrir_application_jarvis)
    reg.regle(r"^(?:ouvre|affiche|montre)(?: moi)? (?:l'application|l'appli|l application|l appli|la fenetre|ta fenetre|"
              r"le tableau de bord|ton tableau de bord|l'ile en grand|la grande ile)(?: de)?(?: jarvis)?$",
              lambda m, t: [("ouvrir_application_jarvis", {})], priorite=12)
    reg.regle(r"^(?:ouvre|affiche|montre)(?: moi)? (?:mes |ma |mon |les |la |le |l'|ta |tes )?"
              r"(?P<s>taches|rappels|habitudes|copies|presse papiers|routines|memoire|souvenirs|carte mentale|batteries)$",
              lambda m, t: [("ouvrir_application_jarvis", {"section": MOTS[m["s"]]})], priorite=25)
