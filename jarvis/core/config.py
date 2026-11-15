"""Lecture de config.toml et chemins du projet."""
import os
import tomllib
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
MODELES = RACINE / "modeles"
FICHIER_CONFIG = RACINE / "config.toml"
DONNEES = Path(os.environ.get("APPDATA", RACINE)) / "Jarvis"

DEFAUTS = {
    "general": {"nom": "Jarvis", "phrase_demarrage": "Jarvis est prêt.", "lancer_givre": True,
                "givre_exe": "", "givre_arguments": "--ile"},
    "micro": {"appareil": "", "seuil_activation": 0.5, "silence_fin": 1.4, "duree_max": 20,
              "attente_debut": 6, "conversation": 5, "ding": True, "interrompre": True},
    "transcription": {"modele": "large-v3-turbo", "appareil": "auto", "precision": "float16"},
    "voix": {"voix": "siwis_hd", "modele": "", "lenteur": 1.0, "volume": 1.0},
    "cerveau": {"ollama_url": "http://127.0.0.1:11434", "modele": "qwen3.5:4b", "historique": 3,
                "contexte": 8192, "garder_en_memoire": "30m", "temperature": 0.4},
    "givre": {"url": "http://127.0.0.1:47800"},
    "serveur": {"port": 47801},
    "annonces": {"actif": True, "pronote": True, "mail": True, "discord": True, "minuteur": True,
                 "ne_pas_deranger": "", "lire_reponses_ile": False},
    "discord": {"confirmer_avant_envoi": True, "memoire": 30},
    "meteo": {"ville": ""},
    "deezer": {"utilisateur": ""},
    "mise_a_jour": {"auto": True, "depot": "", "verifier_toutes_les_heures": 6},
    "applis": {},
    "sites": {},
}


def ecrire_valeur(section: str, cle: str, valeur: str):
    """Change une valeur texte dans config.toml en gardant les commentaires (ex : la voix choisie)."""
    import re
    lignes = FICHIER_CONFIG.read_text(encoding="utf-8").splitlines()
    nouvelle = f'{cle} = "{valeur}"'
    debut = fin = None
    for i, ligne in enumerate(lignes):
        s = ligne.strip()
        if s.startswith("["):
            if debut is not None and fin is None:
                fin = i
            if s == f"[{section}]":
                debut = i
    if debut is None:
        lignes += ["", f"[{section}]", nouvelle]
    else:
        for i in range(debut + 1, fin if fin is not None else len(lignes)):
            if re.match(rf"{re.escape(cle)}\s*=", lignes[i].strip()):
                lignes[i] = nouvelle
                break
        else:
            lignes.insert(debut + 1, nouvelle)
    FICHIER_CONFIG.write_text("\n".join(lignes) + "\n", encoding="utf-8")


def charger() -> dict:
    """Renvoie la config : les valeurs de config.toml, complétées par les défauts."""
    lu = {}
    if FICHIER_CONFIG.exists():
        with open(FICHIER_CONFIG, "rb") as f:
            lu = tomllib.load(f)
    return {section: {**valeurs, **lu.get(section, {})} for section, valeurs in DEFAUTS.items()}
