"""Sortie du son de Jarvis (enceinte choisie par son nom), retard Bluetooth pour l'anti-écho, et « stop » tout
seul pendant qu'il parle — sans vraie carte son, sans micro, sans IA."""
import sys
import threading
import types
from pathlib import Path

import numpy as np
import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core import config, sortie  # noqa: E402
from core.audio import BLOC  # noqa: E402
from modules import est_stop  # noqa: E402

# ── les mots d'arrêt ──────────────────────────────────────────


@pytest.mark.parametrize("texte", ["stop", "Stop !", "arrête", "arrête-toi", "tais-toi", "c'est bon", "ok stop",
                                   "merci c'est tout", "Ça suffit.", "Chut !", "Jarvis, stop.", "Stop, stop.",
                                   "C est bon", "Hey Jarvis, tais-toi", "c'est bon merci", "Non, arrête !"])
def test_mots_d_arret(texte):
    assert est_stop(texte)


@pytest.mark.parametrize("texte", ["arrête la musique", "stop la vidéo", "c'est bon pour demain ?", "mets un minuteur",
                                   "", "merci", "Jarvis", "ok", "c'est bon à savoir", "arrête le minuteur"])
def test_vraies_demandes(texte):
    assert not est_stop(texte)


# ── le choix de la sortie, sur une fausse liste d'appareils ───

def appareil(nom, api, sorties=2, entrees=0):
    return {"name": nom, "hostapi": api, "max_output_channels": sorties, "max_input_channels": entrees}


APIS = [{"name": "MME", "default_output_device": 3}, {"name": "Windows DirectSound", "default_output_device": 5},
        {"name": "Windows WASAPI", "default_output_device": 7}, {"name": "Windows WDM-KS", "default_output_device": 8}]
APPAREILS = [
    appareil("Mappeur de sons Microsoft - Input", 0, 0, 2),
    appareil("Microphone (WO Mic Device)", 0, 0, 1),
    appareil("Mappeur de sons Microsoft - Output", 0),
    appareil("Haut-parleurs (JBL Flip Essenti", 0),                 # 3 : MME coupe les noms à 31 lettres
    appareil("Haut-parleurs (Razer BlackShark", 0),                 # 4
    appareil("Haut-parleurs (JBL Flip Essential)", 1),              # 5 : DirectSound
    appareil("Haut-parleurs (JBL Flip Essential)", 2),              # 6 : WASAPI
    appareil("Haut-parleurs (Razer BlackShark V2 HS 2.4)", 2),      # 7 : WASAPI, sortie de Windows
    appareil("Haut-parleurs (JBL Flip Essential)", 3),              # 8 : WDM-KS, jamais pris
]


def test_sortie_wasapi_d_abord():
    assert sortie.trouver_sortie("JBL", APPAREILS, APIS) == 6
    assert sortie.trouver_sortie("jbl flip", APPAREILS, APIS) == 6        # majuscules ou pas
    assert [i for i, _ in sortie.candidats_sortie("JBL", APPAREILS, APIS)] == [6, 3, 5]


def test_sortie_mme_sans_wasapi():
    sans_wasapi = [a for a in APPAREILS if a["hostapi"] != 2]
    assert sortie.trouver_sortie("JBL", sans_wasapi, APIS) == sans_wasapi.index(APPAREILS[3])


def test_sortie_introuvable_ou_vide():
    assert sortie.trouver_sortie("Bose", APPAREILS, APIS) is None         # enceinte éteinte : sortie de Windows
    assert sortie.trouver_sortie("", APPAREILS, APIS) is None
    assert sortie.trouver_sortie("WO Mic", APPAREILS, APIS) is None       # un micro n'est pas une sortie
    seulement_wdm = [appareil("Haut-parleurs (JBL Flip Essential)", 3)]
    assert sortie.trouver_sortie("JBL", seulement_wdm, APIS) is None


def test_present_dans_windows_noms_coupes():
    noms = ["Haut-parleurs (JBL Flip Essenti", "Haut-parleurs (Razer BlackShark"]
    assert sortie._present("JBL", noms)
    assert sortie._present("JBL Flip Essential", noms)                    # Windows a coupé le nom
    assert not sortie._present("Bose", noms)


# ── le retard automatique de l'enceinte ───────────────────────

def test_delai_automatique():
    assert sortie.delai_sortie_ms(0, "Haut-parleurs (JBL Flip Essential)") == 200
    assert sortie.delai_sortie_ms(0, "Bose SoundLink Mini") == 200
    assert sortie.delai_sortie_ms(0, "Casque Bluetooth") == 200
    assert sortie.delai_sortie_ms(0, "", "JBL") == 200                    # le nom réglé suffit
    assert sortie.delai_sortie_ms(0, "Haut-parleurs (Razer BlackShark V2 HS 2.4)") == 0
    assert sortie.delai_sortie_ms(0, "Haut-parleurs (Realtek(R) Audio)") == 0
    assert sortie.delai_sortie_ms(0, "Google Nest") == 0                  # « go » doit être un mot entier
    assert sortie.delai_sortie_ms(0, "") == 0


