"""Parole -> texte, en local : Whisper large-v3-turbo (par défaut), Parakeet, ou Whisper français.

« francais » = Whisper large-v3 réentraîné sur du français (bofenghuang, « distil-dec16 », ~2,2 Go).
Essayé le 2026-11-17 : plus lent (0,21 s contre 0,13 s) et moins bon sur les noms d'applis
(« Discord » -> « 10 scores ») que large-v3-turbo, qui reste le choix par défaut.
"""
import os
import re

import numpy as np

from . import config, cuda

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")

# Phrases que Whisper « invente » parfois sur du silence ou du bruit.
HALLUCINATIONS = (
    "sous-titres réalisés par", "sous-titrage", "amara.org", "merci d'avoir regardé",
    "abonnez-vous", "sous-titres fait par",
)

# Parakeet TDT 0.6B v3 (NVIDIA) : 25 langues européennes, version int8 (~700 Mo).
PARAKEET = {"nom": "nemo-parakeet-tdt-0.6b-v3", "depot": "istupakov/parakeet-tdt-0.6b-v3-onnx",
            "precision": "int8"}
FRANCAIS = {"depot": "bofenghuang/whisper-large-v3-french-distil-dec16", "dossier": "whisper-fr"}
# Les choix proposés dans les Paramètres et à la voix (« passe sur Whisper »).
CHOIX = [{"valeur": "francais", "titre": "Whisper français"},
         {"valeur": "parakeet", "titre": "Parakeet"},
         {"valeur": "large-v3-turbo", "titre": "Whisper"}]


def titre(modele: str) -> str:
    m = str(modele).lower()
    if m.startswith("francais"):
        return "Whisper français"
    return "Parakeet" if m.startswith("parakeet") else "Whisper"


def chemin_francais() -> str:
    """Le dossier du Whisper français (téléchargé la première fois)."""
    dossier = config.MODELES / FRANCAIS["dossier"]
    if not (dossier / "ctranslate2" / "model.bin").is_file():
        from huggingface_hub import snapshot_download
        print("[transcription] Téléchargement du Whisper français (~2,2 Go, une seule fois)…")
        snapshot_download(FRANCAIS["depot"], allow_patterns=["ctranslate2/*"], local_dir=str(dossier))
    return str(dossier / "ctranslate2")
# On corrige les façons d'écrire nos mots (Parakeet n'a pas de « mots à favoriser », et Whisper se trompe parfois).
CORRECTIONS = (
    (re.compile(r"\b(?:d?jarv[iy]|jarvy?s+e?|charvis|jervis)\b", re.IGNORECASE), "Jarvis"),
    (re.compile(r"\bpro\s?n[oa]t(?:e?s)?\b", re.IGNORECASE), "Pronote"),
    (re.compile(r"^launch\b", re.IGNORECASE), "Lance"),          # « lance Chrome » écrit en anglais
)


def est_parakeet(cfg: dict) -> bool:
    return str(cfg.get("modele", "")).lower().startswith("parakeet")


def telecharger_parakeet():
    """Télécharge Parakeet dans modeles/parakeet (reprend là où il s'était arrêté)."""
    from huggingface_hub import snapshot_download
    from onnx_asr.models.nemo import NemoConformerTdt

    fichiers = list(NemoConformerTdt._get_model_files(PARAKEET["precision"]).values())
    snapshot_download(PARAKEET["depot"], local_dir=str(config.MODELES / "parakeet"),
                      allow_patterns=["config.json", "config.yaml", *fichiers])


