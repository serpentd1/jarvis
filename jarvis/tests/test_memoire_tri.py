"""Tri de la mémoire : suggestions de nettoyage, suppression multiple, garder, renommer, annuler, commandes vocales.
Tout se passe dans un dossier temporaire : les vraies données de l'utilisateur ne sont jamais touchées."""
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from core import stockage  # noqa: E402
from modules import memoire, normaliser  # noqa: E402


class Registre:
    def __init__(self):
        self.apis, self.outils, self.regles = {}, {}, []

    def api(self, nom, fonction):
        self.apis[nom] = fonction

    def contexte(self, fonction):
        self.contexte_ = fonction

    def apres_echange(self, _fonction):
        pass

    def outil(self, nom, _description, _parametres, _requis, fonction):
        self.outils[nom] = fonction

    def regle(self, motif, fonction, priorite=50):
        self.regles.append((re.compile(motif, re.IGNORECASE), fonction, priorite))
        self.regles.sort(key=lambda r: r[2])

    def raccourci(self, phrase):
        texte = normaliser(phrase)
        for motif, fonction, _ in self.regles:
            m = motif.match(texte)
            if m:
                return fonction(m, phrase)
        return None


class Contexte:
    cfg = {"cerveau": {}}
    simulation = True


def installer(monkeypatch, tmp_path):
    monkeypatch.setattr(stockage, "DOSSIER", tmp_path / "donnees")
    reg = Registre()
    memoire.installer(Contexte(), reg)
    return reg, reg.apis["memoire"]


def remplir(api, noms):
    for nom, type_ in noms:
        assert api("POST", {"action": "ajouter_note", "nom": nom, "type": type_})["ok"], nom


BRUIT = [("3h", "autre"), ("7 mai 2022", "evenement"), ("une alarme à 17h", "autre"), ("les livres", "objet"),
         ("Livre", "objet"), ("Maman", "personne"), ("Pronote", "ecole"), ("demain", "autre"), ("42", "autre")]


def ids_suggeres(donnees):
    return {s["id"]: s["raison"] for s in donnees["suggestions"]}


def test_analyse_repere_le_bruit_et_les_doublons(monkeypatch, tmp_path):
    _, api = installer(monkeypatch, tmp_path)
    remplir(api, BRUIT)
    sugg = ids_suggeres(api("GET", {}))
    assert sugg["3h"].startswith("heure")
    assert sugg["7_mai_2022"] == "date seule"
    assert sugg["demain"] == "date seule"
    assert sugg["une_alarme_a_17h"] == "commande ponctuelle"
    assert sugg["42"] == "nombre seul"
    # un seul des deux « livres » est proposé (fusion vers l'autre)
    assert ("les_livres" in sugg) != ("livre" in sugg)
    assert "maman" not in sugg and "pronote" not in sugg and "moi" not in sugg


def test_garder_retire_des_suggestions(monkeypatch, tmp_path):
    _, api = installer(monkeypatch, tmp_path)
    remplir(api, BRUIT)
    assert api("POST", {"action": "modifier", "ids": ["3h"], "garde": True})["ok"]
    donnees = api("GET", {})
    assert "3h" not in ids_suggeres(donnees)
    assert next(n for n in donnees["noeuds"] if n["id"] == "3h")["garde"] is True
    # même coché, un souvenir gardé n'est pas supprimé
    r = api("POST", {"action": "nettoyer", "ids": ["3h"]})
    assert any(n["id"] == "3h" for n in r["noeuds"])


def test_nettoyer_puis_annuler(monkeypatch, tmp_path):
    _, api = installer(monkeypatch, tmp_path)
    remplir(api, BRUIT)
    avant = api("GET", {})
    assert avant["sauvegarde"] is None
    nb_avant = len(avant["noeuds"])
    r = api("POST", {"action": "nettoyer"})
    assert r["ok"] and "Mémoire rangée" in r["message"]
    assert r["sauvegarde"]                       # une sauvegarde existe avant toute suppression de masse
    restants = {n["id"] for n in r["noeuds"]}
    assert {"moi", "maman", "pronote"} <= restants
    assert "3h" not in restants and "7_mai_2022" not in restants
    assert sum(i in restants for i in ("livre", "les_livres")) == 1     # doublon fusionné
    assert not r["suggestions"]
    assert (tmp_path / "donnees" / "memoire.sauvegarde.json").exists()

    r = api("POST", {"action": "annuler_nettoyage"})
    assert r["ok"] and len(r["noeuds"]) == nb_avant
    assert {n["id"] for n in r["noeuds"]} == {n["id"] for n in avant["noeuds"]}
    assert len(r["liens"]) == len(avant["liens"])
    assert not api("POST", {"action": "annuler_nettoyage"})["ok"]      # plus rien à annuler


def test_suppression_multiple_ne_touche_pas_moi(monkeypatch, tmp_path):
    _, api = installer(monkeypatch, tmp_path)
    remplir(api, BRUIT)
    r = api("POST", {"action": "nettoyer", "ids": ["maman", "pronote", "moi"]})
    ids = {n["id"] for n in r["noeuds"]}
    assert "moi" in ids and "maman" not in ids and "pronote" not in ids
    assert all("maman" not in (l["a"], l["b"]) for l in r["liens"])
    assert not api("POST", {"action": "nettoyer", "ids": "maman"})["ok"]


