"""Lien avec Givre (la barre / l'île en haut de l'écran), via son API locale 127.0.0.1:47800.
Si Givre n'est pas lancé, chaque appel échoue en silence et renvoie None : Jarvis marche aussi seul.
"""
import logging
import subprocess
import threading
import time
from pathlib import Path

import requests

from .config import RACINE

log = logging.getLogger("givre")
SANS_FENETRE = 0x08000000
DETACHE = 0x00000008 | 0x00000200  # DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP


CONNEXION = 0.3      # en local, une connexion est instantanée ; sous Windows un port fermé met ~2 s à répondre « non »
PAUSE_ABSENT = 10    # Givre absent : on ne réessaie pas avant 10 s (sinon chaque question attend)


class Givre:
    def __init__(self, url: str):
        self.url = url.rstrip("/")
        self._cache, self._cache_t = None, 0.0
        self._absent_jusqua = 0.0

    # ── lecture ──────────────────────────────────────────────
    def disponible(self) -> bool:
        return bool((self.etat(age_max=1) or {}).get("ok"))

    def etat(self, age_max: float = 2.0):
        """GET /etat (gardé 2 s en cache). None si Givre n'est pas lancé."""
        if self._cache is not None and time.time() - self._cache_t < age_max:
            return self._cache
        if time.time() < self._absent_jusqua:
            return None
        try:
            self._cache = requests.get(f"{self.url}/etat", timeout=(CONNEXION, 2)).json()
        except Exception:  # noqa: BLE001
            self._cache = None
            self._absent_jusqua = time.time() + PAUSE_ABSENT
        self._cache_t = time.time()
        return self._cache

    def mail(self, n: int):
        """Le contenu complet du mail n (même numérotation que /etat). None si indisponible."""
        r = self._appel("GET", f"/mail?n={int(n)}", None, 15)
        return r if r and r.get("ok") else None

    def notifs(self, depuis: int):
        r = self._appel("GET", f"/notifs?depuis={depuis}", None, 2)
        return r if r and r.get("ok") else None

    # ── actions ──────────────────────────────────────────────
    def minuteur(self, secondes: int, nom: str = ""):
        return self._appel("POST", "/minuteur", {"secondes": int(secondes), "nom": nom})

    def annuler_minuteur(self, id_=None, tous=False):
        return self._appel("POST", "/minuteur/annuler", {"tous": True} if tous else {"id": id_})

    def alarme(self, heure: str, active: bool = True):
        return self._appel("POST", "/alarme", {"heure": heure, "active": active})

    def chrono(self, commande: str):
        return self._appel("POST", "/chrono", {"commande": commande})

    def ouvrir(self, quoi: str, n: int = 0, section: str = ""):
        """quoi = "grande" déplie l'île en grand, sur `section` (taches, rappels, memoire, pc…) si donnée."""
        return self._appel("POST", "/ouvrir", {"quoi": quoi, "n": n, **({"section": section} if section else {})})

    def actualiser_meteo(self):
        return self._appel("POST", "/meteo/actualiser", {})

    def quitter(self):
        """Ferme la barre (île + widgets) : Jarvis et sa barre forment une seule appli."""
        self._absent_jusqua = 0
        return self._appel("POST", "/quitter", {}, 2)

    def notif(self, titre: str, texte: str, source: str = "jarvis"):
        self._en_fond("POST", "/notif", {"titre": titre, "texte": texte, "source": source})

    def etat_jarvis(self, etat: str, texte: str = "", question: str = "", reponse: str = "", duree: float = 0.0):
        """Montre dans l'île si Jarvis écoute, réfléchit ou parle (et la conversation).
        `duree` : secondes que dure la phrase dite, pour l'afficher mot par mot."""
        donnees = {"etat": etat, "texte": texte}
        if duree > 0:
            donnees["duree"] = round(duree, 2)
        if question or reponse:
            donnees.update(question=question, reponse=reponse)
        self._en_fond("POST", "/jarvis", donnees)

    # ── lancement ────────────────────────────────────────────
    def trouver_exe(self, chemin_config: str = ""):
        """La barre de Jarvis (l'île en haut de l'écran) : à côté de Jarvis une fois installé, sinon la version compilée.
        Givre, l'appli de widgets, est une autre appli : Jarvis ne s'en sert plus."""
        candidats = [Path(chemin_config)] if chemin_config else []
        candidats += [RACINE / "JarvisBarre.exe", RACINE / "barre" / "target" / "release" / "JarvisBarre.exe"]
        return next((c for c in candidats if c.is_file()), None)

    def lancer(self, chemin_config: str = "", arguments: str = "") -> bool:
        self._absent_jusqua = 0
        if self.disponible():
            return True
        self._absent_jusqua = 0          # la barre démarre : on veut la voir arriver tout de suite
        exe = self.trouver_exe(chemin_config)
        if not exe:
            log.info("Barre introuvable (JarvisBarre.exe) : je continue sans elle.")
            return False
        log.info("Je lance la barre : %s %s", exe, arguments)
        subprocess.Popen([str(exe), *arguments.split()], cwd=exe.parent, creationflags=DETACHE,
                         stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return True

    # ── interne ──────────────────────────────────────────────
    def _appel(self, methode: str, chemin: str, donnees, delai: float = 3):
        if time.time() < self._absent_jusqua:
            return None
        try:
            r = requests.request(methode, f"{self.url}{chemin}", json=donnees, timeout=(CONNEXION, delai))
            if r.status_code == 404:
                log.warning("Givre ne connaît pas encore %s %s (version trop ancienne ?)", methode, chemin)
                return None
            return r.json()
        except requests.ConnectionError:
            self._absent_jusqua = time.time() + PAUSE_ABSENT
            return None
        except Exception:  # noqa: BLE001
            return None

    def _en_fond(self, methode, chemin, donnees):
        threading.Thread(target=self._appel, args=(methode, chemin, donnees, 1.5), daemon=True).start()
