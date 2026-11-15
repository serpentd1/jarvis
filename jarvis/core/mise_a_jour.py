"""Mise à jour automatique : Jarvis regarde les « Releases » du dépôt GitHub (config.toml, [mise_a_jour]).
Si une version plus récente existe, il télécharge Installer-Jarvis.exe et le lance en mode automatique :
l'installeur arrête Jarvis et Givre, remplace les fichiers (tes réglages sont gardés), puis relance Jarvis.
Seulement sur une copie INSTALLÉE (pas dans le dossier de développement)."""
import logging
import os
import subprocess
import tempfile
import threading
import time

import requests

from .config import RACINE

log = logging.getLogger("mise_a_jour")
DETACHE = 0x00000008 | 0x00000200


def version_locale() -> str:
    try:
        return (RACINE / "VERSION").read_text(encoding="utf-8").strip()
    except OSError:
        return "0.0.0"


def en_nombres(version: str) -> tuple:
    morceaux = []
    for x in version.strip().lstrip("vV").split("."):
        chiffres = "".join(c for c in x if c.isdigit())
        morceaux.append(int(chiffres) if chiffres else 0)
    return tuple((morceaux + [0, 0, 0])[:3])


def derniere_version(depot: str):
    """{"version", "url"} de la dernière Release GitHub, ou None."""
    r = requests.get(f"https://api.github.com/repos/{depot}/releases/latest", timeout=10,
                     headers={"Accept": "application/vnd.github+json", "User-Agent": "Jarvis-mise-a-jour"})
    if r.status_code != 200:
        return None
    j = r.json()
    url = next((a["browser_download_url"] for a in j.get("assets", []) if a.get("name") == "Installer-Jarvis.exe"), None)
    return {"version": j.get("tag_name", "").lstrip("vV"), "url": url} if url else None


def appliquer(info: dict, etat):
    """Télécharge l'installeur de la nouvelle version et le lance (il arrêtera puis relancera Jarvis)."""
    cible = os.path.join(tempfile.gettempdir(), f"Installer-Jarvis-{info['version']}.exe")
    with requests.get(info["url"], stream=True, timeout=60) as r:
        r.raise_for_status()
        with open(cible + ".part", "wb") as f:
            for morceau in r.iter_content(1 << 20):
                f.write(morceau)
    os.replace(cible + ".part", cible)
    etat.annoncer(f"Une nouvelle version de Jarvis est arrivée, la {info['version']}. Je me mets à jour, "
                  "je reviens dans une minute.")
    time.sleep(8)                                  # le temps de le dire
    log.info("Mise à jour vers %s", info["version"])
    subprocess.Popen([cible, "--auto", "--dossier", str(RACINE), "--sans-raccourcis", "--relancer"],
                     creationflags=DETACHE, close_fds=True)


def demarrer(cfg: dict, etat):
    m = cfg.get("mise_a_jour", {})
    depot = str(m.get("depot", "")).strip()
    if not depot:                                  # le fichier DEPOT est fourni (et remplacé) par l'installeur
        try:
            depot = (RACINE / "DEPOT").read_text(encoding="utf-8").strip()
        except OSError:
            depot = ""
    if not m.get("auto", True) or not depot or not (RACINE / "installation.log").exists():
        return                                     # pas configuré, ou dossier de développement

    def boucle():
        time.sleep(60)                             # on laisse Jarvis démarrer tranquillement
        while not etat.quitter.is_set():
            try:
                info = derniere_version(depot)
                if info and en_nombres(info["version"]) > en_nombres(version_locale()):
                    appliquer(info, etat)
                    return
            except Exception:  # noqa: BLE001
                log.warning("vérification de mise à jour impossible")
            etat.quitter.wait(float(m.get("verifier_toutes_les_heures", 6)) * 3600)

    threading.Thread(target=boucle, daemon=True, name="mise-a-jour").start()
