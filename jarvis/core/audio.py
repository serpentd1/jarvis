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
                             ignorer_debut=0.3, voix_min=0.24, volume_min=150.0, prefixe=None) -> np.ndarray:
        """Enregistre ta phrase. Renvoie du float32 16 kHz (vide si tu n'as rien dit).

        - attend jusqu'à `attente_debut` s que tu commences à parler (seul le son juste avant ta voix est
          gardé : une longue attente ne ralentit pas la transcription) ;
        - « tu parles » = au moins `voix_min` s de son plus fort que `volume_min` et que le bruit de fond
          (un clic ou le « ding » ne comptent pas), sans compter les `ignorer_debut` premières secondes ;
        - s'arrête après `silence_fin` s de silence (tu peux faire de petites pauses) ou `duree_max` s de parole.
        """
        fond = float(np.median(self.bruit)) if self.bruit else 200.0
        seuil = max(fond * 3, volume_min)
        duree_bloc = BLOC / FREQ
        avant = deque(maxlen=max(1, int(0.8 / duree_bloc)))
        # `prefixe` : les ~2 s juste avant « Hey Jarvis ». S'il y a déjà une phrase dedans (« mets pause, Jarvis »),
        # on n'attend presque pas la suite.
        prefixe = list(prefixe or [])
        deja_dit = sum(duree_bloc for b in prefixe if niveau(b) > seuil) >= 1.3
        if deja_dit:
            attente_debut = min(attente_debut, 1.0)
        blocs, parle, silence, temps, continu, parole = [], False, 0.0, 0.0, 0.0, 0.0
        while True:
            bloc = self.bloc()
            if bloc is None:
                break
            temps += duree_bloc
            fort = niveau(bloc) > seuil
            if not parle:
                avant.append(bloc)
                continu = continu + duree_bloc if fort and temps > ignorer_debut else 0.0
                if continu >= voix_min:
                    parle, blocs = True, prefixe + list(avant)
                elif temps > attente_debut:
                    break
            else:
                blocs.append(bloc)
                parole += duree_bloc
                silence = 0.0 if fort else silence + duree_bloc
                if silence >= silence_fin or parole >= duree_max:
                    break
        if not parle:
            if not deja_dit:
                return np.zeros(0, dtype=np.float32)
            blocs = prefixe
        return np.concatenate(blocs).astype(np.float32) / 32768.0


def niveau(bloc: np.ndarray) -> float:
    return float(np.sqrt(np.mean(bloc.astype(np.float32) ** 2)))


def son_ding(aigu: bool = False, volume: float = 0.22):
    """Le « pou-poum » : deux notes graves et rondes, qui s'éteignent doucement (pas de bip aigu)."""
    freq = 22050

    def note(hauteur: float, duree: float):
        t = np.linspace(0, duree, int(freq * duree), endpoint=False)
        onde = np.sin(2 * np.pi * hauteur * t) + 0.25 * np.sin(2 * np.pi * 2 * hauteur * t)
        attaque = np.minimum(1, t / 0.012)                 # montée douce : pas de « clac »
        return onde * attaque * np.exp(-t * 14)            # puis ça s'éteint tout seul, comme un xylophone en bois

    f1, f2 = (392, 523) if aigu else (294, 392)            # question oui/non : un peu plus haut
    silence = np.zeros(int(freq * 0.03))
    son = np.concatenate([note(f1, 0.13), silence, note(f2, 0.24)])
    return (son / 1.25 * volume).astype(np.float32), freq


def ding(volume: float = 0.22, aigu: bool = False):
    """Petit son « je t'écoute » (un peu plus haut quand Jarvis attend une réponse oui/non)."""
    sd.play(*son_ding(aigu, volume))
    sd.wait()
