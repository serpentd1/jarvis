"""Icône de Jarvis près de l'horloge : sa couleur montre l'état, son menu donne les commandes."""
import logging
import os
import subprocess

from PIL import Image, ImageDraw, ImageFont

from . import demarrage
from .config import FICHIER_CONFIG
from .journal import FICHIER as FICHIER_JOURNAL

log = logging.getLogger("icone")
COULEURS = {"chargement": (120, 120, 130), "repos": (40, 140, 255), "ecoute": (40, 205, 110),
            "reflechit": (255, 160, 30), "parle": (170, 90, 255), "muet": (225, 55, 65)}
LIBELLES = {"chargement": "démarre…", "repos": "prêt", "ecoute": "t'écoute", "reflechit": "réfléchit",
            "parle": "parle", "muet": "micro coupé"}


def dessiner(couleur, taille=64) -> Image.Image:
    img = Image.new("RGBA", (taille, taille), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.ellipse((2, 2, taille - 3, taille - 3), fill=couleur + (255,), outline=(255, 255, 255, 230), width=3)
    try:
        police = ImageFont.truetype("segoeuib.ttf", int(taille * 0.62))
    except OSError:
        police = ImageFont.load_default()
    d.text((taille / 2, taille / 2 + 1), "J", font=police, fill=(255, 255, 255, 255), anchor="mm")
    return img


class Icone:
    def __init__(self, etat, givre, cfg_general, voix_actuelle=None, changer_voix=None):
        import pystray
        from .tts import CATALOGUE
        self.etat, self.givre, self.cfg = etat, givre, cfg_general

        def action_voix(nom):
            return lambda: changer_voix(nom) if changer_voix else None

        def coche_voix(nom):
            return lambda _item: bool(voix_actuelle) and voix_actuelle() == nom

        menu_voix = pystray.Menu(*[pystray.MenuItem(v["titre"], action_voix(nom), checked=coche_voix(nom), radio=True)
                                   for nom, v in CATALOGUE.items()])
        menu = pystray.Menu(
            pystray.MenuItem("Parler maintenant", lambda: etat.ecouter_maintenant.set(), default=True),
            pystray.MenuItem("Micro activé", self._basculer_micro, checked=lambda _i: etat.micro),
            pystray.MenuItem("Voix", menu_voix),
            pystray.MenuItem("Démarrer avec Windows", self._basculer_demarrage,
                             checked=lambda _i: demarrage.est_active()),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Ouvrir la barre Givre", lambda: givre.lancer(cfg_general["givre_exe"],
                                                                           cfg_general["givre_arguments"])),
            pystray.MenuItem("Réglages (config.toml)", lambda: _ouvrir_texte(FICHIER_CONFIG)),
            pystray.MenuItem("Journal", lambda: _ouvrir_texte(FICHIER_JOURNAL)),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Redémarrer Jarvis", self._redemarrer),
            pystray.MenuItem("Quitter Jarvis", lambda: etat.quitter.set()),
        )
        self.icone = pystray.Icon("Jarvis", dessiner(COULEURS["chargement"]), "Jarvis — démarre…", menu)
        etat.abonner(lambda e, *_: self.maj(e))

    def demarrer(self):
        self.icone.run_detached()

    def maj(self, etat_nom: str = None):
        nom = "muet" if not self.etat.micro and (etat_nom or self.etat.etat) == "repos" else (etat_nom or self.etat.etat)
        try:
            self.icone.icon = dessiner(COULEURS.get(nom, COULEURS["repos"]))
            self.icone.title = f"Jarvis — {LIBELLES.get(nom, nom)}"
        except Exception:  # noqa: BLE001
            pass

    def arreter(self):
        try:
            self.icone.stop()
        except Exception:  # noqa: BLE001
            pass

    def _basculer_micro(self):
        self.etat.micro = not self.etat.micro
        self.maj()

    def _basculer_demarrage(self):
        demarrage.activer(not demarrage.est_active())

    def _redemarrer(self):
        self.etat.redemarrer = True
        self.etat.quitter.set()


def _ouvrir_texte(chemin):
    try:
        subprocess.Popen(["notepad.exe", str(chemin)])
    except OSError:
        os.startfile(chemin)
