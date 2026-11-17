"""Ton téléphone, relié au PC par « Mobile connecté » : Givre reçoit ses SMS, appels et notifications,
Jarvis les annonce (modules/annonces.py) et les connaît (modules/givre_infos.py). Ici : le retrouver quand il est perdu.
"""
import os

from . import Resultat

LOCALISER = "https://www.google.com/android/find"


def installer(ctx, reg):
    def faire_sonner_telephone():
        os.startfile(LOCALISER)
        return Resultat("J'ouvre « Localiser mon appareil » de Google : clique sur « Faire sonner ». "
                        "Ton téléphone sonnera même s'il est en silencieux.")

    reg.outil("faire_sonner_telephone", "Aide à retrouver le téléphone perdu : ouvre la page de Google qui le fait sonner.",
              {}, [], faire_sonner_telephone)
    reg.regle(r"^(?:fais|fait|faire) sonner (?:mon|le) (?:telephone|portable|tel|smartphone)$"
              r"|^(?:ou est|j'ai perdu|je trouve plus|je ne trouve plus|aide moi a retrouver|retrouve) "
              r"(?:mon|le) (?:telephone|portable|tel|smartphone)$",
              lambda m, t: [("faire_sonner_telephone", {})], priorite=30)
