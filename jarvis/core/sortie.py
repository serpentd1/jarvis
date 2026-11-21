"""Haut-parleurs de Jarvis et anti-écho.

- `Lecteur` : un seul flux de sortie qui reste ouvert (pas de petit délai au début de chaque phrase).
  Les morceaux de voix sont joués à la suite, dès qu'ils arrivent. Il sait exactement ce qui a été dit,
  pour que l'historique ne garde que ça quand on coupe Jarvis.
- `AntiEcho` : l'annulation d'écho de WebRTC (celle des appels vidéo). Le son joué sert de référence :
  elle le retire de ce qu'entend le micro, pour que Jarvis ne se coupe pas en entendant sa propre voix.
- La sortie de Jarvis peut être un autre appareil que celui de Windows (`[voix] sortie`, ex. "JBL") :
  seul Jarvis parle dans l'enceinte, le reste du PC garde sa sortie.
"""
import collections
import logging
import re
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

# Le son d'une enceinte Bluetooth sort ~150-300 ms après avoir quitté l'ordinateur (le PC ne le sait pas).
DELAI_SANS_FIL_MS = 200
SANS_FIL = re.compile(r"\b(?:bluetooth|bt|jbl|bose|sony|flip|charge|go|clip|boom|megaboom|wonderboom|xtreme|pulse|"
                      r"soundcore|soundlink|marshall|anker|beats|airpods|buds|ue|hands free|mains libres)\b")
# Ordre de préférence des façons de parler à la carte son : WASAPI (rapide), puis MME (marche partout).
# WDM-KS est laissé de côté : il prend l'appareil pour lui tout seul et refuse souvent nos 24 kHz.
ORDRE_API = {"windows wasapi": 0, "mme": 1, "windows directsound": 2}


def candidats_sortie(nom: str, appareils=None, apis=None) -> list:
    """Les sorties dont le nom contient `nom` (majuscules ou pas), la meilleure d'abord : [(indice, api), …].
    Vide si `nom` est vide ou si aucune sortie ne correspond (enceinte éteinte)."""
    nom = (nom or "").strip().lower()
    if not nom:
        return []
    appareils = sd.query_devices() if appareils is None else appareils
    apis = sd.query_hostapis() if apis is None else apis
    trouves = []
    for i, d in enumerate(appareils):
        if d.get("max_output_channels", 0) <= 0 or nom not in str(d.get("name", "")).lower():
            continue
        h = d.get("hostapi", -1)
        api = str(apis[h]["name"]) if 0 <= h < len(apis) else ""
        rang = ORDRE_API.get(api.lower())
        if rang is not None:
            trouves.append((rang, d.get("index", i), api))
    trouves.sort()
    return [(i, api) for _, i, api in trouves]


def trouver_sortie(nom: str, appareils=None, apis=None):
    """Indice de la sortie dont le nom contient `nom` (WASAPI d'abord, sinon MME). None = sortie de Windows."""
    c = candidats_sortie(nom, appareils, apis)
    return c[0][0] if c else None


def delai_sortie_ms(reglage, *noms) -> int:
    """Retard (ms) entre le son envoyé et le son qui sort vraiment du haut-parleur, pour l'anti-écho.
    `reglage` plus grand que 0 : c'est lui (`[conversation] delai_sortie_ms`). 0 = automatique : 200 ms si un
    des `noms` ressemble à une enceinte Bluetooth (JBL, Bose, Sony, Flip…), sinon 0 (sortie avec un fil)."""
    try:
        r = float(reglage or 0)
    except (TypeError, ValueError):
        r = 0.0
    if r > 0:
        return int(r)
    texte = " ".join(str(n) for n in noms if n).lower()
    return DELAI_SANS_FIL_MS if SANS_FIL.search(texte) else 0


