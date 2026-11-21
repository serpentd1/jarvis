"""Mesure la vitesse de quelques modèles NVIDIA (build.nvidia.com) et garde le plus rapide.

    python outils/mesurer_modeles.py            -> mesure et affiche
    python outils/mesurer_modeles.py --ecrire   -> écrit aussi le gagnant dans config.toml ([conversation] modele)

Pour chaque modèle : temps avant le premier mot (3 essais, on garde la médiane), puis deux vérifications :
il sait appeler un outil (function calling) et il accepte une image (vision).
La clé : variable d'environnement NVIDIA_API_KEY, sinon celle enregistrée dans les Paramètres de la barre (ia.bin).
"""
import base64
import ctypes
import ctypes.wintypes as wt
import json
import os
import statistics
import sys
import time
from pathlib import Path

import requests

URL = "https://integrate.api.nvidia.com/v1"
# Rapides, sans raisonnement (ou raisonnement coupé), avec outils et vision. Ceux absents du catalogue sont sautés.
CANDIDATS = [
    "nvidia/nemotron-3.5-lightning-30b-a3b",
    "nvidia/nemotron-3.5-bolt-e4b",
    "nvidia/nemotron-3.5-super-vl-120b-a12b",
    "nvidia/nemotron-3-ultra-550b-a55b",
    "mistralai/mistral-large-4-1t-a52b",
    "google/gemma-4-31b-it",
    "google/gemma-3-12b-it",
    "meta/muse-glimmer-30b",
    "minimaxai/minimax-m3.1-flash",
    "z-ai/glm-5.3-flash",
]
QUESTION = [{"role": "system", "content": "Tu es Jarvis. Réponds en français, en une phrase courte."},
            {"role": "user", "content": "Salut, ça va ? Dis-moi une chose sympa."}]
OUTIL = [{"type": "function", "function": {
    "name": "web_search", "description": "Cherche sur Internet.",
    "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}}}]
def image_rouge() -> str:
    """Un carré rouge en PNG (base64) : assez pour savoir si le modèle accepte les images."""
    import io
    from PIL import Image
    tampon = io.BytesIO()
    Image.new("RGB", (64, 64), (220, 20, 20)).save(tampon, "PNG")
    return base64.b64encode(tampon.getvalue()).decode()


def cle() -> str:
    if os.environ.get("NVIDIA_API_KEY"):
        return os.environ["NVIDIA_API_KEY"].strip()
    fichier = Path(os.environ.get("APPDATA", "")) / "Jarvis" / "barre" / "ia.bin"
    if not fichier.is_file():
        return ""

    class Blob(ctypes.Structure):
        _fields_ = [("cbData", wt.DWORD), ("pbData", ctypes.POINTER(ctypes.c_char))]

    brut = fichier.read_bytes()
    entree, sortie = Blob(len(brut), ctypes.cast(ctypes.c_char_p(brut), ctypes.POINTER(ctypes.c_char))), Blob()
    if not ctypes.windll.crypt32.CryptUnprotectData(ctypes.byref(entree), None, None, None, None, 1, ctypes.byref(sortie)):
        return ""
    donnees = ctypes.string_at(sortie.pbData, sortie.cbData)
    ctypes.windll.kernel32.LocalFree(sortie.pbData)
    for f in json.loads(donnees).get("fournisseurs", []):
        if "nvidia.com" in f.get("url", "") and f.get("cle"):
            return f["cle"].strip()
    return ""


def premier_mot(session, modele: str) -> float | None:
    """Secondes entre l'envoi et le premier morceau de texte (réponse en direct)."""
    corps = {"model": modele, "messages": QUESTION, "stream": True, "max_tokens": 60, "temperature": 0.2,
             "reasoning_effort": "none"}
    for _ in range(2):
        debut = time.perf_counter()
        r = session.post(f"{URL}/chat/completions", json=corps, stream=True, timeout=20)
        if r.status_code in (400, 422) and "reasoning_effort" in corps:
            corps.pop("reasoning_effort")
            continue
        if r.status_code != 200:
            print(f"   erreur {r.status_code} : {r.text[:120]}")
            return None
        for ligne in r.iter_lines():
            if not ligne.startswith(b"data:") or ligne.strip() == b"data: [DONE]":
                continue
            delta = json.loads(ligne[5:])["choices"][0]["delta"]
            if delta.get("content"):
                t = time.perf_counter() - debut
                r.close()
                return t
        return None
    return None


def sait_outils(session, modele: str) -> bool:
    corps = {"model": modele, "max_tokens": 80, "tools": OUTIL,
             "messages": [{"role": "user", "content": "Cherche sur Internet la météo à Paris."}]}
    r = session.post(f"{URL}/chat/completions", json=corps, timeout=20)
    return r.ok and bool(r.json()["choices"][0]["message"].get("tool_calls"))


def sait_voir(session, modele: str) -> bool:
    corps = {"model": modele, "max_tokens": 20, "messages": [{"role": "user", "content": [
        {"type": "text", "text": "De quelle couleur est cette image ? Un mot."},
        {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{image_rouge()}"}}]}]}
    r = session.post(f"{URL}/chat/completions", json=corps, timeout=20)
    return r.ok and any(c in (r.json()["choices"][0]["message"].get("content") or "").lower() for c in ("roug", "red"))


def ecrire(modele: str):
    from core.config import ecrire_valeur
    ecrire_valeur("conversation", "modele", modele)


def essayer(f, *args):
    """Une erreur réseau ou un modèle qui ne répond pas = échec, pas de plantage."""
    try:
        return f(*args)
    except Exception as e:  # noqa: BLE001
        print(f"   ({f.__name__} : {type(e).__name__})", flush=True)
        return None


def main():
    k = cle()
    if not k:
        print("Pas de clé NVIDIA : mets-la dans la variable NVIDIA_API_KEY, ou dans les Paramètres de la barre (onglet IA).")
        return 1
    s = requests.Session()
    s.headers["Authorization"] = f"Bearer {k}"
    try:
        dispo = {m["id"] for m in s.get(f"{URL}/models", timeout=15).json()["data"]}
    except Exception:  # noqa: BLE001
        dispo = set(CANDIDATS)
    resultats = []
    for modele in CANDIDATS:
        if modele not in dispo:
            print(f"-- {modele} : absent du catalogue")
            continue
        print(f"-- {modele}", flush=True)
        temps = []
        for _ in range(3):
            t = essayer(premier_mot, s, modele)
            if t is None:
                break
            temps.append(t)
        if not temps:
            continue
        mediane = statistics.median(temps)
        outils, vision = bool(essayer(sait_outils, s, modele)), bool(essayer(sait_voir, s, modele))
        print(f"   premier mot : {mediane * 1000:.0f} ms   outils : {'oui' if outils else 'non'}   vision : {'oui' if vision else 'non'}")
        resultats.append((mediane, modele, outils, vision))
    complets = sorted(r for r in resultats if r[2] and r[3])
    if not complets:
        print("\nAucun modèle n'a tout réussi (outils + vision).")
        return 1
    t, gagnant, *_ = complets[0]
    print(f"\nLe plus rapide avec outils et vision : {gagnant} ({t * 1000:.0f} ms)")
    if "--ecrire" in sys.argv:
        ecrire(gagnant)
        print("Écrit dans config.toml ([conversation] modele).")
    return 0


if __name__ == "__main__":
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    sys.exit(main())
