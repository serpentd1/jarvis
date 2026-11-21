"""Haut-parleurs de Jarvis et anti-écho.

- `Lecteur` : un seul flux de sortie qui reste ouvert (pas de petit délai au début de chaque phrase).
  Les morceaux de voix sont joués à la suite, dès qu'ils arrivent. Il sait exactement ce qui a été dit,
  pour que l'historique ne garde que ça quand on coupe Jarvis.
- `AntiEcho` : l'annulation d'écho de WebRTC (celle des appels vidéo). Le son joué sert de référence :
  elle le retire de ce qu'entend le micro, pour que Jarvis ne se coupe pas en entendant sa propre voix.
"""
import collections
import logging
import threading
import time

import numpy as np
import sounddevice as sd
from scipy.signal import firwin, lfilter, resample_poly

log = logging.getLogger("sortie")

FREQ_SORTIE = 24000
BLOC_SORTIE = 480           # 20 ms
FREQ_MICRO = 16000
TRAME_AEC = 160             # 10 ms à 16 kHz : la taille attendue par WebRTC


class Reechantillonneur:
    """24 kHz -> 16 kHz, bloc après bloc, sans coupure entre les blocs (l'anti-écho a besoin d'un son propre)."""

    def __init__(self):
        self.b = firwin(96, 7600, fs=48000) * 2.0
        self.zi = np.zeros(len(self.b) - 1)
        self.phase = 0

    def __call__(self, x: np.ndarray) -> np.ndarray:
        haut = np.zeros(len(x) * 2)
        haut[::2] = x
        y, self.zi = lfilter(self.b, 1.0, haut, zi=self.zi)
        debut = (-self.phase) % 3
        self.phase = (self.phase + len(haut)) % 3
        return y[debut::3].astype(np.float32)


class AntiEcho:
    def __init__(self):
        from livekit import rtc
        self._rtc = rtc
        self.apm = rtc.AudioProcessingModule(echo_cancellation=True, noise_suppression=False,
                                             high_pass_filter=True, auto_gain_control=False)
        self._rendu = np.zeros(0, dtype=np.int16)
        self._micro = np.zeros(0, dtype=np.int16)
        # le micro et le haut-parleur l'appellent chacun depuis leur fil : jamais en même temps
        self._verrou = threading.Lock()

    def _trame(self, x: np.ndarray):
        return self._rtc.AudioFrame(data=x.tobytes(), sample_rate=FREQ_MICRO, num_channels=1,
                                    samples_per_channel=TRAME_AEC)

    def regler_delai(self, secondes: float):
        try:
            self.apm.set_stream_delay_ms(int(max(0.0, secondes) * 1000))
        except Exception:  # noqa: BLE001
            pass

    def son_joue(self, x16: np.ndarray):
        """Le son qui part dans les haut-parleurs (float32, 16 kHz)."""
        self._rendu = np.concatenate([self._rendu, np.clip(x16 * 32767, -32768, 32767).astype(np.int16)])
        while len(self._rendu) >= TRAME_AEC:
            trame, self._rendu = self._rendu[:TRAME_AEC], self._rendu[TRAME_AEC:]
            with self._verrou:
                self.apm.process_reverse_stream(self._trame(trame))

    def nettoyer(self, x: np.ndarray) -> np.ndarray:
        """Ce qu'entend le micro (int16, 16 kHz), sans l'écho de la voix de Jarvis."""
        self._micro = np.concatenate([self._micro, x])
        sorties = []
        while len(self._micro) >= TRAME_AEC:
            trame, self._micro = self._micro[:TRAME_AEC], self._micro[TRAME_AEC:]
            f = self._trame(trame)
            with self._verrou:
                self.apm.process_stream(f)
            sorties.append(np.frombuffer(f.data, dtype=np.int16).copy())
        return np.concatenate(sorties) if sorties else np.zeros(0, dtype=np.int16)


class Phrase:
    __slots__ = ("texte", "total", "lus", "complete")

    def __init__(self, texte: str):
        self.texte, self.total, self.lus, self.complete = texte, 0, 0, False