def sorties_windows():
    """Les noms des sorties que Windows voit EN CE MOMENT (PortAudio, lui, garde la liste de son démarrage).
    Les noms sont coupés à 31 lettres. None = impossible de savoir."""
    try:
        import ctypes
        from ctypes import wintypes

        class Caps(ctypes.Structure):
            _fields_ = [("wMid", wintypes.WORD), ("wPid", wintypes.WORD), ("vDriverVersion", wintypes.UINT),
                        ("szPname", wintypes.WCHAR * 32), ("dwFormats", wintypes.DWORD),
                        ("wChannels", wintypes.WORD), ("wReserved1", wintypes.WORD), ("dwSupport", wintypes.DWORD)]

        winmm = ctypes.WinDLL("winmm")
        noms = []
        for i in range(winmm.waveOutGetNumDevs()):
            c = Caps()
            if winmm.waveOutGetDevCapsW(ctypes.c_size_t(i), ctypes.byref(c), ctypes.sizeof(c)) == 0:
                noms.append(c.szPname)
        return noms
    except Exception:  # noqa: BLE001
        return None


def jouer_simple(audio, freq: int, nom_sortie: str = ""):
    """Joue un son et attend la fin, sur la sortie de Jarvis (ou celle de Windows). Sert quand il n'y a pas de
    `Lecteur` (mode texte, petit « ding » de secours)."""
    for appareil, api in candidats_sortie(nom_sortie) + [(None, "")]:
        try:
            extra = sd.WasapiSettings(auto_convert=True) if "wasapi" in api.lower() else None
            sd.play(audio, freq, device=appareil, extra_settings=extra)
            sd.wait()
            return
        except Exception as e:  # noqa: BLE001
            log.warning("Sortie %s impossible (%s)", appareil, e)


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
        """Le temps entre le son envoyé au haut-parleur et son retour dans le micro (WebRTC accepte 0 à 500 ms)."""
        try:
            self.apm.set_stream_delay_ms(int(min(0.5, max(0.0, secondes)) * 1000))
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


def _present(nom: str, noms_windows) -> bool:
    """`nom` est-il dans la liste de Windows ? (ses noms sont coupés à 31 lettres : « …(JBL Flip Essenti »)"""
    nom = nom.lower()
    for n in (x.lower() for x in noms_windows):
        if nom in n or (len(n) >= 31 and any(n.endswith(nom[:k]) for k in range(len(nom) - 1, 2, -1))):
            return True
    return False


