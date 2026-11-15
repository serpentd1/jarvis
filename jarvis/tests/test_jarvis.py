"""Tests rapides de Jarvis (sans micro ni IA) : python -m pytest tests"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core.cerveau import enlever_mot_activation  # noqa: E402
from modules import duree_en_secondes, nombre, normaliser  # noqa: E402
from modules.annonces import dans_creneau, phrase  # noqa: E402
from modules.minuteurs import dire_duree, heure_depuis_texte  # noqa: E402


def test_nombres():
    assert nombre("5") == 5
    assert nombre("2,5") == 2.5
    assert nombre("cinq") == 5
    assert nombre("vingt et un") == 21
    assert nombre("quatre-vingt-dix") == 90
    assert nombre("soixante-dix") == 70
    assert nombre("de cinq") is None


def test_durees():
    assert duree_en_secondes("mets un minuteur de 5 minutes pour les pâtes") == 300
    assert duree_en_secondes("minuteur de cinq minutes") == 300
    assert duree_en_secondes("une heure et demie") == 5400
    assert duree_en_secondes("1h30") == 5400
    assert duree_en_secondes("90 secondes") == 90
    assert duree_en_secondes("deux minutes et demie") == 150
    assert duree_en_secondes("un quart d'heure") == 900
    assert duree_en_secondes("une demi-heure") == 1800
    assert duree_en_secondes("1 heure 20 minutes") == 4800
    assert duree_en_secondes("trois quarts d'heure") == 2700
    assert duree_en_secondes("lance Discord") is None


def test_heures():
    assert heure_depuis_texte("réveille-moi à 7h30") == "07:30"
    assert heure_depuis_texte("mets une alarme à 7 heures") == "07:00"
    assert heure_depuis_texte("réveille-moi à sept heures et demie") == "07:30"
    assert heure_depuis_texte("alarme à huit heures quinze") == "08:15"
    assert heure_depuis_texte("alarme à 9 heures moins le quart") == "08:45"
    assert heure_depuis_texte("alarme à midi") == "12:00"
    assert heure_depuis_texte("à 6 heures du soir") == "18:00"


def test_divers():
    assert normaliser("Lance-moi Discord !") == "lance moi discord"
    assert enlever_mot_activation("Hey Jarvis, lance Discord") == "lance Discord"
    assert enlever_mot_activation("Jarvis. Mets pause") == "Mets pause"
    assert enlever_mot_activation("Mets pause") == "Mets pause"
    assert dire_duree(300) == "5 minutes"
    assert dire_duree(5400) == "1 heure et 30 minutes"
    assert dire_duree(61) == "1 minute et 1 seconde"
    assert phrase({"source": "discord", "titre": "Tom", "texte": "salut"}) == "Message Discord de Tom : salut"


def test_creneau():
    from datetime import datetime
    nuit = datetime(2026, 1, 1, 23, 0)
    jour = datetime(2026, 1, 1, 15, 0)
    assert dans_creneau("22:00-07:00", nuit)
    assert not dans_creneau("22:00-07:00", jour)
    assert dans_creneau("13:00-16:00", jour)
    assert not dans_creneau("", jour)
