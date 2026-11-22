"""État partagé de Jarvis entre la voix, l'API locale, l'icône et les modules."""
import logging
import queue
import threading

log = logging.getLogger("etat")


class Etat:
    # ecoute = il t'écoute, tu peux parler ; parole = il entend ta voix ; parle_ecoute = il finit sa phrase
    # pendant que tu lui parles par-dessus (il t'écoute en même temps)
    ETATS = ("chargement", "repos", "ecoute", "parole", "reflechit", "parle", "parle_ecoute")

    def __init__(self):
        self.etat = "chargement"
        self.micro = True
        self.dernier_texte = ""
        self.duree = 0.0                              # secondes que met Jarvis à dire le texte en cours
        self.toi = ""                                 # ta phrase, envoyée avec ce changement (affichée dans l'île)
        self.ecouter_maintenant = threading.Event()   # bouton « parler » (île, icône)
        self.quitter = threading.Event()
        self.redemarrer = False
        self.couper_voix = False                      # « Couper la voix » : le moteur s'arrête, la barre reste
        self.annonces = queue.Queue()                 # phrases à dire dès que Jarvis est libre
        self.selection = None                         # ce que tu regardes dans l'île (ex : un mail), avec l'heure
        self._abonnes = []

    def abonner(self, fonction):
        """fonction(etat, texte, question, reponse) appelée à chaque changement."""
        self._abonnes.append(fonction)

    def changer(self, etat: str, texte: str = "", question: str = "", reponse: str = "", duree: float = 0.0,
                toi: str = ""):
        """`duree` : temps que met Jarvis à dire `texte` (l'île l'affiche alors mot par mot).
        `toi` : ce que l'utilisateur vient de dire (l'île l'affiche au-dessus de la réponse de Jarvis)."""
        self.etat, self.duree, self.toi = etat, duree, toi
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
