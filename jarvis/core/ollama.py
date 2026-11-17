"""Le lien avec l'IA. Jarvis ne parle plus directement à Ollama : il demande à la barre (JarvisBarre.exe,
127.0.0.1:47802, route POST /ia/discuter). La barre choisit l'IA : LM Studio sur le PC (gratuit, proposé en premier),
ou une clé d'API collée dans ses réglages (gratuite d'abord, puis payante).
Si la barre ne répond pas, Jarvis retombe sur Ollama directement (ancien comportement, en secours).
"""
import logging
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

import requests

from . import config

log = logging.getLogger("ollama")
SANS_FENETRE = 0x08000000
BARRE_DELAI = 130          # une réponse peut prendre du temps (modèle lent, réseau)
URL_BARRE_DEFAUT = "http://127.0.0.1:47802"


def url_barre() -> str:
    """L'adresse de la barre, lue dans config.toml, section [givre], clé url."""
    try:
        return str(config.charger()["givre"]["url"]).rstrip("/")
    except Exception:  # noqa: BLE001
        return URL_BARRE_DEFAUT


def barre_repond() -> bool:
    try:
        return requests.get(f"{url_barre()}/ia/etat", timeout=2).ok
    except Exception:  # noqa: BLE001
        return False


def repond(url: str) -> bool:
    try:
        return requests.get(f"{url.rstrip('/')}/api/version", timeout=2).ok
    except Exception:  # noqa: BLE001
        return False


def discuter(cfg: dict, messages: list, temperature: float = 0.3, max_mots: int = 300, json_seulement: bool = False):
    """Une question simple à l'IA, sans outils (résumés, explications, lecture d'image). Renvoie le texte, ou None.

    Passe par la barre (POST /ia/discuter). Les images vont dans le message : {"images": [base64]}.
    `cfg` (la section [cerveau]) ne sert qu'au secours Ollama, si la barre ne répond pas.
    """
    try:
        r = requests.post(f"{url_barre()}/ia/discuter", timeout=BARRE_DELAI,
                          json={"messages": messages, "temperature": temperature, "max_mots": max_mots,
                                "json": json_seulement, "outils": []})
    except requests.ConnectionError:
        log.info("La barre ne répond pas : j'interroge Ollama directement.")
        return _discuter_ollama(cfg, messages, temperature, max_mots, json_seulement)
    except requests.RequestException:
        log.exception("La barre (discuter) n'a pas répondu à temps")
        return None
    if r.status_code == 404:
        log.warning("La barre ne connaît pas encore /ia/discuter (version trop ancienne ?) : Ollama en secours.")
        return _discuter_ollama(cfg, messages, temperature, max_mots, json_seulement)
    try:
        j = r.json()
    except ValueError:
        log.error("La barre a renvoyé une réponse illisible.")
        return None
    if j.get("ok"):
        texte = str(j.get("texte") or "")
        return re.sub(r"<think>.*?</think>", "", texte, flags=re.S).strip() or None
    log.warning("IA (barre) : %s", j.get("erreur", "pas de réponse"))
    return None


def _discuter_ollama(cfg: dict, messages: list, temperature: float, max_mots: int, json_seulement: bool):
    """Secours : Ollama directement (ancien comportement)."""
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
