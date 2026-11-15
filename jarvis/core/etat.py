"""État partagé de Jarvis entre la voix, l'API locale, l'icône et les modules."""
import logging
import queue
import threading

log = logging.getLogger("etat")


class Etat:
    ETATS = ("chargement", "repos", "ecoute", "reflechit", "parle")

    def __init__(self):
        self.etat = "chargement"
        self.micro = True
        self.dernier_texte = ""
        self.ecouter_maintenant = threading.Event()   # bouton « parler » (île, icône)
        self.quitter = threading.Event()
        self.redemarrer = False
        self.annonces = queue.Queue()                 # phrases à dire dès que Jarvis est libre
        self.selection = None                         # ce que tu regardes dans l'île (ex : un mail), avec l'heure
        self._abonnes = []

    def abonner(self, fonction):
        """fonction(etat, texte, question, reponse) appelée à chaque changement."""
        self._abonnes.append(fonction)

    def changer(self, etat: str, texte: str = "", question: str = "", reponse: str = ""):
        self.etat = etat
        if texte:
            self.dernier_texte = texte
        for f in self._abonnes:
            try:
                f(etat, texte, question, reponse)
            except Exception:  # noqa: BLE001
                log.exception("abonné en erreur")

    def annoncer(self, phrase: str):
        if phrase:
            self.annonces.put(phrase)
