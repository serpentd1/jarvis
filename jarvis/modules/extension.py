"""Aide à installer l'extension Chrome de Jarvis : « installe l'extension », « ouvre le dossier de l'extension »."""
import os
import subprocess
from pathlib import Path

from core.config import RACINE
from . import Resultat

DOSSIER = RACINE / "extension-chrome"
ETAPES = ("Dans la page des extensions, active le mode développeur en haut à droite, clique sur "
          "« Charger l'extension non empaquetée », choisis le dossier extension-chrome qui vient de s'ouvrir, "
          "puis recharge ton onglet Discord.")


def trouver_chrome():
    for base in (os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)"), os.environ.get("LOCALAPPDATA")):
        if base:
            chrome = Path(base) / "Google" / "Chrome" / "Application" / "chrome.exe"
            if chrome.is_file():
                return chrome
    return None


def installer(ctx, reg):
    def installer_extension_chrome():
        if ctx.pont.connecte:
            return Resultat("L'extension Chrome est déjà installée et connectée.")
        subprocess.Popen(["explorer.exe", str(DOSSIER)])
        chrome = trouver_chrome()
        if chrome:
            subprocess.Popen([str(chrome), "chrome://extensions/"])
            return Resultat("J'ai ouvert la page des extensions de Chrome et le dossier de l'extension. " + ETAPES)
        return Resultat(f"J'ai ouvert le dossier de l'extension. Dans Chrome, va sur la page chrome deux points slash "
                        f"slash extensions. {ETAPES}")

    reg.outil("installer_extension_chrome",
              "Aide à installer l'extension Chrome de Jarvis (nécessaire pour Discord et les vidéos) : ouvre la page "
              "des extensions de Chrome et le dossier de l'extension.", {}, [], installer_extension_chrome)
    reg.contexte(lambda: f"L'extension Chrome de Jarvis est dans le dossier {DOSSIER} ; pour aider à l'installer, "
                         "appelle l'outil installer_extension_chrome.")
    reg.regle(r"\b(installe|installer|ouvre|ouvrir|montre|trouve|ou est|est ou|charge|charger|ajoute)\b.*\bextension\b|"
              r"\bextension\b.*\b(installe|installer|dossier|ou est|est ou|chemin)\b",
              lambda m, t: [("installer_extension_chrome", {})], priorite=12)
