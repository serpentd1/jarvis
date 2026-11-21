"""Texte -> voix. Plusieurs voix au choix (menu de l'icône, « change de voix », config.toml).

- Edge   : voix neuronales de Microsoft (Denise, Henri…), en ligne, gratuites, aucune mémoire graphique.
           Le son arrive par morceaux : Jarvis commence à parler avant la fin de la phrase.
           Sans Internet, Jarvis prend tout seul une voix locale (Piper « siwis »).
- Kokoro : voix plus naturelles, utilise le GPU CUDA quand disponible (sinon processeur).
- Piper  : voix rapides et légères, hors ligne.
Une voix locale manquante est téléchargée la première fois qu'on la choisit.
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
    "denise": {"moteur": "edge", "voix": "fr-FR-DeniseNeural", "titre": "Denise (femme, en ligne)"},
    "henri": {"moteur": "edge", "voix": "fr-FR-HenriNeural", "titre": "Henri (homme, en ligne)"},
    "vivienne": {"moteur": "edge", "voix": "fr-FR-VivienneMultilingualNeural", "titre": "Vivienne (femme, en ligne)"},
    "remy": {"moteur": "edge", "voix": "fr-FR-RemyMultilingualNeural", "titre": "Rémy (homme, en ligne)"},
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
    edge = next((n for n, v in CATALOGUE.items() if v["moteur"] == "edge" and v["voix"].lower() == nom), None)
    if edge:
        return edge
    ancien = str(cfg.get("modele") or "")
    return next((n for n, v in CATALOGUE.items() if v.get("modele") == ancien and "locuteur" not in v), "siwis_hd")


def est_telechargee(nom: str) -> bool:
    v = CATALOGUE[nom]
    if v["moteur"] == "edge":
        return True
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
            if v["moteur"] == "edge":
                self.moteur = ("edge", v["voix"])
            elif v["moteur"] == "kokoro":
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

    def flux(self, texte: str):
        """Donne la voix par morceaux (audio float32, fréquence) dès qu'ils sont prêts."""
        if self.moteur[0] != "edge":
            yield self.synthetiser(texte)
            return
        propre = nettoyer(texte)
        if not re.search(r"\w", propre):
            return
        donne = False
        try:
            for morceau in _edge(propre, self.moteur[1], float(self.cfg["lenteur"]), float(self.cfg["volume"])):
                donne = True
                yield morceau
        except Exception as e:  # noqa: BLE001 — pas d'Internet, service indisponible…
            if donne:
                log.warning("voix en ligne coupée en route (%s)", e)
                return
            log.warning("voix en ligne indisponible (%s) : je prends la voix locale", e)
            yield self.secours().synthetiser(texte)

    def prechauffer(self):
        """Ouvre à l'avance la connexion de la voix en ligne (pendant que tu parles)."""
        if self.moteur[0] == "edge":
            from .edge_direct import direct
            direct().prechauffer()

    def secours(self) -> "Voix":
        """La voix locale (Piper « siwis ») quand la voix en ligne ne répond pas."""
        if getattr(self, "_secours", None) is None:
            self._secours = Voix({**self.cfg, "voix": "siwis"})
        return self._secours

    def synthetiser(self, texte: str) -> tuple[np.ndarray, int]:
        """Renvoie (audio float32, fréquence)."""
        if self.moteur[0] == "edge":
            morceaux = list(self.flux(texte))
            if not morceaux:
                return np.zeros(1, dtype=np.float32), 24000
            freq = morceaux[0][1]
            return np.concatenate([np.asarray(a, dtype=np.float32) for a, _ in morceaux]), freq
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

    def dire(self, texte: str, nom_sortie: str = ""):
        """Dit la phrase d'un coup (mode texte), sur la sortie de Jarvis (`[voix] sortie`, vide = Windows)."""
        audio, frequence = self.synthetiser(texte)
        if nom_sortie:
            from .sortie import jouer_simple
            jouer_simple(audio, frequence, nom_sortie)
            return
        sd.play(audio, frequence)
        sd.wait()

    def audio_16k(self, texte: str) -> np.ndarray:
        """La phrase dite par la voix, convertie en 16 kHz (pour les tests sans micro)."""
        audio, frequence = self.synthetiser(texte)
        return resample_poly(audio, 16000, frequence).astype(np.float32)


def _edge(texte: str, voix: str, lenteur: float, volume: float):
    """Voix Edge en direct : le MP3 arrive par petits paquets, on décode et on donne le son au fur et à mesure."""
    import miniaudio

    from .edge_direct import direct

    vitesse = f"{round((1.0 / max(0.5, lenteur) - 1.0) * 100):+d}%"
    paquets = direct().synthese(texte, voix, vitesse)
    mp3, donnes, decode_a, seuil = bytearray(), 0, 0, 2500     # le 1er morceau part très vite (~0,2 s de son)
    while True:
        p = next(paquets, None)
        fini = p is None
        if not fini:
            mp3 += p
        if fini or len(mp3) - decode_a >= seuil:
            decode_a, seuil = len(mp3), 9000
            if mp3:
                pcm = miniaudio.decode(bytes(mp3), output_format=miniaudio.SampleFormat.FLOAT32,
                                       nchannels=1, sample_rate=24000)
                son = np.frombuffer(pcm.samples, dtype=np.float32)
                garde = len(son) if fini else max(0, len(son) - 2304)   # la dernière trame MP3 peut être incomplète
                if garde > donnes:
                    yield son[donnes:garde] * volume, 24000
                    donnes = garde
        if fini:
            return


def nettoyer(texte: str) -> str:
    """Retire ce qui ne se prononce pas (markdown, emojis, liens)."""
    texte = re.sub(r"https?://\S+", "le lien", texte or "")
    texte = re.sub(r"[*_#`>|~]", "", texte)
    texte = re.sub(r"[\U0001F000-\U0001FFFF☀-➿]", "", texte)
    return re.sub(r"\s+", " ", texte).strip()
