"""Parole -> texte avec faster-whisper (carte graphique si possible)."""
import os

import numpy as np

from . import config, cuda

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")

# Phrases que Whisper « invente » parfois sur du silence ou du bruit.
HALLUCINATIONS = (
    "sous-titres réalisés par", "sous-titrage", "amara.org", "merci d'avoir regardé",
    "abonnez-vous", "sous-titres fait par",
)


class Transcripteur:
    def __init__(self, cfg: dict):
        cuda.preparer()
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
        segments, _ = self.modele.transcribe(
            audio16k, language="fr", beam_size=5, vad_filter=True,
            condition_on_previous_text=False, hotwords="Jarvis Pronote",
        )
        texte = " ".join(s.text.strip() for s in segments).strip()
        if any(h in texte.lower() for h in HALLUCINATIONS):
            return ""
        return texte
