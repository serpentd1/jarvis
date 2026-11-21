"""Détecteur de voix Silero : « est-ce que quelqu'un parle ? », trame par trame (32 ms).

On utilise le petit modèle ONNX livré avec faster-whisper (rien à télécharger, sur le processeur, aucune
mémoire graphique). Il distingue une voix d'un bruit (clavier, porte, musique…) bien mieux qu'un simple volume.
"""
import os

import numpy as np

TRAME = 512        # 32 ms à 16 kHz
CONTEXTE = 64      # le modèle voit aussi la fin de la trame précédente


def _chemin_modele() -> str:
    import faster_whisper
    return os.path.join(os.path.dirname(faster_whisper.__file__), "assets", "silero_vad_v6.onnx")


class Silero:
    _session = None                 # un seul modèle chargé, partagé

    def __init__(self):
        if Silero._session is None:
            import onnxruntime as ort
            options = ort.SessionOptions()
            options.inter_op_num_threads = options.intra_op_num_threads = 1
            options.log_severity_level = 3
            Silero._session = ort.InferenceSession(_chemin_modele(), sess_options=options,
                                                   providers=["CPUExecutionProvider"])
        self.reinitialiser()

    def reinitialiser(self):
        self.h = np.zeros((1, 1, 128), dtype=np.float32)
        self.c = np.zeros((1, 1, 128), dtype=np.float32)
        self.contexte = np.zeros(CONTEXTE, dtype=np.float32)
        self.reste = np.zeros(0, dtype=np.float32)
        self.derniere = 0.0

    def proba(self, bloc: np.ndarray) -> float:
        """Probabilité qu'il y ait une voix dans ce bloc (int16 ou float32, 16 kHz), de 0 à 1.
        C'est la plus haute de ses trames de 32 ms (un bloc trop court garde la dernière valeur)."""
        x = bloc.astype(np.float32)
        if bloc.dtype == np.int16:
            x /= 32768.0
        self.reste = np.concatenate([self.reste, x])
        meilleure = None
        while len(self.reste) >= TRAME:
            trame, self.reste = self.reste[:TRAME], self.reste[TRAME:]
            entree = np.concatenate([self.contexte, trame])[None, :]
            p, self.h, self.c = Silero._session.run(None, {"input": entree, "h": self.h, "c": self.c})
            self.contexte = trame[-CONTEXTE:]
            p = float(np.ravel(p)[0])
            meilleure = p if meilleure is None else max(meilleure, p)
        if meilleure is not None:
            self.derniere = meilleure
        return self.derniere
