"""Changer la façon de parler de Jarvis à la voix : « sois plus court », « parle comme un pirate »,
« redeviens normal ». Les consignes sont écrites dans prompt-systeme.txt (relu à chaque question), sous une ligne
« Ajouté à la voix » ; prompt-systeme.defaut.txt reste intact pour revenir en arrière."""
import re
import shutil

from core import config
from . import Resultat

MARQUE = "# ── Ajouté à la voix (« redeviens normal » pour tout enlever) ──"


def _lire() -> list[str]:
    config.prompt_systeme()                       # crée prompt-systeme.txt depuis le défaut s'il manque
    try:
        return config.PROMPT.read_text(encoding="utf-8").splitlines()
    except OSError:
        return []


def consignes_ajoutees(lignes: list[str] | None = None) -> list[str]:
    lignes = _lire() if lignes is None else lignes
    if MARQUE not in lignes:
        return []
    return [l.removeprefix("- ").strip() for l in lignes[lignes.index(MARQUE) + 1:] if l.strip() and not l.startswith("#")]


def ajouter(consigne: str) -> list[str]:
    lignes = _lire()
    if MARQUE not in lignes:
        lignes += ["", MARQUE]
    lignes.append(f"- {consigne}")
    config.PROMPT.write_text("\n".join(lignes) + "\n", encoding="utf-8")
    return consignes_ajoutees(lignes)


def retirer_derniere() -> str:
    lignes = _lire()
    if not consignes_ajoutees(lignes):
        return ""
    for i in range(len(lignes) - 1, lignes.index(MARQUE), -1):
        if lignes[i].strip() and not lignes[i].startswith("#"):
            enlevee = lignes.pop(i).removeprefix("- ").strip()
            config.PROMPT.write_text("\n".join(lignes) + "\n", encoding="utf-8")
            return enlevee
    return ""


def par_defaut() -> bool:
    """Remet le prompt de base (l'ancien est gardé dans prompt-systeme.ancien.txt)."""
    if not config.PROMPT_DEFAUT.is_file():
        return False
    if config.PROMPT.is_file():
        shutil.copyfile(config.PROMPT, config.PROMPT.with_name("prompt-systeme.ancien.txt"))
    shutil.copyfile(config.PROMPT_DEFAUT, config.PROMPT)
    return True


def nettoyer_consigne(texte: str) -> str:
    """« Jarvis, à partir de maintenant sois plus court ! » -> « Sois plus court. »"""
    t = re.sub(r"^\s*(?:(?:hey|ok|dis)\s+)?jarvis\W*", "", texte or "", flags=re.I).strip()
    t = re.sub(r"^(?:à|a) partir de maintenant,?\s*|^désormais,?\s*|^dorénavant,?\s*|^maintenant,?\s*", "", t, flags=re.I)
    t = t.strip(" .!?,")
    return (t[:1].upper() + t[1:] + ".") if t else ""


def installer(ctx, reg):
    simule = getattr(ctx, "simulation", False)

    def personnalite(action: str, consigne: str = ""):
        if action == "ajouter":
            consigne = nettoyer_consigne(consigne)
            if not consigne:
                return Resultat("Dis-moi comment tu veux que je parle.", ok=False)
            if not simule:
                ajouter(consigne)
            return Resultat(f"D'accord, je retiens : {consigne[:-1].lower()}. Dis « redeviens normal » pour revenir "
                            "comme avant.")
        if action == "retirer_derniere":
            enlevee = "" if simule else retirer_derniere()
            return Resultat(f"J'ai enlevé : {enlevee}" if enlevee else "Tu ne m'as rien demandé de changer.")
        if action == "defaut":
            if not simule and not par_defaut():
                return Resultat("Je ne trouve pas mon prompt de base.", ok=False)
            return Resultat("C'est fait, je reparle normalement.")
        ajouts = [] if simule else consignes_ajoutees()
        texte = config.prompt_systeme()
        return Resultat(("Ce que tu m'as demandé : " + " ".join(ajouts)) if ajouts else
                        "Je parle avec ma personnalité de base : tu ne m'as rien demandé de changer.",
                        pour_ia=f"Ton prompt système actuel (fichier prompt-systeme.txt) :\n{texte[:3000]}\n"
                                "Résume-le à l'oral en deux ou trois phrases, et dis qu'on peut le changer à la voix "
                                "(« sois plus court », « parle comme un pirate », « redeviens normal »).")

    reg.outil("personnalite", "Change la façon de parler de Jarvis (son prompt système), à la demande : ajouter une "
              "consigne (« sois plus court », « parle comme un pirate »), enlever la dernière, revenir au prompt de "
              "base, ou dire le prompt actuel.",
              {"action": {"type": "string", "enum": ["ajouter", "retirer_derniere", "defaut", "lire"]},
               "consigne": "la consigne à ajouter, telle que dite (ex : sois plus court)"},
              ["action"], personnalite)

    reg.regle(r"^(?:(?:a partir de maintenant|desormais|dorenavant|maintenant) )?(?:sois|soit|soyez) (?:plus|moins|un peu plus|un peu moins) .+$"
              r"|^(?:(?:a partir de maintenant|desormais|dorenavant|maintenant) )?(?:parle|parles|reponds|reponde) "
              r"(?:moi )?(?:comme|en (?!plus fort|moins fort)|plus (?:court|bref|simplement|serieusement)|moins long|toujours|avec).*$"
              r"|^(?:a partir de maintenant|desormais|dorenavant) .+$",
              lambda m, t: [("personnalite", {"action": "ajouter", "consigne": t})], priorite=21)
    reg.regle(r"^(?:redeviens normal|reparle normalement|parle normalement|sois normal|arrete de parler comme .+"
              r"|(?:remets?|reviens a|retourne a) (?:ton |le )?(?:prompt|comportement|personnalite)(?: systeme)?"
              r"(?: de base| par defaut| normal| d'origine)?|reinitialise (?:ton |le )?prompt(?: systeme)?)$",
              lambda m, t: [("personnalite", {"action": "defaut"})], priorite=21)
    reg.regle(r"^(?:enleve|oublie|annule) (?:ta |la )?derniere (?:consigne|regle|modification)$",
              lambda m, t: [("personnalite", {"action": "retirer_derniere"})], priorite=21)
    reg.regle(r"^(?:quel est|c'est quoi|dis moi|montre moi|lis moi) (?:ton|le) prompt(?: (?:de )?systeme)?$",
              lambda m, t: [("personnalite", {"action": "lire"})], priorite=21)
