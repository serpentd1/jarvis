"""« Appelle Tom sur Discord » : toujours une confirmation avant, puis l'extension Chrome lance l'appel."""
import sys
import types
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from modules import Registre, discord, normaliser  # noqa: E402


def registre(connecte=True, reponse=None):
    commandes = []

    def commande(c, delai=0):
        commandes.append(c)
        return reponse if reponse is not None else {"ok": True}

    pont = types.SimpleNamespace(connecte=connecte, abonner=lambda *a: None, commande=commande)
    ctx = types.SimpleNamespace(cfg={"discord": {"confirmer_avant_envoi": True, "memoire": 30}, "annonces": {}},
                                pont=pont, givre=types.SimpleNamespace(disponible=lambda: False, etat=lambda: {}),
                                etat=None, simulation=False, appels=[])
    reg = Registre(ctx)
    discord.installer(ctx, reg)
    return reg, commandes


def raccourci(reg, phrase):
    n = normaliser(phrase)
    for regle in reg.regles:
        m = regle.motif.search(n)
        if m:
            appels = regle.fonction(m, phrase)
            if appels:
                return appels
    return None


def test_appel_discord_avec_confirmation():
    reg, commandes = registre()
    for phrase in ("Appelle Tom sur Discord", "appelle tom avec discord", "Appelle Tom Discord"):
        assert raccourci(reg, phrase) == [("appeler_discord", {"destinataire": "tom"})], phrase
    r = reg.executer("appeler_discord", {"destinataire": "Tom"})
    assert r.dire == "Tu es sûr de vouloir appeler Tom sur Discord ?" and r.confirmer
    assert commandes == []                                   # rien ne part avant le « oui »
    assert r.annuler == "D'accord, je n'appelle pas."
    assert r.confirmer().dire == "J'appelle Tom sur Discord."
    assert commandes == [{"commande": "discord_appeler", "destinataire": "Tom"}]


def test_appel_discord_impossible():
    reg, _ = registre(reponse={"ok": False, "erreur": "je ne trouve pas le bouton d'appel de Discord"})
    r = reg.executer("appeler_discord", {"destinataire": "Tom"}).confirmer()
    assert not r.ok and "bouton d'appel" in r.dire
    reg, commandes = registre(connecte=False)                # extension pas installée : on le dit, sans question
    r = reg.executer("appeler_discord", {"destinataire": "Tom"})
    assert not r.ok and r.confirmer is None and commandes == []
    assert not reg.executer("appeler_discord", {"destinataire": " "}).ok
