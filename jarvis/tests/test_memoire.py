import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core import stockage  # noqa: E402
from modules import memoire  # noqa: E402


class RegistreTest:
    def __init__(self):
        self.apis = {}
        self.apres_echanges = []

    def api(self, nom, fonction):
        self.apis[nom] = fonction

    def contexte(self, _fonction):
        pass

    def apres_echange(self, fonction):
        self.apres_echanges.append(fonction)

    def outil(self, *_args):
        pass

    def regle(self, *_args, **_kwargs):
        pass


class ContexteTest:
    cfg = {"cerveau": {}}
    simulation = False


def installer_memoire(monkeypatch, tmp_path):
    monkeypatch.setattr(stockage, "DOSSIER", tmp_path / "donnees")
    registre = RegistreTest()
    memoire.installer(ContexteTest(), registre)
    return registre.apis["memoire"], registre.apres_echanges


def test_ajouter_notes_et_les_relier(monkeypatch, tmp_path):
    api, _ = installer_memoire(monkeypatch, tmp_path)
    assert api("POST", {"action": "ajouter_note", "nom": "Minecraft", "type": "objet"})["ok"]
    assert api("POST", {"action": "ajouter_note", "nom": "Tom", "type": "personne"})["ok"]

    resultat = api("POST", {"action": "lier", "a": "minecraft", "b": "tom", "relation": "joue avec"})
    assert resultat["ok"]
    assert any(l["a"] == "minecraft" and l["b"] == "tom" and l["relation"] == "joue avec"
               for l in resultat["liens"])

    resultat = api("POST", {"action": "lier", "a": "minecraft", "b": "minecraft", "relation": "identique"})
    assert not resultat["ok"]

    resultat = api("POST", {"action": "lier", "a": "minecraft", "b": "tom", "relation": "joue souvent avec"})
    assert resultat["ok"]
    assert sum(l["a"] == "minecraft" and l["b"] == "tom" for l in resultat["liens"]) == 1
    assert any(l["relation"] == "joue souvent avec" for l in resultat["liens"])

    resultat = api("POST", {"action": "supprimer_lien", "a": "minecraft", "b": "tom"})
    assert resultat["ok"] and all(l["a"] != "minecraft" or l["b"] != "tom" for l in resultat["liens"])
    assert not api("POST", {"action": "supprimer", "id": "moi"})["ok"]
    assert not api("POST", {"action": "inconnue"})["ok"]

    api("POST", {"action": "lier", "a": "minecraft", "b": "tom", "relation": "joue avec"})
    resultat = api("POST", {"action": "supprimer", "id": "tom"})
    assert all("tom" not in (l["a"], l["b"]) for l in resultat["liens"])


def test_memoire_refuse_les_secrets(monkeypatch, tmp_path):
    api, apres_echanges = installer_memoire(monkeypatch, tmp_path)
    resultat = api("POST", {"action": "ajouter_note", "nom": "Mot de passe GitHub: abc123"})
    assert not resultat["ok"]
    resultat = api("POST", {"action": "ajouter_note", "nom": "ghp_abcdefghijklmnopqrstuvwxyz0123456789"})
    assert not resultat["ok"]

    resultat = api("POST", {"action": "ajouter", "texte": "Mon code de vérification est 123456"})
    assert not resultat["ok"]

    monkeypatch.setattr(memoire.ollama, "discuter", lambda *_args, **_kwargs:
                        '{"faits":[{"sujet":"Moi","relation":"utilise","objet":"token GitHub abc123"},'
                        '{"sujet":"Moi","relation":"aime","objet":"Minecraft","type_objet":"objet"}]}')
    apres_echanges[0]("J'aime Minecraft et j'utilise GitHub", "D'accord.")
    donnees = api("GET", {})
    assert all("token" not in n["nom"].lower() for n in donnees["noeuds"])
    assert any(n["nom"] == "Minecraft" for n in donnees["noeuds"])
