"""Le cerveau Rust (barre/src/cerveau.rs) vu depuis Python :
- le « registre » que la voix lui offre (raccourcis, outils, confirmations) ;
- la réponse reçue en direct, phrase par phrase, depuis une fausse barre ;
- le secours Python quand la barre ne répond pas."""
import json
import sys
import threading
import types
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core.cerveau import Cerveau  # noqa: E402
from modules import Registre, Resultat  # noqa: E402

CFG = {"ollama_url": "http://127.0.0.1:9", "modele": "x", "historique": 3, "contexte": 2048,
       "garder_en_memoire": "1m", "temperature": 0.1}
RECU = []


class FausseBarre(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def do_POST(self):
        RECU.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
        self.send_response(200)
        self.send_header("Content-Type", "application/x-ndjson")
        self.send_header("Connection", "close")
        self.end_headers()
        for ligne in ({"phrase": "Bonjour à toi."}, {"phrase": "Il fait beau."},
                      {"fin": True, "reponse": "Bonjour à toi. Il fait beau. À demain.", "reste": "À demain.",
                       "suivi": True}):
            self.wfile.write((json.dumps(ligne) + "\n").encode())
            self.wfile.flush()


def cerveau(url=None, simulation=False):
    ctx = types.SimpleNamespace(simulation=simulation, appels=[], cfg={"givre": {"url": url or "http://127.0.0.1:9"}})
    reg = Registre(ctx)
    fait = []
    reg.outil("dire_bonjour", "Dit bonjour.", {"nom": "prénom"}, ["nom"], lambda nom: Resultat(f"Bonjour {nom} !"))
    reg.outil("effacer", "Efface tout.", {}, [],
              lambda: Resultat("Tu confirmes ?", confirmer=lambda: fait.append(1) or Resultat("Effacé."),
                               annuler="Je ne touche à rien."))
    reg.outil("secret", "Outil caché.", {}, [], lambda: Resultat("chut"))
    reg.cacher_a_ia("secret")
    reg.regle(r"^salut (?P<nom>\w+)$", lambda m, t: [("dire_bonjour", {"nom": m["nom"]})])
    reg.intercepteur(lambda texte, n: "Révision en cours." if n == "reponse a" else None)
    return Cerveau(CFG, "Jarvis", reg), fait


def test_registre_pour_le_cerveau_rust():
    c, fait = cerveau()
    assert c.avant("Salut Léo") == {"appels": [["dire_bonjour", {"nom": "leo"}]]}
    assert c.avant("réponse A") == {"dire": "Révision en cours."}
    assert c.avant("Oublie tout") == {"oubli": True}
    assert c.avant("quel temps fait-il ?") == {}
    assert c.avant("salut léo et lance discord") == {}          # plusieurs ordres : c'est l'IA qui découpe
    assert [o["function"]["name"] for o in c.reg.schemas()] == ["dire_bonjour", "effacer"]   # pas l'outil caché
    assert c.executer_outil("dire_bonjour", {"nom": "Léa", "inconnu": 1}) == {
        "ok": True, "dire": "Bonjour Léa !", "annuler": "D'accord, j'annule."}
    question = c.executer_outil("effacer", {})
    assert question["dire"] == "Tu confirmes ?" and question["annuler"] == "Je ne touche à rien."
    assert not fait                                              # rien n'est fait avant le « oui »
    assert c.confirmer(question["confirmer"])["dire"] == "Effacé." and fait == [1]
    assert c.confirmer(question["confirmer"])["ok"] is False     # un « oui » ne sert qu'une fois
    assert c.executer_outil("nexiste_pas", {})["ok"] is False


def test_reponse_en_direct_depuis_la_barre():
    serveur = HTTPServer(("127.0.0.1", 0), FausseBarre)
    threading.Thread(target=serveur.serve_forever, daemon=True).start()
    try:
        c, _ = cerveau(f"http://127.0.0.1:{serveur.server_port}")
        c.coupe = "ce que je disais"
        phrases = []
        rep = c.repondre("Hey Jarvis, bonjour", "voix", sur_phrase=phrases.append)
        assert phrases == ["Bonjour à toi.", "Il fait beau."]
        assert (rep.texte, rep.reste, rep.suivi) == ("Bonjour à toi. Il fait beau. À demain.", "À demain.", True)
        chemin, corps = RECU[-1]
        assert chemin == "/cerveau" and corps["texte"] == "Hey Jarvis, bonjour" and corps["source"] == "voix"
        assert corps["coupe"] == "ce que je disais" and c.coupe is None
        assert corps["reglages"]["modele"] == "x" and corps["reglages"]["nom"] == "Jarvis"
        # sans « sur_phrase » (chat de l'île) : toute la réponse est à dire d'un coup
        rep = c.repondre("bonjour", "ile")
        assert rep.texte == "Bonjour à toi. Il fait beau. À demain." and rep.reste is None
    finally:
        serveur.shutdown()


def test_secours_python_si_la_barre_est_absente():
    c, _ = cerveau("http://127.0.0.1:9")             # personne n'écoute sur ce port
    rep = c.repondre("salut léo", "voix")            # le raccourci marche quand même, avec l'ancien cerveau
    assert rep.texte == "Bonjour leo !"
    assert c._barre_absente > 0                      # et on n'insiste pas tout de suite
    c2, _ = cerveau(simulation=True)                 # tests de Jarvis : jamais de barre
    assert c2._url_barre() is None
