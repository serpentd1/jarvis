"""Musique pour la grande application : titre, artiste, pochette (Deezer, Spotify, YouTube…) et contrôles."""
import asyncio
import base64
import logging

log = logging.getLogger("musique")


async def _infos():
    from winrt.windows.media.control import GlobalSystemMediaTransportControlsSessionManager as Gestion
    gestion = await Gestion.request_async()
    s = gestion.get_current_session()
    if s is None:
        return {"ok": True, "titre": None}
    p = await s.try_get_media_properties_async()
    info = s.get_playback_info()
    pochette = None
    if p.thumbnail:
        try:
            from winrt.windows.storage.streams import Buffer, InputStreamOptions
            flux = await p.thumbnail.open_read_async()
            tampon = Buffer(flux.size)
            await flux.read_async(tampon, flux.size, InputStreamOptions.READ_AHEAD)
            pochette = base64.b64encode(bytes(tampon)).decode()
        except Exception:  # noqa: BLE001
            log.debug("pochette illisible", exc_info=True)
    appli = s.source_app_user_model_id
    nom_appli = ("Deezer" if "deezer" in appli.lower() else "Spotify" if "spotify" in appli.lower()
                 else "Chrome" if "chrome" in appli.lower() else appli.split("!")[0].split(".")[0])
    t = s.get_timeline_properties()
    return {"ok": True, "titre": p.title, "artiste": p.artist, "album": p.album_title, "appli": nom_appli,
            "en_lecture": info.playback_status == 4, "pochette": pochette,
            "position": t.position.total_seconds(), "duree": t.end_time.total_seconds()}


def installer(ctx, reg):
    def api(methode, p):
        from .medias import volume_windows, windows
        if methode == "POST":
            a = p.get("action", "")
            if a in ("pause", "lecture", "basculer", "suivant", "precedent"):
                windows(a)
            elif a == "volume":
                volume_windows("regler", float(p.get("valeur", 50)))
            elif a == "deezer" and p.get("recherche"):
                r = ctx.cerveau.reg.executer("jouer_deezer", {"recherche": p["recherche"], "genre": p.get("genre", "playlist")})
                return {"ok": r.ok, "message": r.dire}
        try:
            infos = asyncio.run(_infos())
        except Exception:  # noqa: BLE001
            infos = {"ok": True, "titre": None}
        try:
            infos["volume"] = int(volume_windows("niveau").split()[-2])
        except Exception:  # noqa: BLE001
            infos["volume"] = None
        return infos

    reg.api("musique", api)
