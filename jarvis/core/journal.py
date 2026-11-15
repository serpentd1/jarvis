"""Journal de Jarvis : %APPDATA%\\Jarvis\\jarvis.log (et la console quand il y en a une)."""
import logging
import logging.handlers
import sys

from .config import DONNEES

FICHIER = DONNEES / "jarvis.log"


class _VersJournal:
    """Remplace la console quand Jarvis tourne sans fenêtre (pythonw) : tout va dans le journal."""

    encoding = "utf-8"

    def __init__(self, niveau):
        self.niveau = niveau

    def write(self, texte):
        texte = texte.rstrip()
        if texte and "\r" not in texte:          # (pas les barres de progression)
            logging.getLogger("console").log(self.niveau, texte)
        return len(texte)

    def flush(self):
        pass

    def isatty(self):
        return False


def configurer(sans_console: bool):
    DONNEES.mkdir(parents=True, exist_ok=True)
    fichier = logging.handlers.RotatingFileHandler(FICHIER, maxBytes=1_000_000, backupCount=2, encoding="utf-8")
    fichier.setFormatter(logging.Formatter("%(asctime)s %(levelname)-7s %(name)s : %(message)s", "%d/%m %H:%M:%S"))
    gestionnaires = [fichier]
    if sans_console or sys.stdout is None:
        sys.stdout = _VersJournal(logging.INFO)
        sys.stderr = _VersJournal(logging.ERROR)
    else:
        sys.stdout.reconfigure(encoding="utf-8")
        console = logging.StreamHandler(sys.stdout)
        console.setFormatter(logging.Formatter("%(message)s"))
        gestionnaires.append(console)
    logging.basicConfig(level=logging.INFO, handlers=gestionnaires, force=True)
    for bruyant in ("aiohttp.access", "urllib3", "faster_whisper", "comtypes", "PIL", "httpx", "huggingface_hub"):
        logging.getLogger(bruyant).setLevel(logging.WARNING)