def test_delai_regle_a_la_main():
    assert sortie.delai_sortie_ms(300, "Haut-parleurs (Realtek(R) Audio)") == 300
    assert sortie.delai_sortie_ms("250", "JBL") == 250                    # écrit en texte par la grande appli
    assert sortie.delai_sortie_ms("n'importe quoi", "JBL") == 200


def test_reglages_par_defaut():
    assert config.DEFAUTS["voix"]["sortie"] == ""
    assert config.DEFAUTS["conversation"]["delai_sortie_ms"] == 0
    from modules.parametres import REGLAGES
    lignes = {(s, c): t for s, c, t, _ in REGLAGES}
    assert lignes[("voix", "sortie")] == "texte"
    assert lignes[("micro", "appareil")] == "texte"
    assert lignes[("conversation", "delai_sortie_ms")] == "nombre"


def test_anti_echo_garde_500_ms_au_plus():
    vus = []
    ae = sortie.AntiEcho.__new__(sortie.AntiEcho)
    ae.apm = types.SimpleNamespace(set_stream_delay_ms=vus.append)
    ae.regler_delai(0.27)
    ae.regler_delai(0.9)
    assert vus == [270, 500]


# ── le Lecteur, avec une fausse carte son ─────────────────────

class FauxFlux:
    refus = set()

    def __init__(self, device=None, extra_settings=None, **_):
        if device in FauxFlux.refus:
            raise RuntimeError("Invalid sample rate")
        self.device, self.extra, self.latency, self.active = device, extra_settings, 0.04, False

    def start(self):
        self.active = True

    def abort(self):
        self.active = False

    def stop(self):
        self.active = False

    def close(self):
        pass


@pytest.fixture
def carte_son(monkeypatch):
    liste = {"appareils": list(APPAREILS)}

    def query_devices(i=None, kind=None):
        return liste["appareils"] if i is None else liste["appareils"][i]

    monkeypatch.setattr(sortie.sd, "query_devices", query_devices)
    monkeypatch.setattr(sortie.sd, "query_hostapis", lambda: APIS)
    monkeypatch.setattr(sortie.sd, "OutputStream", FauxFlux)
    monkeypatch.setattr(sortie.sd, "WasapiSettings", lambda auto_convert=False: ("wasapi", auto_convert))
    monkeypatch.setattr(sortie, "sorties_windows", lambda: [])
    FauxFlux.refus = set()
    return liste


class FauxAntiEcho:
    def __init__(self):
        self.delais = []

    def regler_delai(self, s):
        self.delais.append(s)


def test_lecteur_sur_l_enceinte(carte_son):
    ae = FauxAntiEcho()
    lecteur = sortie.Lecteur(ae, nom_sortie="JBL")
    assert lecteur.flux.device == 6 and lecteur.flux.extra == ("wasapi", True)   # Windows convertit nos 24 kHz
    assert lecteur.sur_voulue and lecteur.delai_ms() == 200
    lecteur.latence_micro = 0.03
    lecteur.regler_anti_echo()
    assert ae.delais[-1] == pytest.approx(0.03 + 0.04 + 0.2)                     # micro + carte son + Bluetooth


def test_lecteur_wasapi_refuse_puis_mme(carte_son):
    FauxFlux.refus = {6}
    lecteur = sortie.Lecteur(nom_sortie="JBL")
    assert lecteur.flux.device == 3 and lecteur.flux.extra is None and lecteur.sur_voulue


def test_lecteur_enceinte_eteinte_puis_rallumee(carte_son, monkeypatch):
    lecteur = sortie.Lecteur(nom_sortie="Bose", delai_ms=0)
    assert lecteur.flux.device is None and not lecteur.sur_voulue               # sortie de Windows, sans planter
    assert lecteur.nom_reel.startswith("Haut-parleurs (Razer") and lecteur.delai_ms() == 0
    lecteur.verifier_sortie()                                                    # toujours éteinte : rien ne change
    assert lecteur.flux.device is None
    # l'enceinte s'allume : Windows la voit, PortAudio pas encore (sa liste date du démarrage)
    rallumee = APPAREILS + [appareil("Haut-parleurs (Bose SoundLink)", 2)]
    monkeypatch.setattr(sortie, "sorties_windows", lambda: ["Haut-parleurs (Bose SoundLink)"])
    relu = []
    micro = types.SimpleNamespace(flux=types.SimpleNamespace(active=True, latency=0.02), ferme=False)
    micro.fermer_flux = lambda: setattr(micro, "ferme", True)
    micro.rouvrir = lambda demarrer=True: relu.append(demarrer)
    monkeypatch.setattr(sortie.sd, "_terminate", lambda: carte_son.__setitem__("appareils", rallumee))
    monkeypatch.setattr(sortie.sd, "_initialize", lambda: None)
    monkeypatch.setattr(sortie.sd, "_last_callback", None, raising=False)
    lecteur._derniere_recherche = 0.0
    lecteur.verifier_sortie(micro)
    assert lecteur.flux.device == len(APPAREILS) and lecteur.sur_voulue
    assert micro.ferme and relu == [True]                                        # le micro est rouvert, allumé