class Lecteur:
    def __init__(self, anti_echo: AntiEcho | None = None):
        self.anti_echo = anti_echo
        self._vers16 = Reechantillonneur()
        self.verrou = threading.Lock()
        self.morceaux = collections.deque()       # [indice de la phrase, audio, position]
        self.phrases: list[Phrase] = []
        self.actuelle = None                      # indice de la phrase qui sort des haut-parleurs
        self.premier_son = None                   # heure du tout premier son de la réponse
        self.dernier_son = 0.0                    # heure du dernier son joué
        self.flux = sd.OutputStream(samplerate=FREQ_SORTIE, channels=1, dtype="float32",
                                    blocksize=BLOC_SORTIE, latency="low", callback=self._remplir)
        self.flux.start()
        log.info("Haut-parleurs : %s (latence %.0f ms)", sd.query_devices(self.flux.device)["name"],
                 self.flux.latency * 1000)

    # ── appelé par la carte son, toutes les 20 ms ──
    def _remplir(self, sortie, frames, _temps, _statut):
        bloc = np.zeros(frames, dtype=np.float32)
        rempli = 0
        with self.verrou:
            while rempli < frames and self.morceaux:
                m = self.morceaux[0]
                i, audio, pos = m
                n = min(frames - rempli, len(audio) - pos)
                bloc[rempli:rempli + n] = audio[pos:pos + n]
                rempli += n
                m[2] += n
                self.phrases[i].lus += n
                self.actuelle = i
                if m[2] >= len(audio):
                    self.morceaux.popleft()
            if rempli:
                maintenant = time.perf_counter()
                self.dernier_son = maintenant
                if self.premier_son is None:
                    self.premier_son = maintenant
        sortie[:, 0] = bloc
        if self.anti_echo is not None:
            try:
                self.anti_echo.son_joue(self._vers16(bloc))
            except Exception:  # noqa: BLE001
                pass

    # ── une nouvelle réponse ──
    def reinitialiser(self):
        with self.verrou:
            self.morceaux.clear()
            self.phrases = []
            self.actuelle = None
            self.premier_son = None

    def nouvelle_phrase(self, texte: str) -> int:
        with self.verrou:
            self.phrases.append(Phrase(texte))
            return len(self.phrases) - 1

    def ajouter(self, i: int, audio: np.ndarray, freq: int):
        audio = np.asarray(audio, dtype=np.float32).ravel()
        if freq != FREQ_SORTIE:
            audio = resample_poly(audio, FREQ_SORTIE, freq).astype(np.float32)
        if not len(audio):
            return
        with self.verrou:
            if i < len(self.phrases):
                self.phrases[i].total += len(audio)
                self.morceaux.append([i, audio, 0])

    def fin_phrase(self, i: int):
        with self.verrou:
            if i < len(self.phrases):
                self.phrases[i].complete = True

    def occupe(self) -> bool:
        """Il reste du son à jouer (ou il vient juste de sortir de la carte son)."""
        with self.verrou:
            if self.morceaux:
                return True
        return time.perf_counter() - self.dernier_son < self.flux.latency + 0.05

    def son_recent(self, secondes: float = 0.3) -> bool:
        return time.perf_counter() - self.dernier_son < secondes

    def phrase_en_cours(self):
        with self.verrou:
            if self.actuelle is None:
                return None
            p = self.phrases[self.actuelle]
            return self.actuelle, p.texte, p.total / FREQ_SORTIE

    def vider(self):
        """Stop immédiat : tout ce qui restait à dire est jeté."""
        with self.verrou:
            self.morceaux.clear()
        self.dernier_son = 0.0

    def dit(self) -> str:
        """Ce que Jarvis a vraiment prononcé (la phrase coupée est gardée jusqu'au mot près, à peu près)."""
        morceaux = []
        with self.verrou:
            for p in self.phrases:
                if p.lus <= 0:
                    break
                if p.complete and p.lus >= p.total:
                    morceaux.append(p.texte)
                    continue
                mots = p.texte.split()
                n = int(len(mots) * p.lus / max(1, p.total))
                if n:
                    morceaux.append(" ".join(mots[:n]))
                break
        return " ".join(morceaux).strip()

    def fermer(self):
        try:
            self.flux.stop()
            self.flux.close()
        except Exception:  # noqa: BLE001
            pass
