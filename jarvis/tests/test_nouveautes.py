"""Nouveautés : vidéo YouTube sans pub, sites dans Chrome, ce que Jarvis sait faire, révision d'après Pronote,
aide aux comptes, a-ajouter, fichiers, personnalité, concentration, volume par appli."""
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from modules import Registre, normaliser  # noqa: E402
from modules import (a_ajouter, applis, capacites, comptes, concentration, fichiers, lecons, medias,  # noqa: E402
                     minuteurs, personnalite, web)
from modules.fichiers import chercher, mots_de  # noqa: E402
from modules.lecons import lecon_depuis_pronote  # noqa: E402
from modules.medias import morceaux_appli  # noqa: E402
from modules.personnalite import nettoyer_consigne  # noqa: E402
from modules.web import premieres_videos  # noqa: E402


def registre(*mods, simulation=True, etat=None):
    givre = types.SimpleNamespace(etat=lambda age_max=2: etat or {}, ouvrir=lambda *a: {"ok": True},
                                  disponible=lambda: True, ne_pas_deranger=lambda actif=None: {"ok": True},
                                  minuteur=lambda *a: None)
    pont = types.SimpleNamespace(connecte=False, commande=lambda *a, **k: {"ok": False})
    ctx = types.SimpleNamespace(cfg={"sites": {"youtube": "https://www.youtube.com"}, "applis": {},
                                     "fichiers": {}, "concentration": {}, "cerveau": {}},
                                givre=givre, pont=pont, etat=None, simulation=simulation, appels=[])
    reg = Registre(ctx)
    for m in mods:
        m.installer(ctx, reg)
    return reg


def raccourci(reg, phrase):
    n = normaliser(phrase)
    for regle in reg.regles:
        m = regle.motif.search(n)
        if m:
            appels = regle.fonction(m, phrase)
            if appels:
                return appels
    return None


def test_premiere_video_sans_pub():
    html = ('{"adSlotRenderer":{"videoId":"PUBPUBPUB01"}},{"reelItemRenderer":{"videoId":"SHORTSHORT1"}},'
            '{"videoRenderer":{"videoId":"abcdefghijk","thumbnail":{},"title":{"runs":[{"text":"La vraie vid\\u00e9o"}]}}},'
            '{"videoRenderer":{"videoId":"zzzzzzzzzzz","title":{"runs":[{"text":"Deuxième"}]}}}')
    assert premieres_videos(html) == [("abcdefghijk", "La vraie vidéo"), ("zzzzzzzzzzz", "Deuxième")]


def test_raccourcis_web():
    reg = registre(web)
    assert raccourci(reg, "Prends la première vidéo qui n'est pas une pub") == [("jouer_youtube", {"recherche": ""})]
    assert raccourci(reg, "Mets Squeezie sur YouTube") == [("jouer_youtube", {"recherche": "squeezie"})]
    assert raccourci(reg, "Lance YouTube sur Chrome") == [("ouvrir_site", {"site": "youtube"})]
    assert raccourci(reg, "Lanche Youtube sur Chrome") == [("ouvrir_site", {"site": "youtube"})]
    assert raccourci(reg, "Lance Pronote sur Chrome") == [("ouvrir_givre", {"quoi": "pronote"})]
    assert raccourci(reg, "Ferme Pronote dans Chrome") == [("fermer_onglet", {"cherche": "pronote"})]
    assert raccourci(reg, "Ferme YouTube") == [("fermer_onglet", {"cherche": "youtube"})]
    assert raccourci(reg, "Ferme la calculatrice") is None


def test_ce_que_je_sais_faire():
    reg = registre(capacites, web, minuteurs, lecons)
    assert raccourci(reg, "Qu'est-ce que tu sais faire ?") == [("ce_que_je_sais_faire", {})]
    assert raccourci(reg, "Qu'est-ce que tu ne sais pas faire ?") is None
    phrase = capacites.phrase_capacites(reg)
    assert "minuteurs" in phrase and "réviser" in phrase and "Discord" not in phrase


def test_revision_pronote():
    p = {"connecte": True, "cours_aujourdhui": [{"debut": "08:00", "fin": "09:00", "matiere": "HISTOIRE-GEO"},
                                                {"debut": "15:00", "fin": "16:00", "matiere": "ANGLAIS"}],
         "devoirs": [{"matiere": "HISTOIRE-GEO", "texte": "Apprendre la leçon sur la Révolution"}]}
    lecon = lecon_depuis_pronote(p, "", maintenant="10:00")
    assert lecon["matiere"] == "HISTOIRE-GEO" and "Révolution" in lecon["texte"]
    assert lecon_depuis_pronote(p, "anglais")["matiere"] == "anglais"
    assert lecon_depuis_pronote({"connecte": False}) is None
    reg = registre(lecons)
    assert raccourci(reg, "Fais-moi réviser") == [("reciter_lecon", {"matiere": ""})]
    assert raccourci(reg, "Fais-moi réviser l'histoire") is None or raccourci(reg, "Fais-moi réviser en histoire") == \
        [("reciter_lecon", {"matiere": "histoire"})]


