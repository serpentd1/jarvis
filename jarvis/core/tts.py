"""Texte -> voix, en local. Plusieurs voix au choix (menu de l'icône, « change de voix », config.toml).

- Kokoro : voix plus naturelles, utilise le GPU CUDA quand disponible (sinon processeur).
- Piper  : voix rapides et légères.
Une voix manquante est téléchargée la première fois qu'on la choisit.
"""
import logging
import re
import subprocess
import sys
import threading

import numpy as np
import requests
import sounddevice as sd
from scipy.signal import resample_poly

from . import config

log = logging.getLogger("voix")

CATALOGUE = {
    "siwis_hd": {"moteur": "kokoro", "voix": "ff_siwis", "titre": "Siwis HD (femme, la plus naturelle)"},
    "jarvis": {"moteur": "kokoro", "voix": "bm_george", "titre": "Jarvis (homme, accent anglais, expérimental)"},
    "tom": {"moteur": "piper", "modele": "fr_FR-tom-medium", "titre": "Tom (homme)"},
    "pierre": {"moteur": "piper", "modele": "fr_FR-upmc-medium", "locuteur": "pierre", "titre": "Pierre (homme)"},
    "jessica": {"moteur": "piper", "modele": "fr_FR-upmc-medium", "locuteur": "jessica", "titre": "Jessica (femme)"},
    "siwis": {"moteur": "piper", "modele": "fr_FR-siwis-medium", "titre": "Siwis (femme, rapide)"},
}
KOKORO_URL = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/"
KOKORO_FICHIERS = ("kokoro-v1.0.onnx", "voices-v1.0.bin")
DOSSIER_KOKORO = config.MODELES / "kokoro"
DOSSIER_PIPER = config.MODELES / "piper"


def nom_depuis_config(cfg: dict) -> str:
    """Le nom de la voix choisie (accepte aussi l'ancien réglage « modele = fr_FR-… »)."""
    nom = str(cfg.get("voix") or "").strip().lower()
    if nom in CATALOGUE:
        return nom
    ancien = str(cfg.get("modele") or "")
    return next((n for n, v in CATALOGUE.items() if v.get("modele") == ancien and "locuteur" not in v), "siwis_hd")


def est_telechargee(nom: str) -> bool:
    v = CATALOGUE[nom]
    if v["moteur"] == "kokoro":
        return all((DOSSIER_KOKORO / f).is_file() for f in KOKORO_FICHIERS)
    return (DOSSIER_PIPER / f"{v['modele']}.onnx").is_file()


def telecharger(nom: str):
    """Télécharge la voix si besoin (Kokoro ≈ 340 Mo la première fois, Piper ≈ 60 Mo)."""
    if est_telechargee(nom):
        return
    v = CATALOGUE[nom]
    if v["moteur"] == "kokoro":
        DOSSIER_KOKORO.mkdir(parents=True, exist_ok=True)
        for fichier in KOKORO_FICHIERS:
            cible = DOSSIER_KOKORO / fichier
            if cible.is_file():
                continue
            log.info("Téléchargement de %s…", fichier)
            temporaire = cible.with_suffix(cible.suffix + ".part")
            with requests.get(KOKORO_URL + fichier, stream=True, timeout=60) as r:
                r.raise_for_status()
                with open(temporaire, "wb") as f:
                    for morceau in r.iter_content(1 << 20):
                        f.write(morceau)
            temporaire.replace(cible)
    else:
        DOSSIER_PIPER.mkdir(parents=True, exist_ok=True)
        log.info("Téléchargement de la voix %s…", v["modele"])
        subprocess.run([sys.executable, "-m", "piper.download_voices", v["modele"], "--download-dir", str(DOSSIER_PIPER)],
                       check=True, capture_output=True, creationflags=0x08000000)


