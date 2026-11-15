"""Vidéo et musique en cours (YouTube dans Chrome, Spotify…) + volume de Windows.

- Pause, lecture, suivant, précédent : contrôles média de Windows (marche avec n'importe quelle appli).
- Avancer, reculer, vitesse : l'extension Chrome de Jarvis (plus précis dans Chrome), sinon Windows.
- Volume : celui de Windows.
"""
import asyncio
import logging
from datetime import timedelta

from . import Resultat, nombre

log = logging.getLogger("medias")


# ── contrôles média de Windows ───────────────────────────────────────────────

async def _session():
    from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as Gestion
    gestion = await Gestion.request_async()
    session = gestion.get_current_session()
    if session is None:
        sessions = list(gestion.get_sessions())
        session = sessions[0] if sessions else None
    return session


async def _windows(action: str, valeur: float = 0):
    s = await _session()
    if s is None:
        return None
    if action == "pause":
        return await s.try_pause_async()
    if action == "lecture":
        return await s.try_play_async()
    if action == "basculer":
        return await s.try_toggle_play_pause_async()
    if action == "suivant":
        return await s.try_skip_next_async()
    if action == "precedent":
        return await s.try_skip_previous_async()
    if action in ("avancer", "reculer"):
        position = s.get_timeline_properties().position
        decalage = timedelta(seconds=valeur if action == "avancer" else -valeur)
        nouvelle = max(timedelta(0), position + decalage)
        return await s.try_change_playback_position_async(int(nouvelle.total_seconds() * 10_000_000))
    if action == "vitesse":
        return await s.try_change_playback_rate_async(float(valeur))
    if action == "infos":
        p = await s.try_get_media_properties_async()
        appli = s.source_app_user_model_id.split(".")[0].split("!")[0]
        return {"titre": p.title, "artiste": p.artist, "appli": appli}
    return None


def windows(action: str, valeur: float = 0):
    try:
        return asyncio.run(_windows(action, valeur))
    except Exception:  # noqa: BLE001
        log.exception("contrôle média Windows (%s)", action)
        return None


# ── volume de Windows ────────────────────────────────────────────────────────

def volume_windows(action: str, valeur=None):
    import comtypes
    from pycaw.pycaw import AudioUtilities
    comtypes.CoInitialize()
    try:
        vol = AudioUtilities.GetSpeakers().EndpointVolume
        actuel = round(vol.GetMasterVolumeLevelScalar() * 100)
        if action == "couper":
            vol.SetMute(1, None)
            return "Son coupé."
        if action == "remettre":
            vol.SetMute(0, None)
            return f"Son remis, à {actuel} %."
        if action == "niveau":
            return f"Le volume est à {actuel} %."
        if action in ("monter", "baisser"):
            pas = float(valeur) if valeur else 10
            cible = actuel + pas if action == "monter" else actuel - pas
        else:
            cible = float(valeur if valeur is not None else actuel)
        cible = max(0, min(100, cible))
        vol.SetMute(0, None)
        vol.SetMasterVolumeLevelScalar(cible / 100, None)
        return f"Volume à {round(cible)} %."
    finally:
        comtypes.CoUninitialize()


# ── module ───────────────────────────────────────────────────────────────────