class Lecteur:
    def __init__(self, anti_echo: AntiEcho | None = None, nom_sortie: str = "", delai_ms=0):
        self.anti_echo = anti_echo
        self.nom_sortie = str(nom_sortie or "").strip()   # « JBL » : seul Jarvis parle dans l'enceinte
        self.reglage_delai = delai_ms                     # [conversation] delai_sortie_ms (0 = automatique)
        self.latence_micro = 0.0                          # donnée par main.py, pour l'anti-écho
        self._vers16 = Reechantillonneur()
        self.verrou = threading.Lock()
        self.morceaux = collections.deque()       # [indice de la phrase, audio, position]
        self.phrases: list[Phrase] = []
        self.actuelle = None                      # indice de la phrase qui sort des haut-parleurs
        self.premier_son = None                   # heure du tout premier son de la réponse
        self.dernier_son = 0.0                    # heure du dernier son joué
        self._dernier_appel = time.perf_counter()  # la carte son nous demande du son toutes les 20 ms
        self._derniere_recherche, self._pause_recherche = 0.0, 3.0
        self.flux, self.sur_voulue, self.nom_reel, self._retard = None, False, "", 0.0
        self._ouvrir()

    # ── choix de la sortie ──
    def _ouvrir(self, sortie_windows: bool = False):
        """Ouvre le flux sur la sortie voulue ; si elle n'est pas là (enceinte éteinte), sur celle de Windows."""
        self._fermer_flux()
        essais = [] if sortie_windows else candidats_sortie(self.nom_sortie)
        if self.nom_sortie and not essais and not sortie_windows:
            log.warning("Sortie « %s » introuvable (enceinte éteinte ?) : je parle sur la sortie de Windows, "
                        "je la chercherai encore à la prochaine phrase.", self.nom_sortie)
        essais.append((None, ""))
        for appareil, api in essais:
            reglages = dict(samplerate=FREQ_SORTIE, channels=1, dtype="float32", blocksize=BLOC_SORTIE,
                            latency="low", callback=self._remplir, device=appareil)
            if "wasapi" in api.lower():
                # WASAPI n'accepte que la fréquence de l'appareil (44,1 ou 48 kHz) : Windows convertit nos 24 kHz
                reglages["extra_settings"] = sd.WasapiSettings(auto_convert=True)
            try:
                flux = sd.OutputStream(**reglages)
                self._dernier_appel = time.perf_counter()
                flux.start()
            except Exception as e:  # noqa: BLE001
                if appareil is None:
                    raise
                log.warning("Sortie %s (%s) impossible : %s", appareil, api, e)
                continue
            self.flux, self.sur_voulue = flux, appareil is not None
            break
        self.nom_reel = self._nom_appareil()
        self._retard = self.delai_ms() / 1000
        log.info("Haut-parleurs : %s (latence %.0f ms, retard de l'enceinte %.0f ms)", self.nom_reel,
                 self.flux.latency * 1000, self._retard * 1000)
        self.regler_anti_echo()

    def _nom_appareil(self) -> str:
        try:
            if self.sur_voulue:
                return sd.query_devices(self.flux.device)["name"]
            # sortie de Windows : son vrai nom est dans WASAPI (MME dit juste « Mappeur de sons »)
            for api in sd.query_hostapis():
                if "wasapi" in api["name"].lower() and api.get("default_output_device", -1) >= 0:
                    return sd.query_devices(api["default_output_device"])["name"]
            return sd.query_devices(self.flux.device)["name"]
        except Exception:  # noqa: BLE001
            return ""

    def _fermer_flux(self):
        if self.flux is None:
            return
        try:
            self.flux.abort()
            self.flux.close()
        except Exception:  # noqa: BLE001
            pass
        self.flux = None

    def delai_ms(self) -> int:
        """Retard de l'enceinte (Bluetooth…) en plus de la latence de la carte son."""
        return delai_sortie_ms(self.reglage_delai, self.nom_reel, self.nom_sortie if self.sur_voulue else "")

    def regler_anti_echo(self):
        """Dit à l'anti-écho quand la voix de Jarvis revient dans le micro : micro + carte son + enceinte."""
        if self.anti_echo is None or self.flux is None:
            return
        total = self.latence_micro + self.flux.latency + self._retard
        self.anti_echo.regler_delai(total)
        log.info("Anti-écho : la voix revient dans le micro après ~%.0f ms", total * 1000)

    def _flux_mort(self) -> bool:
        """La sortie ne demande plus de son (enceinte éteinte ou partie en pleine phrase)."""
        return self.flux is None or not self.flux.active or time.perf_counter() - self._dernier_appel > 2.0

    def verifier_sortie(self, micro=None):
        """Avant chaque réponse : Jarvis est-il sur la bonne sortie ? Sinon on cherche encore l'enceinte (elle a pu
        s'allumer, ou se reconnecter avec un autre numéro). `micro` : il faut le rouvrir si la liste est relue.
        Ne plante jamais : au pire, Jarvis parle sur la sortie de Windows."""
        try:
            self._verifier_sortie(micro)
        except Exception:  # noqa: BLE001
            log.exception("vérification de la sortie du son")
            if self.flux is None:
                try:
                    self._ouvrir(sortie_windows=True)
                except Exception:  # noqa: BLE001
                    log.exception("aucune sortie de son")

    def _verifier_sortie(self, micro=None):
        mort = self._flux_mort()
        if not mort and (self.sur_voulue or not self.nom_sortie):
            return
        if not self.nom_sortie:
            log.warning("La sortie de Windows ne répond plus : je la rouvre.")
            self._ouvrir()
            return
        maintenant = time.perf_counter()
        if not mort and maintenant - self._derniere_recherche < self._pause_recherche:
            return                                  # pas à chaque petite phrase : toutes les 3 s au plus
        self._derniere_recherche = maintenant
        noms = sorties_windows()
        if noms is None or not _present(self.nom_sortie, noms):
            if mort:                                # l'enceinte est partie : la sortie de Windows, en attendant
                self._ouvrir(sortie_windows=True)
            return
        # Windows voit l'enceinte, mais la liste de PortAudio date du démarrage (et ses numéros ont pu changer) :
        # on la relit. Cela ferme tous les flux de son : le micro aussi, qu'on rouvre juste après.
        log.info("La sortie « %s » est de retour : je relis la liste des appareils.", self.nom_sortie)
        actif = bool(micro is not None and micro.flux is not None and micro.flux.active)
        if micro is not None:
            micro.fermer_flux()
        self._fermer_flux()
        try:
            try:
                sd._terminate()
            except Exception:  # noqa: BLE001
                log.exception("PortAudio ne s'arrête pas")
            else:
                sd._initialize()
                if hasattr(sd, "_last_callback"):
                    sd._last_callback = None        # l'ancien « sd.play » appartient à l'ancienne liste
        finally:
            if micro is not None:
                micro.rouvrir(demarrer=actif)
                if micro.flux is not None:
                    self.latence_micro = micro.flux.latency
            self._ouvrir()
            # Windows la montre mais elle ne s'ouvre pas : on ne relit plus la liste à chaque phrase
            self._pause_recherche = 3.0 if self.sur_voulue else 60.0

    def jouer(self, audio: np.ndarray, freq: int, attendre: bool = True):
        """Un petit son (le « ding ») par la même sortie que la voix : l'anti-écho le connaît aussi."""
        i = self.nouvelle_phrase("")
        self.ajouter(i, audio, freq)
        self.fin_phrase(i)
        fin = time.perf_counter() + len(audio) / freq + 1.0
        while attendre and self.occupe() and time.perf_counter() < fin:
            time.sleep(0.01)

    # ── appelé par la carte son, toutes les 20 ms ──
    def _remplir(self, sortie, frames, _temps, _statut):
        self._dernier_appel = time.perf_counter()
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
        """Il reste du son à jouer (ou il vient juste de sortir de la carte son, ou de l'enceinte Bluetooth)."""
        with self.verrou:
            reste = bool(self.morceaux)
        if reste and self._flux_mort():
            # l'enceinte s'est éteinte en pleine phrase : la suite sort sur la sortie de Windows
            log.warning("La sortie du son ne répond plus : je continue sur la sortie de Windows.")
            try:
                self._ouvrir(sortie_windows=True)
            except Exception:  # noqa: BLE001
                log.exception("aucune sortie de son")
                self.vider()
                return False
        if reste:
            return True
        latence = self.flux.latency if self.flux is not None else 0.0
        return time.perf_counter() - self.dernier_son < latence + self._retard + 0.05

    def son_recent(self, secondes: float = 0.3) -> bool:
        """Du son est sorti il y a moins de `secondes` (+ le retard de l'enceinte) : son écho peut encore arriver."""
        return time.perf_counter() - self.dernier_son < secondes + self._retard

    def textes_recents(self) -> list:
        """La phrase qui sort des haut-parleurs et celle d'avant (pour reconnaître l'écho de sa propre voix)."""
        with self.verrou:
            if self.actuelle is None:
                return []
            return [p.texte for p in self.phrases[max(0, self.actuelle - 1):self.actuelle + 2] if p.texte]

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
        self.flux = None