class Transcripteur:
    def __init__(self, cfg: dict):
        cuda.preparer()
        self.parakeet = est_parakeet(cfg)
        self.nom = cfg["modele"]
        # Mots que Whisper doit bien écrire (prénoms, applis…) : [transcription] mots dans config.toml.
        self.mots = str(cfg.get("mots") or "Jarvis, Pronote").replace(",", " ").split()
        if self.parakeet:
            try:
                self._charger_parakeet(cfg)
                self.nom = "parakeet"
                return
            except Exception as e:  # noqa: BLE001
                print(f"[transcription] Parakeet impossible, on repasse sur Whisper : {e}")
                self.parakeet = False
                cfg = {**cfg, "modele": "large-v3-turbo"}
                self.nom = cfg["modele"]
        self._charger_whisper(cfg)

    def _charger_parakeet(self, cfg: dict):
        import onnx_asr
        import onnxruntime as ort

        dossier = config.MODELES / "parakeet"
        if not (dossier / "config.json").exists():
            telecharger_parakeet()
        # La version int8 est faite pour le processeur : elle y est même plus rapide (~0,1 s)
        # et laisse la carte graphique à Ollama et à la voix. appareil = "cuda" force la carte graphique.
        essais = [("cpu", ["CPUExecutionProvider"])]
        if cfg["appareil"] == "cuda" and "CUDAExecutionProvider" in ort.get_available_providers():
            essais.insert(0, ("cuda", ["CUDAExecutionProvider", "CPUExecutionProvider"]))

        options = ort.SessionOptions()
        options.log_severity_level = 3      # pas d'avertissements techniques dans le journal

        def charger(fournisseurs):
            return onnx_asr.load_model(PARAKEET["nom"], dossier, quantization=PARAKEET["precision"],
                                       sess_options=options, providers=fournisseurs)

        derniere_erreur = None
        for appareil, fournisseurs in essais:
            try:
                try:
                    self.modele = charger(fournisseurs)
                except Exception:  # noqa: BLE001 — fichier manquant ou abîmé : on retélécharge
                    telecharger_parakeet()
                    self.modele = charger(fournisseurs)
                self.modele.recognize(np.zeros(16000, dtype=np.float32))  # petit essai
                self.appareil, self.precision = appareil, PARAKEET["precision"]
                return
            except Exception as e:  # noqa: BLE001
                derniere_erreur = e
                print(f"[transcription] Parakeet sur {appareil} impossible : {e}")
        raise RuntimeError(f"Impossible de charger Parakeet : {derniere_erreur}")

    def _charger_whisper(self, cfg: dict):
        from faster_whisper import WhisperModel

        dossier = str(config.MODELES / "whisper")
        essais = []
        if cfg["appareil"] in ("auto", "cuda"):
            essais.append(("cuda", cfg["precision"]))
        if cfg["appareil"] in ("auto", "cpu"):
            essais.append(("cpu", "int8"))

        derniere_erreur = None
        for appareil, precision in essais:
            try:
                if str(cfg["modele"]).lower().startswith("francais"):
                    self.modele = WhisperModel(chemin_francais(), device=appareil, compute_type=precision)
                    self.nom = "francais"
                else:
                    try:        # modèle déjà téléchargé : on ne contacte pas Internet
                        self.modele = WhisperModel(cfg["modele"], device=appareil, compute_type=precision,
                                                   download_root=dossier, local_files_only=True)
                    except Exception:  # noqa: BLE001
                        self.modele = WhisperModel(cfg["modele"], device=appareil, compute_type=precision,
                                                   download_root=dossier)
                # Un petit essai pour être sûr que le GPU marche vraiment.
                list(self.modele.transcribe(np.zeros(16000, dtype=np.float32), language="fr")[0])
                self.appareil, self.precision = appareil, precision
                return
            except Exception as e:  # noqa: BLE001
                derniere_erreur = e
                print(f"[transcription] {appareil} impossible : {e}")
        raise RuntimeError(f"Impossible de charger Whisper : {derniere_erreur}")

    def transcrire(self, audio16k: np.ndarray) -> str:
        """audio16k : float32 mono, 16 kHz, entre -1 et 1."""
        if self.parakeet:
            texte = str(self.modele.recognize(audio16k.astype(np.float32, copy=False), sample_rate=16000)).strip()
        else:
            segments, _ = self.modele.transcribe(
                audio16k, language="fr", beam_size=5, vad_filter=True,
                condition_on_previous_text=False, hotwords=" ".join(self.mots),
            )
            # un morceau où Whisper n'est pas sûr qu'il y ait de la parole, et qu'il « devine » mal, est jeté
            texte = " ".join(s.text.strip() for s in segments
                             if not (s.no_speech_prob > 0.6 and s.avg_logprob < -0.8)).strip()
        for motif, mot in CORRECTIONS:
            texte = motif.sub(mot, texte)
        texte = re.sub(r"^(?:eh|et|hé|é) Jarvis\b", "Hey Jarvis", texte, flags=re.IGNORECASE)
        if any(h in texte.lower() for h in HALLUCINATIONS):
            return ""
        return texte
