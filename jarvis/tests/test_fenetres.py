"""Outil général des fenêtres : la phrase est bien comprise, quelle que soit l'appli ou la page."""
import pytest

from modules import normaliser
from modules.fenetres import comprendre


@pytest.mark.parametrize("phrase, attendu", [
    ("Mets en petit la page qui est sur l'écran", ("reduire", "")),
    ("Mets-la en petit", ("reduire", "")),
    ("Réduis la fenêtre", ("reduire", "")),
    ("Réduis Discord", ("reduire", "discord")),
    ("Minimise Chrome", ("reduire", "chrome")),
    ("Agrandis la fenêtre", ("agrandir", "")),
    ("Mets la page en grand", ("agrandir", "")),
    ("Mets YouTube en plein écran", ("plein_ecran", "youtube")),
    ("Remets-la", ("restaurer", "")),
    ("Restaure Discord", ("restaurer", "discord")),
    ("Ferme la fenêtre", ("fermer", "")),
    ("Ferme la page qui est sur l'écran", ("fermer", "")),
    ("Montre-moi le bureau", ("tout_reduire", "")),
    ("Mets la fenêtre à gauche", ("gauche", "")),
])
def test_compris(phrase, attendu):
    assert comprendre(normaliser(phrase)) == attendu


@pytest.mark.parametrize("phrase", [
    "Ferme Discord",                      # fermer_application s'en charge
    "Réduis le son",                      # le volume, pas une fenêtre
    "Ferme les onglets en double",
    "Raconte-moi une blague",
    "Mets la musique un peu plus fort",
])
def test_pas_une_fenetre(phrase):
    assert comprendre(normaliser(phrase)) is None
