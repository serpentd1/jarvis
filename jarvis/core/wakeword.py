"""Détection du mot d'activation « Hey Jarvis » (openWakeWord, sur le processeur, très léger)."""
import numpy as np


class MotActivation:
    def __init__(self, seuil: float = 0.5):
        from openwakeword.model import Model

        self.modele = Model(wakeword_models=["hey_jarvis"], inference_framework="onnx")
        self.seuil = seuil

    def score(self, bloc_int16: np.ndarray) -> float:
        scores = self.modele.predict(bloc_int16)
        return max(scores.values()) if scores else 0.0

    def entendu(self, bloc_int16: np.ndarray) -> bool:
        return self.score(bloc_int16) >= self.seuil

    def reinitialiser(self):
        """Évite que le même « Hey Jarvis » déclenche deux fois."""
        self.modele.reset()
