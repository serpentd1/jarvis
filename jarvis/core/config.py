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
                "givre_exe": "", "givre_arguments": ""},
    "micro": {"appareil": "", "seuil_activation": 0.5, "silence_fin": 1.4, "duree_max": 20,
              "attente_debut": 6, "conversation": 5, "conversation_continue": True, "fin_conversation": 60,
              "volume_conversation": 350, "ding": True, "interrompre": True, "couper_en_parlant": True},
    "transcription": {"modele": "parakeet", "appareil": "auto", "precision": "float16"},
    "voix": {"voix": "siwis_hd", "modele": "", "lenteur": 1.0, "volume": 1.0},
    "cerveau": {"ollama_url": "http://127.0.0.1:11434", "modele": "qwen3.5:4b", "modele_rapide": "", "historique": 3,
                "contexte": 8192, "garder_en_memoire": "30m", "temperature": 0.4},
    "givre": {"url": "http://127.0.0.1:47802"},          # la barre de Jarvis (JarvisBarre.exe), pas Givre
    "serveur": {"port": 47801},
    "annonces": {"actif": True, "pronote": True, "mail": True, "discord": True, "minuteur": True,
                 "telephone": True, "telephone_applis": False, "ne_pas_deranger": "", "lire_reponses_ile": False},
    "discord": {"confirmer_avant_envoi": True, "memoire": 30},
    "meteo": {"ville": ""},
    "deezer": {"utilisateur": ""},
    "mise_a_jour": {"auto": True, "depot": "", "verifier_toutes_les_heures": 6},
    "applis": {},
    "sites": {},
}


def ecrire_valeur(section: str, cle: str, valeur):
    """Change une valeur dans config.toml (texte, nombre ou oui/non) en gardant les commentaires."""
    import re
    lignes = FICHIER_CONFIG.read_text(encoding="utf-8").splitlines()
    if isinstance(valeur, bool):
        litteral = "true" if valeur else "false"
    elif isinstance(valeur, (int, float)):
        litteral = repr(valeur)
    else:
        litteral = '"' + str(valeur).replace("\\", "\\\\").replace('"', '\\"') + '"'
    nouvelle = f"{cle} = {litteral}"
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
    # Pas encore de réglages (dépôt tout juste copié) : on part des réglages par défaut, publiés sur GitHub.
    # config.toml, lui, reste sur ce PC (il contient tes réglages perso : contacts, ville…).
    defaut = RACINE / "config.defaut.toml"
    if not FICHIER_CONFIG.exists() and defaut.exists():
        try:
            FICHIER_CONFIG.write_bytes(defaut.read_bytes())
        except OSError:
            pass
    if FICHIER_CONFIG.exists():
        with open(FICHIER_CONFIG, "rb") as f:
            lu = tomllib.load(f)
    return {section: {**valeurs, **lu.get(section, {})} for section, valeurs in DEFAUTS.items()}
