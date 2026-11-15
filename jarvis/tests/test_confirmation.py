"""Les « oui », les « non », et le garde-fou contre l'IA qui prétend avoir agi."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core.cerveau import NON, OUI, PRETEND_AGIR  # noqa: E402
from modules import normaliser  # noqa: E402


def test_oui():
    for t in ["Très bien", "oui", "Ouais vas-y", "OK", "c'est bon", "Super", "nickel", "Ça marche", "Envoie-le",
              "Bien sûr", "parfait", "carrément", "Go"]:
        assert OUI.match(normaliser(t)), t


def test_non():
    for t in ["non", "Non merci", "annule", "laisse tomber", "attends", "n'envoie pas"]:
        assert NON.match(normaliser(t)), t
    for t in ["si tu peux, change le texte", "tu peux répéter ?"]:
        assert not OUI.match(normaliser(t)), t


def test_pretend_agir():
    for t in ["J'ai envoyé le message à Matt sur Discord.", "C'est fait !", "J'ouvre Chrome.", "Message envoyé.",
              "Je lance le minuteur.", "C'est envoyé."]:
        assert PRETEND_AGIR.search(normaliser(t)), t
    for t in ["Il fait 10 degrés.", "Tu as un devoir de maths pour lundi.", "Je ne sais pas encore faire ça."]:
        assert not PRETEND_AGIR.search(normaliser(t)), t
