"""Micro : flux continu en 16 kHz, découpé en blocs de 80 ms, + enregistrement d'une commande."""
import queue
import time
from collections import deque

import numpy as np
import sounddevice as sd
from scipy.signal import resample_poly

FREQ = 16000
BLOC = 1280  # 80 ms à 16 kHz : la taille attendue par openWakeWord


def trouver_micro(nom: str):
    """Index du micro dont le nom contient `nom` (None = micro par défaut)."""
    if not nom:
        return None
    for i, d in enumerate(sd.query_devices()):
        if d["max_input_channels"] > 0 and nom.lower() in d["name"].lower():
            return i
    print(f"[micro] « {nom} » introuvable, j'utilise le micro par défaut.")
    return None


class Micro:
    def __init__(self, nom_appareil: str = ""):
        self.appareil = trouver_micro(nom_appareil)
        self.file = queue.Queue()
        self._reste = np.zeros(0, dtype=np.int16)
        self.bruit = deque(maxlen=25)  # niveau sonore des 2 dernières secondes
        try:
            self.freq_source = FREQ
            self.flux = sd.InputStream(samplerate=FREQ, channels=1, dtype="int16",
                                       blocksize=BLOC, device=self.appareil, callback=self._recu)
        except Exception:  # noqa: BLE001 — certains micros refusent 16 kHz
            self.freq_source = int(sd.query_devices(self.appareil, "input")["default_samplerate"])
            self.flux = sd.InputStream(samplerate=self.freq_source, channels=1, dtype="int16",
                                       device=self.appareil, callback=self._recu)
        nom = sd.query_devices(self.flux.device)["name"]
        print(f"[micro] {nom} ({self.freq_source} Hz)")

    def _recu(self, donnees, _frames, _temps, _statut):
        self.file.put(donnees[:, 0].copy())

    def demarrer(self):
        if not self.flux.active:
            self.flux.start()

    def pause(self):
        """Ferme vraiment le micro (le voyant « micro utilisé » de Windows s'éteint)."""
        if self.flux.active:
            self.flux.stop()
        self.vider()

    def arreter(self):
        self.flux.stop()
        self.flux.close()

    def vider(self):
        """Oublie ce que le micro a entendu (par ex. pendant que Jarvis parlait)."""
        while not self.file.empty():
            self.file.get_nowait()
        self._reste = np.zeros(0, dtype=np.int16)

    def bloc(self, delai: float = 1.0):
        """Renvoie le prochain bloc de 80 ms en int16 16 kHz, ou None si rien n'arrive."""
        while len(self._reste) < BLOC:
            try:
                brut = self.file.get(timeout=delai)
            except queue.Empty:
                return None
            if self.freq_source != FREQ:
                brut = resample_poly(brut.astype(np.float32), FREQ, self.freq_source).astype(np.int16)
            self._reste = np.concatenate([self._reste, brut])
        bloc, self._reste = self._reste[:BLOC], self._reste[BLOC:]
        self.bruit.append(niveau(bloc))
        return bloc

    def enregistrer_commande(self, silence_fin=1.4, duree_max=20, attente_debut=6,
                             ignorer_debut=0.3, voix_min=0.24) -> np.ndarray:
        """Enregistre ta phrase. Renvoie du float32 16 kHz (vide si tu n'as rien dit).

        - attend jusqu'à `attente_debut` s que tu commences à parler ;
        - « tu parles » = au moins `voix_min` s de son continu (un clic ou le « ding » ne comptent pas),
          sans compter les `ignorer_debut` premières secondes (le temps du « ding ») ;
        - s'arrête après `silence_fin` s de silence : tu peux faire de petites pauses en parlant.
        """
        fond = float(np.median(self.bruit)) if self.bruit else 200.0
        seuil = max(fond * 3, 150.0)
        duree_bloc = BLOC / FREQ
        blocs, parle, silence, temps, continu = [], False, 0.0, 0.0, 0.0
        while temps < duree_max:
            bloc = self.bloc()
            if bloc is None:
                break
            blocs.append(bloc)
            temps += duree_bloc
            fort = niveau(bloc) > seuil
            if not parle:
                continu = continu + duree_bloc if fort and temps > ignorer_debut else 0.0
                if continu >= voix_min:
                    parle = True
                elif temps > attente_debut:
                    break
            else:
                silence = 0.0 if fort else silence + duree_bloc
                if silence >= silence_fin:
                    break
        if not parle:
            return np.zeros(0, dtype=np.float32)
        return np.concatenate(blocs).astype(np.float32) / 32768.0


def niveau(bloc: np.ndarray) -> float:
    return float(np.sqrt(np.mean(bloc.astype(np.float32) ** 2)))


def ding(volume: float = 0.25, aigu: bool = False):
    """Petit son « je t'écoute » (plus aigu quand Jarvis attend une réponse oui/non)."""
    t = np.linspace(0, 0.12, int(22050 * 0.12), endpoint=False)
    f1, f2 = (1320, 1760) if aigu else (880, 1320)
    son = np.concatenate([np.sin(2 * np.pi * f1 * t), np.sin(2 * np.pi * f2 * t)])
    enveloppe = np.minimum(1, np.minimum(np.arange(len(son)), np.arange(len(son))[::-1]) / 400)
    sd.play((son * enveloppe * volume).astype(np.float32), 22050)
    sd.wait()
