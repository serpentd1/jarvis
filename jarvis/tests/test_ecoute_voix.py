"""Tests de l'écoute (fin de phrase, attente, bruits) et du choix de voix — sans micro ni haut-parleur."""
import queue
import sys
from collections import deque
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core import config  # noqa: E402
from core.audio import BLOC, FREQ, Micro  # noqa: E402
from modules.voix import choisir  # noqa: E402

SILENCE = np.zeros(BLOC, np.int16)
VOIX = (np.random.default_rng(1).standard_normal(BLOC) * 2500).astype(np.int16)


class FausseVAD:
    """Le détecteur de voix, pour les tests : un son fort = une voix."""
    def reinitialiser(self):
        pass

    def proba(self, bloc):
        return 1.0 if np.abs(bloc).mean() > 500 else 0.0


def faux_micro(*morceaux):
    """morceaux : (bloc, secondes) -> un micro qui « entend » ces sons à la suite."""
    m = Micro.__new__(Micro)
    m.file, m._reste, m.freq_source = queue.Queue(), np.zeros(0, np.int16), FREQ
    m.vad, m.anti_echo = FausseVAD(), None
    m.bruit = deque([11.0] * 25, maxlen=25)
    for bloc, secondes in morceaux:
        for _ in range(round(secondes * FREQ / BLOC)):
            m.file.put(bloc)
    return m


def duree(audio):
    return len(audio) / FREQ


def test_attend_que_je_parle():
    # « Hey Jarvis »… 1, 2, 3… « lance Discord » : il attend 2,5 s sans couper
    audio = faux_micro((SILENCE, 2.5), (VOIX, 1.0), (SILENCE, 3)).enregistrer_commande()
    # L'attente avant la voix n'est plus conservée, contrairement au pré-déclencheur de 0,8 s.
    # 0,8 s avant la voix + 1 s de voix + 0,6 s de silence (fin de phrase)
    assert 2.0 < duree(audio) < 2.7


def test_pause_au_milieu_de_la_phrase():
    # une pause d'1 s au milieu ne coupe pas la phrase si silence_fin est plus long (réglage du micro)
    audio = faux_micro((VOIX, 1.0), (SILENCE, 1.0), (VOIX, 1.0), (SILENCE, 3)).enregistrer_commande(silence_fin=1.4)
    assert duree(audio) > 4.2


def test_fin_de_phrase_apres_600_ms():
    # par défaut, 0,6 s de silence termine la phrase : Jarvis répond vite
    audio = faux_micro((VOIX, 1.0), (SILENCE, 3)).enregistrer_commande()
    assert duree(audio) < 2.3


def test_un_clic_n_est_pas_de_la_voix():
    m = faux_micro((SILENCE, 0.5), (VOIX, 0.08), (SILENCE, 8))
    assert duree(m.enregistrer_commande()) == 0


def test_le_ding_est_ignore():
    # un son de 0,24 s pendant le « ding », puis le silence : rien n'est enregistré
    assert duree(faux_micro((VOIX, 0.24), (SILENCE, 8)).enregistrer_commande()) == 0


def test_silence_total():
    assert duree(faux_micro((SILENCE, 8)).enregistrer_commande()) == 0


def test_conversation_courte():
    # mode conversation : 5 s d'attente maximum, puis on revient à « Hey Jarvis »
    m = faux_micro((SILENCE, 6), (VOIX, 1.0), (SILENCE, 3))
    assert duree(m.enregistrer_commande(attente_debut=5, ignorer_debut=0)) == 0


def test_choix_de_voix():
    assert choisir("homme", "siwis_hd") == "tom"
    assert choisir("une voix d'homme", "tom") == "pierre"
    assert choisir("anglais", "tom") == "jarvis"
    assert choisir("", "siwis_hd") == "jarvis"
    assert choisir("femme", "tom") == "siwis_hd"
    assert choisir("pierre", "tom") == "pierre"
    assert choisir("la plus naturelle", "tom") == "siwis_hd"
    assert choisir("dark vador", "tom") is None


def test_ecrire_valeur(tmp_path, monkeypatch):
    f = tmp_path / "config.toml"
    f.write_text('[voix]\n# commentaire\nvoix = "tom"\nlenteur = 1.0\n\n[cerveau]\nmodele = "x"\n', encoding="utf-8")
    monkeypatch.setattr(config, "FICHIER_CONFIG", f)
    config.ecrire_valeur("voix", "voix", "jarvis")
    texte = f.read_text(encoding="utf-8")
    assert 'voix = "jarvis"' in texte and "# commentaire" in texte and 'modele = "x"' in texte
    config.ecrire_valeur("cerveau", "nouvelle", "oui")
    assert '[cerveau]\nnouvelle = "oui"' in f.read_text(encoding="utf-8")
