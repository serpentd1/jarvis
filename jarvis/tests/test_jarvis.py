"""Tests rapides de Jarvis (sans micro ni IA) : python -m pytest tests"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core.cerveau import STOP, commande_autour_du_mot, enlever_mot_activation  # noqa: E402
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


def test_commande_autour_du_mot():
    assert commande_autour_du_mot("blabla. Hey Jarvis, mets pause.") == "mets pause"
    assert commande_autour_du_mot("Mets pause, Jarvis.") == "Mets pause"


def test_commandes_stop():
    for texte in ("stop jarvis", "c'est bon merci"):
        assert STOP.match(normaliser(texte))
    for texte in ("stop la musique", "c'est bon à savoir"):
        assert not STOP.match(normaliser(texte))


def test_creneau():
    from datetime import datetime
    nuit = datetime(2026, 1, 1, 23, 0)
    jour = datetime(2026, 1, 1, 15, 0)
    assert dans_creneau("22:00-07:00", nuit)
    assert not dans_creneau("22:00-07:00", jour)
    assert dans_creneau("13:00-16:00", jour)
    assert not dans_creneau("", jour)


def test_decouper_phrases():
    import main
    classe = next(c for c in vars(main).values() if isinstance(c, type) and hasattr(c, "decouper_phrases"))
    d = classe.decouper_phrases
    assert d("Oui.") == ["Oui."]
    assert d("Il fait 12 degrés. Demain il pleut toute la journée sur Bordeaux, prends ton parapluie !") == [
        "Il fait 12 degrés. Demain il pleut toute la journée sur Bordeaux, prends ton parapluie !"]
    longues = d("Voici la première phrase, assez longue pour partir seule. Et la deuxième phrase, elle aussi bien longue.")
    assert longues == ["Voici la première phrase, assez longue pour partir seule.",
                       "Et la deuxième phrase, elle aussi bien longue."]


def test_masquer_codes():
    from modules import masquer_codes
    assert masquer_codes("Votre code est 482913.") == "Votre code est (code masqué)."
    assert masquer_codes("G-1234 est ton code Google", "••••") == "G-•••• est ton code Google"
    assert masquer_codes("Code WhatsApp : 123-456") == "Code WhatsApp : (code masqué)"
    # les numéros de téléphone, les heures et les petits nombres restent lisibles
    for texte in ("Rappelle-moi au 06 12 34 56 78", "ou au 0612345678", "rdv à 14:05", "j'ai eu 15/20"):
        assert masquer_codes(texte) == texte


def test_annonces_telephone():
    from modules.annonces import a_annoncer
    sms = {"source": "telephone", "type": "sms", "titre": "Maman", "texte": "Ton code : 4829. Tu rentres quand ?"}
    assert phrase(sms) == "SMS de Maman : Ton code : (code masqué). Tu rentres quand ?"
    assert phrase({"source": "telephone", "type": "appel", "titre": "Lucas", "texte": "Appel entrant"}) \
        == "Lucas t'appelle sur ton téléphone."
    assert phrase({"source": "telephone", "type": "appel", "titre": "Lucas", "texte": "Appel manqué"}) \
        == "Appel manqué de Lucas."
    assert phrase({"source": "telephone", "type": "batterie", "titre": "Batterie du téléphone",
                   "texte": "15 % restants"}) == "Ton téléphone n'a plus que 15 % de batterie."
    appli = {"source": "telephone", "type": "appli", "titre": "WhatsApp — Tom", "texte": "on se voit demain ?"}
    assert phrase(appli) == "WhatsApp sur ton téléphone, de Tom : on se voit demain ?"
    cfg = {"telephone": True, "telephone_applis": False, "discord": True}
    assert a_annoncer(cfg, sms) and not a_annoncer(cfg, appli)
    assert a_annoncer({**cfg, "telephone_applis": True}, appli)
    assert not a_annoncer({**cfg, "telephone": False}, sms)
    assert not a_annoncer(cfg, {"source": "jarvis"})


def test_contexte_telephone():
    from modules.givre_infos import resume
    e = {"telephone": [{"type": "sms", "de": "Maman", "texte": "Code 123456", "heure": "14:02", "appli": ""},
                       {"type": "appel", "de": "Lucas", "texte": "Appel manqué", "heure": "13:50", "appli": ""}]}
    texte = resume(e)
    assert "14:02 SMS de Maman : « Code (code masqué) »" in texte and "13:50 appel manqué de Lucas" in texte
    assert "123456" not in texte
    assert "TÉLÉPHONE : rien reçu" in resume({"telephone": {"connecte": False}})


def test_contexte_notes_masquees():
    from modules.givre_infos import resume
    texte = resume({"pronote": {"connecte": True, "notes_masquees": True,
                                "notes": [{"matiere": "Maths", "note": "19/20"}],
                                "moyenne_generale": "18,5/20"}})
    assert "masquer ses notes" in texte
    assert "19/20" not in texte and "18,5/20" not in texte
