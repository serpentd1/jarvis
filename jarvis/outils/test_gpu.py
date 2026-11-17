"""Vérifie que la transcription marche : la voix de Jarvis dit une phrase, Parakeet (ou Whisper) la réécrit."""
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from core import config, cuda  # noqa: E402

cuda.preparer()
import onnxruntime  # noqa: E402

print("Fournisseurs ONNX (Parakeet) :", onnxruntime.get_available_providers())

from core.tts import Voix  # noqa: E402
from core.stt import Transcripteur  # noqa: E402

cfg = config.charger()
voix = Voix(cfg["voix"])
phrase = "Bonjour Jarvis, mets un minuteur de cinq minutes s'il te plaît."
audio16k = voix.audio_16k(phrase)

t = time.time()
stt = Transcripteur(cfg["transcription"])
print(f"{'Parakeet' if stt.parakeet else 'Whisper'} chargé sur {stt.appareil} ({stt.precision}) en {time.time() - t:.1f} s")
stt.transcrire(audio16k)  # échauffement
t = time.time()
texte = stt.transcrire(audio16k)
print(f"Transcrit en {time.time() - t:.2f} s : « {texte} »")
