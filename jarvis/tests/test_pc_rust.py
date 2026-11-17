"""Le pont Python -> pouvoirs Rust (JarvisBarre) : outils chargés depuis /outils, « oui » transmis à /outil/confirmer.
Une fausse barre (petit serveur HTTP) remplace JarvisBarre.exe."""
import json
import sys
import threading
import types
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

import modules  # noqa: E402
from modules import pc_rust  # noqa: E402

APPELS = []


class FausseBarre(BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def _repondre(self, donnees):
        corps = json.dumps(donnees).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(corps)))
        self.end_headers()
        self.wfile.write(corps)

    def do_GET(self):
        self._repondre({"ok": True, "outils": [
            {"type": "function", "function": {"name": "supprimer", "description": "corbeille",
                                              "parameters": {"properties": {"chemin_element": {"type": "string"}},
                                                             "required": ["chemin_element"]}}},
            {"type": "function", "function": {"name": "lancer_programme", "description": "x",
                                              "parameters": {"properties": {}, "required": []}}}],
            "regles": [{"motif": r"\balarme\b.*\bsur mon telephone\b", "outil": "alarme_telephone",
                        "args": {"heure": "$$"}, "priorite": 5},
                       {"motif": r"^mets? mon telephone en (?P<mode>silencieux|vibreur)", "outil": "mode_telephone",
                        "args": {"mode": "$mode"}, "priorite": 5}]})

    def do_POST(self):
        d = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        APPELS.append((self.path, d))
        if self.path == "/outil":
            self._repondre({"ok": True, "dire": "Je mets le fichier a.txt à la corbeille ?", "confirmer": 7})
        else:
            self._repondre({"ok": True, "dire": "C'est fait, a.txt est dans la corbeille."})


def test_pont_vers_rust():
    serveur = HTTPServer(("127.0.0.1", 0), FausseBarre)
    threading.Thread(target=serveur.serve_forever, daemon=True).start()
    ctx = types.SimpleNamespace(cfg={"givre": {"url": f"http://127.0.0.1:{serveur.server_port}"}}, simulation=False, appels=[])
    reg = modules.Registre(ctx)
    reg.outil("lancer_application", "déjà là", {"nom": "nom"}, ["nom"], lambda nom: None)
    pc_rust.installer(ctx, reg)
    for t in reg.taches:
        t()
    assert "supprimer" in reg.outils
    assert "lancer_programme" not in reg.outils          # doublon de lancer_application : pas ajouté
    r = reg.executer("supprimer", {"chemin_element": "a.txt", "inconnu": 1})
    assert r.confirmer and "corbeille" in r.dire
    assert APPELS[-1] == ("/outil", {"nom": "supprimer", "args": {"chemin_element": "a.txt"}})
    fait = r.confirmer()
    assert fait.ok and "corbeille" in fait.dire
    assert APPELS[-1] == ("/outil/confirmer", {"id": 7, "oui": True})
    # raccourcis vocaux venus du Rust
    texte = "Mets une alarme à 7h30 sur mon téléphone"
    n = modules.normaliser(texte)
    regle = next(r for r in reg.regles if r.motif.search(n))
    assert regle.fonction(regle.motif.search(n), texte) == [("alarme_telephone", {"heure": texte})]
    n = modules.normaliser("Mets mon téléphone en silencieux")
    regle = next(r for r in reg.regles if r.motif.search(n))
    assert regle.fonction(regle.motif.search(n), n) == [("mode_telephone", {"mode": "silencieux"})]
    serveur.shutdown()
