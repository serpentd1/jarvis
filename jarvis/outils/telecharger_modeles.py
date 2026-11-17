"""Télécharge les modèles dont Jarvis a besoin (une seule fois) :
- « Hey Jarvis » (openWakeWord, quelques Mo)
- la voix choisie dans config.toml (Kokoro ≈ 340 Mo, ou Piper ≈ 60 Mo)
- le modèle de transcription choisi dans config.toml (Parakeet ≈ 700 Mo, ou Whisper)
Le modèle Ollama se télécharge à part : ollama pull <modèle>.
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from core import config, cuda  # noqa: E402


def main():
    cfg = config.charger()
    config.MODELES.mkdir(exist_ok=True)

    print("1/3  Mot d'activation « Hey Jarvis »…")
    import openwakeword.utils
    openwakeword.utils.download_models(model_names=["hey_jarvis"])

    from core import tts
    voix = tts.nom_depuis_config(cfg["voix"])
    print(f"2/3  Voix « {tts.CATALOGUE[voix]['titre']} »…")
    tts.telecharger(voix)

    from core import stt
    modele = cfg["transcription"]["modele"]
    if stt.est_parakeet(cfg["transcription"]):
        print("3/3  Modèle de transcription Parakeet (~700 Mo la première fois)…")
        stt.telecharger_parakeet()
    else:
        print(f"3/3  Modèle Whisper {modele} (peut être long la première fois)…")
        cuda.preparer()
        from faster_whisper import download_model
        download_model(modele, cache_dir=str(config.MODELES / "whisper"))

    print("Modèles prêts.")


if __name__ == "__main__":
    main()