def installer(ctx, reg):
    def extension(action, valeur=None):
        if not ctx.pont.connecte:
            return None
        r = ctx.pont.commande({"commande": "video", "action": action, "valeur": valeur}, delai=4)
        return r if r and r.get("ok") else None

    def media(action: str, valeur: float = None):
        action = {"play": "lecture", "reprendre": "lecture", "stop": "pause", "suivante": "suivant",
                  "precedente": "precedent", "toggle": "basculer"}.get(action, action)
        if action in ("avancer", "reculer", "vitesse"):
            v = float(valeur) if valeur not in (None, "") else (1.0 if action == "vitesse" else 10)
            if extension(action, v) or windows(action, v):
                if action == "vitesse":
                    return Resultat(f"Vitesse {str(v).replace('.', ',').removesuffix(',0')}.")
                verbe = "J'avance" if action == "avancer" else "Je recule"
                return Resultat(f"{verbe} de {int(v)} secondes.")
            return Resultat("Je ne trouve pas de vidéo à contrôler.", ok=False)
        if action == "infos":
            infos = windows("infos")
            if not infos or not infos.get("titre"):
                return Resultat("Rien ne joue en ce moment.")
            artiste = f" de {infos['artiste']}" if infos.get("artiste") else ""
            return Resultat(f"C'est « {infos['titre']} »{artiste}, sur {infos['appli']}.")
        ok = windows(action)
        if not ok and action in ("pause", "lecture", "basculer"):
            ok = extension(action)
        if not ok:
            return Resultat("Je ne trouve rien à mettre en pause ou à relancer." if action in ("pause", "lecture", "basculer")
                            else "Ça n'a pas marché.", ok=False)
        return Resultat({"pause": "Pause.", "lecture": "Ça repart.", "basculer": "C'est fait.",
                         "suivant": "Suivant.", "precedent": "Précédent."}.get(action, "C'est fait."))

    def volume(action: str, valeur: float = None):
        try:
            return Resultat(volume_windows(action, valeur))
        except Exception:  # noqa: BLE001
            log.exception("volume")
            return Resultat("Je n'arrive pas à changer le volume.", ok=False)

    reg.outil("media", "Contrôle la vidéo ou la musique en cours (YouTube et autres vidéos dans Chrome, Spotify…).",
              {"action": {"type": "string", "enum": ["pause", "lecture", "basculer", "suivant", "precedent",
                                                     "avancer", "reculer", "vitesse", "infos"],
                          "description": "infos = dire ce qui joue"},
               "valeur": {"type": "number", "description": "secondes pour avancer/reculer, ou vitesse (1.5)"}},
              ["action"], media)
    reg.outil("volume", "Change le volume du PC.",
              {"action": {"type": "string", "enum": ["monter", "baisser", "regler", "couper", "remettre", "niveau"]},
               "valeur": {"type": "number", "description": "en pour cent : de combien monter/baisser, ou niveau voulu"}},
              ["action"], volume)

    # ── raccourcis sans IA ──
    objet = r"(?: (?:la |le |cette |ma )?(?:video|musique|chanson|film|serie|episode|youtube|spotify))?"
    reg.regle(rf"^(?:mets?|met)?(?: en)? ?pause{objet}$|^(?:mets?|met){objet} en pause$|^(?:arrete|stoppe|stop){objet}$",
              lambda m, t: [("media", {"action": "pause"})], priorite=10)
    reg.regle(rf"^(?:lecture|play|reprends?|relance|continue|remets?|mets? play|mets? lecture){objet}$",
              lambda m, t: [("media", {"action": "lecture"})], priorite=10)
    reg.regle(r"^(?:(?:la |le )?(?:video|musique|chanson|piste|morceau) )?(?:suivante?|next)$|^passe(?: a la suivante| la chanson| la video| la musique)?$",
              lambda m, t: [("media", {"action": "suivant"})], priorite=10)
    reg.regle(r"^(?:(?:la |le )?(?:video|musique|chanson|piste|morceau) )?precedente?$|^reviens? (?:a la |en )(?:precedente|arriere)$",
              lambda m, t: [("media", {"action": "precedent"})], priorite=10)
    reg.regle(r"^(?:avance|recule)(?: la video| de)? (?P<n>.+?) ?(?:secondes?|s)$",
              lambda m, t: [("media", {"action": "avancer" if m.group(0).startswith("avance") else "reculer",
                                       "valeur": nombre(m["n"]) or 10})] if nombre(m["n"]) else None, priorite=10)
    reg.regle(r"^(?:monte|augmente)(?: un peu)? (?:le son|le volume)$|^plus fort$|^(?:mets|met) plus fort$",
              lambda m, t: [("volume", {"action": "monter"})], priorite=10)
    reg.regle(r"^(?:baisse|diminue)(?: un peu)? (?:le son|le volume)$|^moins fort$|^(?:mets|met) moins fort$",
              lambda m, t: [("volume", {"action": "baisser"})], priorite=10)
    reg.regle(r"^(?:coupe le son|mode muet|mets? (?:le )?(?:son )?en muet|mute)$",
              lambda m, t: [("volume", {"action": "couper"})], priorite=10)
    reg.regle(r"^(?:remets?|reactive|rallume) le son$",
              lambda m, t: [("volume", {"action": "remettre"})], priorite=10)

    def regle_niveau(m, _t):
        n = nombre(m["n"])
        return [("volume", {"action": "regler", "valeur": n})] if n is not None else None
    reg.regle(r"^(?:mets?|met|regle) (?:le )?(?:son|volume) a (?P<n>[a-z0-9 ]+?)(?: pour ?cent| %)?$|^volume (?P<n2>\d+)$",
              lambda m, t: regle_niveau({"n": m["n"] or m["n2"]}, t), priorite=10)
    reg.regle(r"^(?:qu'est ce qui|qu est ce qui|c'est quoi qui|quelle (?:chanson|musique|video)) (?:joue|passe|est en train)|"
              r"^(?:c'est quoi|c est quoi|quelle est|quel est|dis moi) (?:le titre de )?(?:la |cette |le )?"
              r"(?:chanson|musique|video|son|morceau|titre)(?: qui (?:passe|joue)| en cours| actuelle)?$",
              lambda m, t: [("media", {"action": "infos"})], priorite=10)