class Voix:
    _kokoro = None                      # le modèle Kokoro est partagé entre ses voix

    def __init__(self, cfg: dict):
        self.cfg = cfg
        self.verrou = threading.Lock()
        self.nom = None
        self.changer(nom_depuis_config(cfg))

    def changer(self, nom: str):
        """Charge une autre voix (la télécharge si besoin)."""
        if nom not in CATALOGUE:
            raise ValueError(f"voix inconnue : {nom}")
        telecharger(nom)
        v = CATALOGUE[nom]
        with self.verrou:
            if v["moteur"] == "kokoro":
                if Voix._kokoro is None:
                    from . import cuda
                    cuda.preparer()
                    import onnxruntime as ort
                    from kokoro_onnx import Kokoro
                    modele, voix = map(str, (DOSSIER_KOKORO / KOKORO_FICHIERS[0],
                                             DOSSIER_KOKORO / KOKORO_FICHIERS[1]))
                    fournisseurs = ort.get_available_providers()
                    session = None
                    if "CUDAExecutionProvider" in fournisseurs:
                        try:
                            session = ort.InferenceSession(
                                modele, providers=["CUDAExecutionProvider", "CPUExecutionProvider"])
                        except Exception as e:  # noqa: BLE001 — CUDA ONNX peut ne pas avoir toutes ses DLL
                            log.warning("GPU indisponible pour Kokoro (%s) ; repli sur le processeur.", e)
                    if session is not None and "CUDAExecutionProvider" not in session.get_providers():
                        log.warning("Kokoro n'a pas activé CUDA ; repli sur le processeur.")
                    if session is None or "CUDAExecutionProvider" not in session.get_providers():
                        session = ort.InferenceSession(modele, providers=["CPUExecutionProvider"])
                    Voix._kokoro = Kokoro.from_session(session, voix)
                    log.info("Moteur ONNX pour Kokoro : %s", ", ".join(session.get_providers()))
                self.moteur = ("kokoro", v["voix"])
            else:
                from piper import PiperVoice, SynthesisConfig
                piper = PiperVoice.load(str(DOSSIER_PIPER / f"{v['modele']}.onnx"))
                locuteur = None
                if v.get("locuteur"):
                    locuteur = (piper.config.speaker_id_map or {}).get(v["locuteur"])
                reglages = SynthesisConfig(length_scale=float(self.cfg["lenteur"]), volume=float(self.cfg["volume"]),
                                           speaker_id=locuteur)
                self.moteur = ("piper", piper, reglages)
            self.nom = nom
        log.info("Voix : %s", CATALOGUE[nom]["titre"])

    def synthetiser(self, texte: str) -> tuple[np.ndarray, int]:
        """Renvoie (audio float32, fréquence)."""
        texte = nettoyer(texte) or "…"
        with self.verrou:
            if self.moteur[0] == "kokoro":
                vitesse = 1.0 / max(0.5, float(self.cfg["lenteur"]))
                audio, frequence = Voix._kokoro.create(texte, voice=self.moteur[1], speed=vitesse, lang="fr-fr")
                return (np.asarray(audio, dtype=np.float32) * float(self.cfg["volume"])), frequence
            _, piper, reglages = self.moteur
            morceaux, frequence = [], 22050
            for chunk in piper.synthesize(texte, syn_config=reglages):
                morceaux.append(chunk.audio_float_array)
                frequence = chunk.sample_rate
        if not morceaux:
            return np.zeros(1, dtype=np.float32), frequence
        return np.concatenate(morceaux).astype(np.float32), frequence

    def dire(self, texte: str):
        audio, frequence = self.synthetiser(texte)
        sd.play(audio, frequence)
        sd.wait()

    def audio_16k(self, texte: str) -> np.ndarray:
        """La phrase dite par la voix, convertie en 16 kHz (pour les tests sans micro)."""
        audio, frequence = self.synthetiser(texte)
        return resample_poly(audio, 16000, frequence).astype(np.float32)


def nettoyer(texte: str) -> str:
    """Retire ce qui ne se prononce pas (markdown, emojis, liens)."""
    texte = re.sub(r"https?://\S+", "le lien", texte or "")
    texte = re.sub(r"[*_#`>|~]", "", texte)
    texte = re.sub(r"[\U0001F000-\U0001FFFF☀-➿]", "", texte)
    return re.sub(r"\s+", " ", texte).strip()
