"""Micro : flux continu en 16 kHz, découpé en blocs de 80 ms, + enregistrement d'une commande.

Avec l'anti-écho (core/sortie.py), la voix de Jarvis est retirée du son du micro dès son arrivée.
La fin de ta phrase est trouvée par le détecteur de voix Silero (core/vad.py), plus le volume."""
import logging
import queue
import time
from collections import deque

import numpy as np
import sounddevice as sd
from scipy.signal import resample_poly

log = logging.getLogger("micro")

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
    def __init__(self, nom_appareil: str = "", anti_echo=None):
        from .vad import Silero
        self.nom_appareil = nom_appareil           # gardé pour le retrouver si la liste des appareils change
        self.appareil = trouver_micro(nom_appareil)
        self.anti_echo = anti_echo
        self.vad = Silero()
        self.file = queue.Queue()
        self.voulu = False                  # le micro doit être ouvert (sert à le rouvrir s'il se bloque)
        self._dernier_recu = time.perf_counter()
        self._reste = np.zeros(0, dtype=np.int16)
        self.bruit = deque(maxlen=25)  # niveau sonore des 2 dernières secondes
        # les 15 dernières secondes du micro, avant et après l'anti-écho (pour « enregistrer_derniere_phrase »)
        self.histo_brut, self.histo_propre = deque(maxlen=200), deque(maxlen=200)
        try:
            self.freq_source = FREQ
            self.flux = sd.InputStream(samplerate=FREQ, channels=1, dtype="int16",
                                       blocksize=BLOC, device=self.appareil, callback=self._recu)
        except Exception:  # noqa: BLE001 — certains micros refusent 16 kHz
            self.freq_source = int(sd.query_devices(self.appareil, "input")["default_samplerate"])
            self.flux = sd.InputStream(samplerate=self.freq_source, channels=1, dtype="int16",
                                       device=self.appareil, callback=self._recu)
        nom = sd.query_devices(self.flux.device)["name"]
        if self.freq_source != FREQ:
            self.anti_echo = None                   # l'anti-écho travaille en 16 kHz
        print(f"[micro] {nom} ({self.freq_source} Hz){' + anti-écho' if self.anti_echo else ''}")

    def _recu(self, donnees, _frames, _temps, statut):
        self._dernier_recu = time.perf_counter()
        if statut.input_overflow:                    # du son perdu : la transcription peut se tromper
            self.perdus = getattr(self, "perdus", 0) + 1
        x = brut = donnees[:, 0].copy()
        if self.anti_echo is not None:
            try:
                x = self.anti_echo.nettoyer(brut)
            except Exception:  # noqa: BLE001
                pass
        self.histo_brut.append(brut)
        if len(x):
            self.histo_propre.append(x)
            self.file.put(x)

    def sauver_derniers(self, secondes: float, dossier):
        """Écrit les `secondes` dernières du micro dans derniere-phrase-brut.wav (le son du micro) et, avec
        l'anti-écho, derniere-phrase-propre.wav (ce qu'a reçu la transcription). Pour trouver une erreur d'écoute."""
        from scipy.io import wavfile
        n = int(secondes * self.freq_source)
        histos = [("brut", self.histo_brut)] + ([("propre", self.histo_propre)] if self.anti_echo is not None else [])
        for nom, histo in histos:
            blocs = list(histo)
            if blocs:
                wavfile.write(str(dossier / f"derniere-phrase-{nom}.wav"), self.freq_source, np.concatenate(blocs)[-n:])

    def demarrer(self):
        self.voulu = True
        if not self.flux.active:
            self._dernier_recu = time.perf_counter()
            self.flux.start()

    def fermer_flux(self):
        """Ferme le flux du micro (sans le recréer)."""
        try:
            self.flux.abort()
        except Exception:  # noqa: BLE001
            pass
        try:
            self.flux.close()
        except Exception:  # noqa: BLE001
            pass

    def rouvrir(self, demarrer: bool = True):
        """Le flux du micro s'est arrêté ou ne reçoit plus rien (ou la liste des appareils a été relue) : on le
        ferme et on le rouvre, en retrouvant le micro par son nom (son numéro a pu changer)."""
        if demarrer:
            log.warning("Le micro ne donne plus de son : je le rouvre.")
        self.fermer_flux()
        self.vider()
        try:
            if getattr(self, "nom_appareil", ""):
                self.appareil = trouver_micro(self.nom_appareil)
            kw = dict(samplerate=self.freq_source, channels=1, dtype="int16", device=self.appareil, callback=self._recu)
            if self.freq_source == FREQ:
                kw["blocksize"] = BLOC
            self.flux = sd.InputStream(**kw)
            self._dernier_recu = time.perf_counter()
            if demarrer:
                self.flux.start()
        except Exception:  # noqa: BLE001
            log.exception("Impossible de rouvrir le micro")

    def pause(self):
        """Ferme vraiment le micro (le voyant « micro utilisé » de Windows s'éteint)."""
        self.voulu = False
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
                # micro censé être ouvert mais plus aucun son depuis 2 s (flux mort, périphérique changé…)
                if self.voulu and (not self.flux.active or time.perf_counter() - self._dernier_recu > 2.0):
                    self.rouvrir()
                return None
            if self.freq_source != FREQ:
                brut = resample_poly(brut.astype(np.float32), FREQ, self.freq_source).astype(np.int16)
            self._reste = np.concatenate([self._reste, brut])
        bloc, self._reste = self._reste[:BLOC], self._reste[BLOC:]
        self.bruit.append(niveau(bloc))
        return bloc

    def enregistrer_commande(self, silence_fin=0.6, duree_max=20, attente_debut=6,
                             ignorer_debut=0.3, voix_min=0.24, volume_min=150.0, prefixe=None,
                             deja_dit_min=1.3, sur_voix=None) -> np.ndarray:
        """Enregistre ta phrase. Renvoie du float32 16 kHz (vide si tu n'as rien dit).

        - attend jusqu'à `attente_debut` s que tu commences à parler (seul le son juste avant ta voix est
          gardé : une longue attente ne ralentit pas la transcription) ;
        - « tu parles » = au moins `voix_min` s de VOIX (détecteur Silero) plus forte que `volume_min` et que le
          bruit de fond (un clic ou le « ding » ne comptent pas), sans compter les `ignorer_debut` premières secondes ;
        - s'arrête après `silence_fin` s sans voix (tu peux faire de petites pauses) ou `duree_max` s de parole ;
        - `deja_dit_min` : s'il y a déjà autant de voix dans `prefixe`, on n'attend presque pas la suite ;
        - `sur_voix()` est appelée une fois, dès que ta voix est reconnue (l'île montre alors « tu parles »).
        """
        def voix_reconnue():
            nonlocal sur_voix
            if sur_voix is not None:
                try:
                    sur_voix()
                except Exception:  # noqa: BLE001
                    log.exception("sur_voix")
                sur_voix = None

        # Bruit de fond : les blocs les plus calmes des 2 dernières secondes (et jamais plus de 500), pour que la
        # voix de Jarvis ou ta phrase précédente, restées dans l'historique, ne rendent pas le seuil inatteignable.
        fond = min(float(np.percentile(self.bruit, 20)), 500.0) if self.bruit else 200.0
        seuil = max(fond * 3, volume_min)
        plus_fort = proba_max = 0.0
        duree_bloc = BLOC / FREQ
        avant = deque(maxlen=max(1, int(0.8 / duree_bloc)))
        # `prefixe` : les ~2 s juste avant « Hey Jarvis ». S'il y a déjà une phrase dedans (« mets pause, Jarvis »),
        # on n'attend presque pas la suite.
        prefixe = list(prefixe or [])
        self.vad.reinitialiser()
        deja_dit = sum(duree_bloc for b in prefixe if self.vad.proba(b) >= 0.5 and niveau(b) > seuil) >= deja_dit_min
        if deja_dit:
            attente_debut = min(attente_debut, 1.0)
            voix_reconnue()
        blocs, parle, silence, temps, continu, parole = [], False, 0.0, 0.0, 0.0, 0.0
        while True:
            bloc = self.bloc()
            if bloc is None:
                break
            temps += duree_bloc
            p, n = self.vad.proba(bloc), niveau(bloc)
            plus_fort, proba_max = max(plus_fort, n), max(proba_max, p)
            # pour commencer : une vraie voix, assez forte ; ensuite : la voix continue tant qu'elle n'est pas
            # retombée (un peu moins exigeant, pour ne pas couper les fins de mots)
            fort = (p >= 0.35 and n > seuil * 0.5) if parle else (p >= 0.5 and n > seuil)
            if not parle:
                avant.append(bloc)
                continu = continu + duree_bloc if fort and temps > ignorer_debut else 0.0
                if continu >= voix_min:
                    parle, blocs = True, prefixe + list(avant)
                    voix_reconnue()
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
                log.info("Rien entendu : volume max %.0f (seuil %.0f), voix max %.2f, micro %s",
                         plus_fort, seuil, proba_max, "ouvert" if getattr(getattr(self, "flux", None), "active", True) else "FERMÉ")
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


def ding(volume: float = 0.22, aigu: bool = False, lecteur=None, nom_sortie: str = ""):
    """Petit son « je t'écoute » (un peu plus haut quand Jarvis attend une réponse oui/non).
    Il sort au même endroit que la voix de Jarvis : par son `lecteur` (l'anti-écho le connaît alors), sinon
    sur la sortie `nom_sortie` (vide = celle de Windows)."""
    if lecteur is not None:
        lecteur.jouer(*son_ding(aigu, volume))
        return
    from .sortie import jouer_simple
    jouer_simple(*son_ding(aigu, volume), nom_sortie)
