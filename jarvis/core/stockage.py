"""Petites bases de données JSON (tâches, rappels, habitudes, routines, mémoire…) dans %APPDATA%\\Jarvis\\donnees."""
import json
import os
import threading

from .config import DONNEES

DOSSIER = DONNEES / "donnees"


class Stockage:
    def __init__(self, nom: str, defaut):
        self.chemin = DOSSIER / f"{nom}.json"
        self.verrou = threading.RLock()
        self.donnees = defaut
        try:
            self.donnees = json.loads(self.chemin.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            pass

    def __enter__(self):
        self.verrou.acquire()
        return self.donnees

    def __exit__(self, *exc):
        try:
            if exc[0] is None:
                self.enregistrer()
        finally:
            self.verrou.release()

    def enregistrer(self):
        with self.verrou:
            DOSSIER.mkdir(parents=True, exist_ok=True)
            temporaire = self.chemin.with_suffix(".tmp")
            temporaire.write_text(json.dumps(self.donnees, ensure_ascii=False, indent=1), encoding="utf-8")
            os.replace(temporaire, self.chemin)