def test_aide_comptes_pas_a_pas():
    reg = registre(comptes, etat={"ok": True, "pronote": {"connecte": True}, "mails": {"connecte": False}})
    assert raccourci(reg, "Aide-moi à connecter mes comptes")[0][0] == "aide_comptes"
    assert raccourci(reg, "Comment je connecte Pronote ?")[0][0] == "aide_comptes"
    r = reg.outils["aide_comptes"].fonction()
    assert "Déjà connectés : Pronote" in r.dire and "Étape 1 sur 3" in r.dire
    inter = reg.intercepteurs[0]
    assert "Étape 2 sur 3" in inter("suivant", "suivant")
    assert inter("raconte une blague", "raconte une blague") is None
    assert "arrête" in inter("stop", "stop")
    assert inter("suivant", "suivant") is None


def test_noter_a_ajouter():
    reg = registre(a_ajouter)
    assert raccourci(reg, "Ok, écris-le dans un truc que tu ne peux pas faire")[0][0] == "noter_a_ajouter"
    assert a_ajouter.DEMANDE_DE_NOTER.search(normaliser("note-le dans ce que tu ne sais pas faire"))


def test_fichiers(tmp_path):
    (tmp_path / "Histoire").mkdir()
    (tmp_path / "Histoire" / "Exposé Révolution.pptx").write_text("x")
    (tmp_path / "maths.pdf").write_text("x")
    (tmp_path / "node_modules").mkdir()
    (tmp_path / "node_modules" / "expose histoire.pptx").write_text("x")
    assert mots_de("mon exposé d'histoire") == (["expose", "histoire"], set())
    res = chercher("exposé d'histoire", [tmp_path])
    assert [p.name for p in res] == ["Exposé Révolution.pptx"]
    assert [p.name for p in chercher("pdf maths", [tmp_path])] == ["maths.pdf"]
    reg = registre(fichiers)
    assert raccourci(reg, "Trouve mon exposé d'histoire") == [("trouver_fichier", {"recherche": "expose d'histoire",
                                                                                   "ouvrir": False})]
    assert raccourci(reg, "Trouve mon téléphone") is None


def test_personnalite():
    assert nettoyer_consigne("Jarvis, à partir de maintenant sois plus court !") == "Sois plus court."
    reg = registre(personnalite)
    assert raccourci(reg, "Sois plus court")[0] == ("personnalite", {"action": "ajouter", "consigne": "Sois plus court"})
    assert raccourci(reg, "Parle comme un pirate")[0][1]["action"] == "ajouter"
    assert raccourci(reg, "Redeviens normal") == [("personnalite", {"action": "defaut"})]
    assert raccourci(reg, "Parle plus fort") is None


def test_personnalite_fichier(tmp_path, monkeypatch):
    from core import config
    monkeypatch.setattr(config, "PROMPT", tmp_path / "prompt-systeme.txt")
    monkeypatch.setattr(config, "PROMPT_DEFAUT", tmp_path / "defaut.txt")
    (tmp_path / "defaut.txt").write_text("# commentaire\nTu es {nom}.\n", encoding="utf-8")
    assert personnalite.ajouter("Sois plus court.") == ["Sois plus court."]
    personnalite.ajouter("Parle comme un pirate.")
    assert "Parle comme un pirate." in config.prompt_systeme()
    assert personnalite.retirer_derniere() == "Parle comme un pirate."
    assert personnalite.par_defaut() and config.prompt_systeme() == "Tu es {nom}."


def test_concentration():
    reg = registre(concentration)
    assert raccourci(reg, "Lance le mode concentration") == [("mode_concentration", {"action": "demarrer"})]
    assert raccourci(reg, "Active le mode concentration pendant 45 minutes") == \
        [("mode_concentration", {"action": "demarrer", "minutes": 45})]
    assert raccourci(reg, "Arrête le mode concentration") == [("mode_concentration", {"action": "arreter"})]


def test_concentration_execute():
    reg = registre(concentration, simulation=False)
    appels = []
    reg.executer = (lambda vieux: (lambda nom, args: appels.append(nom) or vieux(nom, args)))(reg.executer)
    r = reg.outils["mode_concentration"].fonction("demarrer", 1)
    assert "Mode concentration pour 1 minute" in r.dire and "Ne pas déranger est activé" in r.dire
    assert "minuteur" in appels and reg.ctx.concentration_jusqua > 0
    assert "arrêté" in reg.outils["mode_concentration"].fonction("arreter").dire
    assert reg.ctx.concentration_jusqua == 0


