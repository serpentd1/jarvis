"""Vérifie que la transcription marche sur la carte graphique : Piper dit une phrase, Whisper la réécrit."""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from core import config, cuda  # noqa: E402

cuda.preparer()
import ctranslate2  # noqa: E402

print("Cartes CUDA vues par CTranslate2 :", ctranslate2.get_cuda_device_count())
print("Précisions possibles sur le GPU :", ctranslate2.get_supported_compute_types("cuda"))

from core.tts import Voix  # noqa: E402
from core.stt import Transcripteur  # noqa: E402

cfg = config.charger()
voix = Voix(cfg["voix"])
phrase = "Bonjour Jarvis, mets un minuteur de cinq minutes s'il te plaît."
audio16k = voix.audio_16k(phrase)

t = time.time()
stt = Transcripteur(cfg["transcription"])
print(f"Whisper chargé sur {stt.appareil} ({stt.precision}) en {time.time() - t:.1f} s")
stt.transcrire(audio16k)  # échauffement
t = time.time()
texte = stt.transcrire(audio16k)
print(f"Transcrit en {time.time() - t:.2f} s : « {texte} »")
