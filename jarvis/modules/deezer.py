"""Deezer : « mets la playlist Electro Chill », « joue la chanson Bande organisée », « mets l'album … sur Deezer ».

1. Cherche avec la recherche publique et gratuite de Deezer (et d'abord dans TES playlists si ton profil est
   indiqué dans config.toml, section [deezer]).
2. Ouvre le résultat dans l'appli Deezer (lien deezer://), sinon dans le navigateur.
3. Vérifie que la musique démarre ; sinon, clique sur « Écouter » grâce à la vision (module écran).
"""
import asyncio
import logging
import os
import re
import time

import requests

from . import Resultat, normaliser

log = logging.getLogger("deezer")
API = "https://api.deezer.com"
GENRES = {"playlist": "playlist", "titre": "track", "album": "album", "artiste": "artist"}


def meilleur(resultats: list, recherche: str, cle: str = "title"):
    """Le résultat dont le nom ressemble le plus à la recherche."""
    r = normaliser(recherche)
    noms = [(x, normaliser(str(x.get(cle) or x.get("name") or ""))) for x in resultats]
    for test in (lambda n: n == r, lambda n: n.startswith(r), lambda n: r in n):
        for x, n in noms:
            if test(n):
                return x
    return resultats[0] if resultats else None


async def _deezer_joue():
    """(titre, en lecture ?) de la session Deezer dans les contrôles média de Windows."""
    from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as Gestion
    gestion = await Gestion.request_async()
    for s in gestion.get_sessions():
        if "deezer" in s.source_app_user_model_id.lower():
            p = await s.try_get_media_properties_async()
            return p.title, s.get_playback_info().playback_status == 4
    return None, False


def etat_deezer():
    try:
        return asyncio.run(_deezer_joue())
    except Exception:  # noqa: BLE001
        return None, False


def installer(ctx, reg):
    cfg = ctx.cfg.get("deezer", {}) if isinstance(ctx.cfg.get("deezer"), dict) else {}

    def chercher(recherche: str, genre: str):
        type_api = GENRES.get(genre, "playlist")
        utilisateur = str(cfg.get("utilisateur", "")).strip()
        if type_api == "playlist" and utilisateur:          # tes playlists d'abord
            try:
                mes = requests.get(f"{API}/user/{utilisateur}/playlists", params={"limit": 100}, timeout=8).json()
                trouve = meilleur([p for p in mes.get("data", []) if normaliser(recherche) in normaliser(p.get("title", ""))],
                                  recherche)
                if trouve:
                    return trouve, type_api
            except Exception:  # noqa: BLE001
                log.warning("playlists de l'utilisateur illisibles")
        r = requests.get(f"{API}/search/{type_api}", params={"q": recherche, "limit": 10}, timeout=8).json()
        return meilleur(r.get("data", []), recherche, "name" if type_api == "artist" else "title"), type_api

    def jouer_deezer(recherche: str, genre: str = "playlist"):
        try:
            trouve, type_api = chercher(recherche, genre)
        except Exception:  # noqa: BLE001
            log.exception("recherche Deezer")
            return Resultat("Je n'arrive pas à joindre Deezer.", ok=False)
        if not trouve:
            return Resultat(f"Je ne trouve rien qui s'appelle « {recherche} » sur Deezer.", ok=False)
        nom = trouve.get("title") or trouve.get("name") or recherche
        titre_avant, _ = etat_deezer()
        lien = f"deezer://www.deezer.com/{type_api}/{trouve['id']}"
        try:
            os.startfile(lien)                       # l'appli Deezer
        except OSError:
            os.startfile(f"https://www.deezer.com/fr/{type_api}/{trouve['id']}")   # sinon le site
            return Resultat(f"J'ouvre « {nom} » sur le site de Deezer.")
        log.info("Deezer : %s %s (%s)", type_api, trouve["id"], nom)
        # La musique démarre-t-elle toute seule ? Sinon, on clique sur « Écouter ».
        for _ in range(6):
            time.sleep(1)
            titre, joue = etat_deezer()
            if joue and titre and titre != titre_avant:
                return Resultat(f"C'est parti : « {nom} » sur Deezer.")
        cliquer = getattr(ctx, "cliquer_sur", None)
        if cliquer:
            r = cliquer("le gros bouton « Écouter » (lecture) de la page Deezer ouverte, pas celui d'un titre")
            if r.ok:
                return Resultat(f"C'est parti : « {nom} » sur Deezer.")
        return Resultat(f"J'ai ouvert « {nom} » dans Deezer : appuie sur Écouter pour lancer.")

    reg.outil("jouer_deezer", "Lance de la musique sur Deezer : une playlist, un titre, un album ou un artiste.",
              {"recherche": "le nom de la playlist, du titre, de l'album ou de l'artiste",
               "genre": {"type": "string", "enum": list(GENRES), "description": "playlist par défaut"}},
              ["recherche"], jouer_deezer)

    def regle(genre):
        def f(m, texte):
            brut = re.sub(r"(?i)\s+(?:sur|de|dans)\s+deezer\s*[.!?]*$", "", texte).strip(" .!?")
            mots = {"playlist": r"playlist", "titre": r"(?:chanson|titre|son|morceau)", "album": r"album",
                    "artiste": r"(?:artiste|musique de|chansons de|du)"}[genre]
            q = re.split(rf"(?i)\b{mots}\b\s*", brut, maxsplit=1)
            recherche = (q[1] if len(q) > 1 else m["q"]).strip(" «»\"'")
            return [("jouer_deezer", {"recherche": recherche, "genre": genre})] if recherche else None
        return f

    verbe = r"^(?:mets|met|mettre|lance|joue|jouer|ecoute|demarre)(?: moi)? "
    reg.regle(verbe + r"(?:la |ma |une |le )?playlist (?P<q>.+)$", regle("playlist"), priorite=14)
    reg.regle(verbe + r"(?:la chanson|le titre|le son|le morceau) (?P<q>.+)$", regle("titre"), priorite=14)
    reg.regle(verbe + r"l'?album (?P<q>.+)$", regle("album"), priorite=14)
    reg.regle(verbe + r"(?P<q>.+) sur deezer$",
              lambda m, t: [("jouer_deezer", {"recherche": re.sub(r"(?i)^\W*(?:mets|met|lance|joue)(?:[- ]moi)?\s+|"
                                                                    r"\s+sur deezer\W*$", "", t).strip(),
                                              "genre": "playlist"})], priorite=16)