def test_volume_par_appli():
    reg = registre(medias)
    assert raccourci(reg, "Baisse Chrome, laisse la musique") == [("volume_appli", {"appli": "chrome", "action": "baisser"})]
    assert raccourci(reg, "Mets Discord à 30 %") == [("volume_appli", {"appli": "discord", "action": "regler", "valeur": 30})]
    assert raccourci(reg, "Coupe la musique") == [("media", {"action": "pause"})]
    assert raccourci(reg, "Baisse le son") == [("volume", {"action": "baisser"})]
    assert morceaux_appli("la musique")[0] == "spotify" and morceaux_appli("chrome") == ["chrome"]


def test_applis_ne_volent_pas_les_sites():
    """« Lance YouTube sur Chrome » ne doit pas juste ouvrir Chrome."""
    reg = registre(applis, web)
    assert raccourci(reg, "Lance YouTube sur Chrome") == [("ouvrir_site", {"site": "youtube"})]


# ── PowerPoint, sac, navigateur par défaut ──
def test_construire_pptx(tmp_path):
    from modules.fichiers import construire_pptx, nettoyer_plan
    from pptx import Presentation
    plan = nettoyer_plan({"titre": "Les volcans", "diapos": [
        {"titre": "Définition", "points": ["Une montagne qui crache", "Du magma", ""]},
        {"titre": "Types", "points": ["Effusif", "Explosif", "Éteint"]}]}, "volcans")
    f = construire_pptx(plan, tmp_path / "test.pptx")
    prez = Presentation(str(f))
    assert len(prez.slides) == 3                      # titre + 2 parties
    assert prez.slide_width > prez.slide_height * 1.7  # 16:9
    assert len(plan["diapos"][0]["points"]) == 2      # le point vide est retiré


def test_raccourci_powerpoint():
    reg = registre(fichiers)
    a = raccourci(reg, "Crée-moi un PowerPoint sur les volcans")
    assert a == [("creer_powerpoint", {"sujet": "les volcans"})]
    a = raccourci(reg, "fais un diaporama sur la Révolution française (8 diapos)")
    assert a[0][1]["sujet"] == "la revolution francaise" and a[0][1]["diapos"] == 8


ETAT_SAC = {"ok": True, "pronote": {"connecte": True, "semaine": [
    {"date": "2026-11-23", "jour": "lundi", "cours": [
        {"debut": "08:00", "fin": "09:00", "matiere": "EPS"},
        {"debut": "09:00", "fin": "10:00", "matiere": "MATHS", "annule": True},
        {"debut": "10:00", "fin": "11:00", "matiere": "LATIN", "n_y_va_pas": "PAI"},
        {"debut": "11:00", "fin": "12:00", "matiere": "FRANCAIS"}]}],
    "devoirs": [{"matiere": "FRANCAIS", "pour": "2026-11-23", "texte": "Lire le chapitre 3", "fait": False},
                {"matiere": "EPS", "pour": "2026-11-23", "texte": "déjà fait", "fait": True}]}}


def test_sac_du_jour():
    from modules.givre_infos import sac_du_jour
    matieres, devoirs = sac_du_jour(ETAT_SAC["pronote"], "2026-11-23")
    assert matieres == ["EPS", "FRANCAIS"]
    assert [d["texte"] for d in devoirs] == ["Lire le chapitre 3"]


def test_outil_liste_du_sac():
    from modules import givre_infos
    reg = registre(givre_infos, etat=ETAT_SAC)
    a = raccourci(reg, "Qu'est-ce que je dois mettre dans mon sac lundi ?")
    assert a == [("liste_du_sac", {"jour": "lundi"})]
    assert raccourci(reg, "mon sac pour demain")[0][1]["jour"] == "demain"
    from datetime import date
    assert givre_infos.jour_vise("lundi", date(2026, 11, 21)).isoformat() == "2026-11-23"
    r = reg.outils["liste_du_sac"].fonction(jour="lundi")
    assert r.ok
    assert "EPS" in r.pour_ia and "LATIN" not in r.pour_ia and "MATHS" not in r.pour_ia


def test_nom_navigateur():
    from modules.applis import nom_navigateur
    assert nom_navigateur("ChromeHTML") == "Google Chrome"
    assert nom_navigateur("MSEdgeHTM") == "Microsoft Edge"
    assert nom_navigateur("FirefoxURL-308046B0AF4A39CB") == "Firefox"
    assert nom_navigateur("BraveHTML") == "Brave"
    assert nom_navigateur("OperaStable") == "Opera"
    assert nom_navigateur("Truc.html") == "Truc.html"


def test_raccourci_navigateur():
    reg = registre(applis)
    assert raccourci(reg, "Quel est mon navigateur par défaut ?") == [("navigateur_par_defaut", {})]