def test_lecteur_ding_par_la_meme_sortie(carte_son):
    lecteur = sortie.Lecteur(nom_sortie="JBL")
    lecteur.jouer(np.zeros(100, np.float32), 24000, attendre=False)
    assert lecteur.morceaux and lecteur.phrases[-1].texte == ""


# ── « stop » pendant qu'il parle (fonction parler de main.py) ──

VOIX = (np.random.default_rng(3).standard_normal(BLOC) * 2500).astype(np.int16)
SILENCE = np.zeros(BLOC, np.int16)


class FauxLecteurParle:
    """Jarvis « parle » pendant `secondes` : chaque bloc du micro fait avancer le temps de 80 ms."""

    def __init__(self, secondes=4.0, phrase="Il fait beau aujourd'hui."):
        self.restant, self.vide, self.premier_son, self.phrase = int(secondes / 0.08), False, None, phrase

    def verifier_sortie(self, micro=None): pass
    def reinitialiser(self): pass
    def nouvelle_phrase(self, t): return 0
    def ajouter(self, *a): pass
    def fin_phrase(self, i): pass
    def occupe(self): return self.restant > 0 and not self.vide
    def phrase_en_cours(self): return (0, self.phrase, 3.0)
    def son_recent(self, s=0.3): return self.restant > 0
    def dit(self): return self.phrase
    def textes_recents(self): return [self.phrase]

    def vider(self):
        self.vide = True


class FauxMicroParle:
    def __init__(self, lecteur, *morceaux):
        self.lecteur, self.blocs = lecteur, []
        for bloc, secondes in morceaux:
            self.blocs += [bloc] * round(secondes / 0.08)
        self.flux = types.SimpleNamespace(active=True)
        self.vad = types.SimpleNamespace(reinitialiser=lambda: None,
                                         proba=lambda b: 1.0 if np.abs(b).mean() > 500 else 0.0)

    def bloc(self, _delai):
        self.lecteur.restant -= 1
        return self.blocs.pop(0) if self.blocs else SILENCE

    def vider(self): pass


def jarvis_qui_parle(entendu, casque=False, phrase="Il fait beau aujourd'hui."):
    import main
    j = main.Jarvis.__new__(main.Jarvis)
    j.cfg = {section: dict(valeurs) for section, valeurs in config.DEFAUTS.items()}
    j.cfg["micro"].update(interrompre=True, couper_en_parlant=True, volume_conversation=350, silence_fin=0.6)
    j.cfg["conversation"].update(mode_casque=casque, couper_apres=0.3, seuil_voix_pendant=0.85)
    j.lecteur = FauxLecteurParle(phrase=phrase)
    j.micro = FauxMicroParle(j.lecteur, (SILENCE, 1.0), (VOIX, 0.5), (SILENCE, 2.0))
    j.mot = types.SimpleNamespace(score=lambda b: 0.0, seuil=0.5, reinitialiser=lambda: None)
    j.voix = types.SimpleNamespace(flux=lambda t: iter([(np.zeros(10, np.float32), 24000)]))
    j.stt = types.SimpleNamespace(transcrire=lambda audio: entendu)
    j.etat = types.SimpleNamespace(changer=lambda *a, **k: None, quitter=threading.Event(), etat="parle")
    j.stop_entendu = False
    return j


def test_stop_pendant_qu_il_parle_enceinte():
    j = jarvis_qui_parle("Stop !")
    assert j.parler("Il fait beau aujourd'hui.") is True
    assert j.stop_entendu and j.lecteur.vide and j.phrase_coupure is None


def test_autre_phrase_ignoree_avec_enceinte():
    j = jarvis_qui_parle("arrête la musique")
    assert j.parler("Il fait beau aujourd'hui.") is False                    # il continue jusqu'au bout
    assert not j.stop_entendu and not j.lecteur.vide


def test_son_propre_echo_ne_le_coupe_pas():
    j = jarvis_qui_parle("C'est bon.", phrase="C'est bon, j'ai mis le minuteur.")
    assert j.parler("C'est bon, j'ai mis le minuteur.") is False
    assert not j.stop_entendu


def test_casque_n_importe_quelle_phrase_le_coupe():
    j = jarvis_qui_parle("mets un minuteur", casque=True)
    assert j.parler("Il fait beau aujourd'hui.") is True
    assert j.phrase_coupure == "mets un minuteur" and not j.stop_entendu


def test_casque_stop_finit_la_conversation():
    j = jarvis_qui_parle("tais-toi", casque=True)
    assert j.parler("Il fait beau aujourd'hui.") is True
    assert j.stop_entendu and j.phrase_coupure is None
