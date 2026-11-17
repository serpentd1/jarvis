"""S'assure qu'Ollama (le moteur d'IA) tourne, et le lance sinon."""
import logging
import os
import shutil
import subprocess
import time
from pathlib import Path

import requests

log = logging.getLogger("ollama")
SANS_FENETRE = 0x08000000


def repond(url: str) -> bool:
    try:
        return requests.get(f"{url.rstrip('/')}/api/version", timeout=2).ok
    except Exception:  # noqa: BLE001
        return False


def discuter(cfg: dict, messages: list, temperature: float = 0.3, max_mots: int = 300, json_seulement: bool = False):
    """Une question simple à l'IA, sans outils (résumés, explications). Renvoie le texte, ou None."""
    import re
    corps = {"model": cfg["modele"], "messages": messages, "stream": False, "think": False,
             "keep_alive": cfg["garder_en_memoire"],
             "options": {"temperature": temperature, "num_ctx": int(cfg["contexte"]), "num_predict": max_mots}}
    if json_seulement:
        corps["format"] = "json"
    try:
        r = requests.post(f"{cfg['ollama_url'].rstrip('/')}/api/chat", json=corps, timeout=120)
        r.raise_for_status()
        texte = r.json().get("message", {}).get("content", "")
        return re.sub(r"<think>.*?</think>", "", texte, flags=re.S).strip() or None
    except Exception:  # noqa: BLE001
        log.exception("Ollama (discuter)")
        return None


def assurer_lance(url: str, attente: float = 20) -> bool:
    if repond(url):
        return True
    appli = Path(os.environ.get("LOCALAPPDATA", "")) / "Programs" / "Ollama" / "ollama app.exe"
    if appli.is_file():
        log.info("Ollama ne répond pas : je le lance (%s)", appli)
        subprocess.Popen([str(appli)], creationflags=SANS_FENETRE)
    elif exe := shutil.which("ollama"):
        log.info("Ollama ne répond pas : je lance « ollama serve »")
        subprocess.Popen([exe, "serve"], creationflags=SANS_FENETRE,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        log.error("Ollama n'est pas installé : https://ollama.com/download")
        return False
    fin = time.time() + attente
    while time.time() < fin:
        time.sleep(1)
        if repond(url):
            return True
    return False