def test_renommer_changer_type_et_fusion(monkeypatch, tmp_path):
    _, api = installer(monkeypatch, tmp_path)
    remplir(api, [("Helene Dupre", "autre"), ("Hélène", "personne")])
    r = api("POST", {"action": "modifier", "id": "helene_dupre", "nom": "Hélène Dupré"})
    assert r["ok"] and any(n["nom"] == "Hélène Dupré" for n in r["noeuds"])
    r = api("POST", {"action": "modifier", "ids": ["helene_dupre"], "type": "personne"})
    assert next(n for n in r["noeuds"] if n["id"] == "helene_dupre")["type"] == "personne"
    r = api("POST", {"action": "modifier", "ids": ["helene_dupre"], "type": "nimporte"})
    assert next(n for n in r["noeuds"] if n["id"] == "helene_dupre")["type"] == "autre"
    # renommer comme un autre souvenir = fusion
    r = api("POST", {"action": "modifier", "id": "helene", "nom": "Helene Dupre"})
    assert r["ok"] and "Fusionné" in r["message"]
    assert not any(n["id"] == "helene" for n in r["noeuds"])
    assert not api("POST", {"action": "modifier", "id": "moi", "nom": "Toi"})["ok"]
    assert not api("POST", {"action": "modifier", "id": "helene_dupre", "nom": "mot de passe 1234"})["ok"]


def test_voix_range_et_oublie_demandent_un_oui(monkeypatch, tmp_path):
    reg, api = installer(monkeypatch, tmp_path)
    remplir(api, BRUIT)
    assert reg.raccourci("Range ma mémoire") == [("ranger_memoire", {})]
    assert reg.raccourci("fais le tri dans ta mémoire") == [("ranger_memoire", {})]
    assert reg.raccourci("Annule le nettoyage") == [("annuler_nettoyage_memoire", {})]
    assert reg.raccourci("oublie Maman") == [("oublier_souvenir", {"sujet": "maman"})]
    assert reg.raccourci("oublie pas de sortir") is None

    # « range » réorganise tout de suite (sans « oui »), et « annule le rangement » remet tout
    fait = reg.outils["ranger_memoire"]()
    assert "Mémoire rangée" in fait.dire and not fait.confirmer
    assert "3h" not in {n["id"] for n in api("GET", {})["noeuds"]}
    assert "Rangement annulé" in reg.outils["annuler_nettoyage_memoire"]().dire
    assert len(api("GET", {})["noeuds"]) == len(BRUIT) + 1

    question = reg.outils["oublier_souvenir"]("Maman de ta mémoire")
    assert question.confirmer and "Maman" in question.dire
    assert any(n["id"] == "maman" for n in api("GET", {})["noeuds"])
    question.confirmer()
    assert not any(n["id"] == "maman" for n in api("GET", {})["noeuds"])
    assert reg.outils["annuler_nettoyage_memoire"]().ok
    assert any(n["id"] == "maman" for n in api("GET", {})["noeuds"])
    assert "rien" in reg.outils["oublier_souvenir"]("Batman").dire


def test_vieux_souvenir_jamais_utilise():
    vieux = time.time() - 90 * memoire.JOUR
    noeuds = [{"id": "moi", "nom": "Moi", "type": "moi"},
              {"id": "truc", "nom": "truc bizarre", "type": "autre", "cree": vieux, "vu": vieux},
              {"id": "foot", "nom": "foot", "type": "activite", "cree": vieux, "vu": vieux},
              {"id": "note", "nom": "le prof de maths est sympa et rigolo mais un peu strict", "type": "autre",
               "cree": vieux}]
    liens = [{"a": "moi", "b": "truc", "relation": "parle de"}, {"a": "moi", "b": "foot", "relation": "aime"},
             {"a": "moi", "b": "note", "relation": "a dit"}]
    sugg = {s["id"]: s["raison"] for s in memoire.analyser(noeuds, liens)}
    assert sugg == {"truc": "jamais utilisé depuis 2 mois"}      # « a dit » = noté exprès : jamais proposé
    assert "2 dates" not in memoire.resumer_suggestions([{"nom": "x", "raison": "date seule"}])


def test_coffre_obsidian(monkeypatch, tmp_path):
    reg, api = installer(monkeypatch, tmp_path)
    coffre = tmp_path / "Mémoire"
    remplir(api, [("Lucas", "personne"), ("le football", "activite")])
    assert (coffre / "Moi.md").exists()
    assert (coffre / "Personnes" / "Lucas.md").exists()
    assert "[[Lucas]]" in (coffre / "Moi.md").read_text(encoding="utf-8")
    # une note écrite à la main dans Obsidian est relue, avec son lien
    (coffre / "Goûts").mkdir(exist_ok=True)
    (coffre / "Goûts" / "Pizza.md").write_text("# Pizza\n\nLa 4 fromages surtout.\n- va avec [[Lucas]]\n",
                                                encoding="utf-8")
    d = api("GET", {})
    pizza = next(n for n in d["noeuds"] if n["nom"] == "Pizza")
    assert pizza["type"] == "preference" and "4 fromages" in pizza["texte"]
    assert any(l["a"] == pizza["id"] and l["b"] == "lucas" for l in d["liens"])
    # « oublie que j'aime le foot » : le fait part, et la note qui n'est plus reliée à rien aussi
    with memoire.Coffre(coffre) as m:
        m["liens"] = [l for l in m["liens"] if l["b"] != "le_football"]
        m["liens"].append({"a": "moi", "b": "le_football", "relation": "aime", "cree": 1.0})
    q = reg.outils["oublier_souvenir"]("que j'aime le foot")
    assert q.confirmer and "football" in q.dire
    q.confirmer()
    assert not (coffre / "Activités" / "le football.md").exists()
    assert not any(n["id"] == "le_football" for n in api("GET", {})["noeuds"])
    # la note écrite à la main n'est jamais rangée toute seule
    reg.outils["ranger_memoire"]()
    assert (coffre / "Goûts" / "Pizza.md").exists()
