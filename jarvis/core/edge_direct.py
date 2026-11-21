"""Voix Edge (Microsoft, gratuite) avec UNE connexion gardée ouverte.

edge-tts ouvre une nouvelle connexion pour chaque phrase : ~0,8 s avant le premier son. En gardant la même
connexion (et en l'ouvrant à l'avance, pendant que tu parles), le premier son arrive en ~0,35 s.
"""
import asyncio
import logging
import queue
import threading
import time

log = logging.getLogger("voix")

CONFIG = ("X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n"
          '{{"context":{{"synthesis":{{"audio":{{"metadataoptions":{{"sentenceBoundaryEnabled":"false",'
          '"wordBoundaryEnabled":"false"}},"outputFormat":"audio-24khz-48kbitrate-mono-mp3"}}}}}}}}\r\n')
# Au-delà, on ne se fie plus à la connexion (le service a pu la fermer sans prévenir) : on en ouvre une neuve.
REPOS_MAX = 45.0


class EdgeDirect:
    def __init__(self):
        self.boucle = asyncio.new_event_loop()
        threading.Thread(target=self.boucle.run_forever, daemon=True, name="edge").start()
        self.session = self.ws = None
        self.utilise = 0.0
        self._verrou = None

    # ── appelé depuis n'importe quel fil ──
    def prechauffer(self):
        """Ouvre la connexion à l'avance (sans attendre)."""
        asyncio.run_coroutine_threadsafe(self._avec_verrou(self._connecter()), self.boucle)

    def synthese(self, texte: str, voix: str, vitesse: str):
        """Donne les paquets MP3 de la phrase dès qu'ils arrivent."""
        paquets = queue.Queue()
        asyncio.run_coroutine_threadsafe(self._avec_verrou(self._dire(texte, voix, vitesse, paquets)), self.boucle)
        while True:
            p = paquets.get(timeout=15)
            if p is None:
                return
            if isinstance(p, Exception):
                raise p
            yield p

    # ── dans la boucle asyncio ──
    async def _avec_verrou(self, coro):
        if self._verrou is None:
            self._verrou = asyncio.Lock()
        async with self._verrou:
            try:
                return await coro
            except Exception as e:  # noqa: BLE001
                log.debug("edge : %s", e)

    def _ouverte(self) -> bool:
        return self.ws is not None and not self.ws.closed and time.time() - self.utilise < REPOS_MAX

    async def _connecter(self, neuve: bool = False):
        if self._ouverte() and not neuve:
            return
        await self._fermer()
        import aiohttp
        from edge_tts.communicate import _SSL_CTX, connect_id, date_to_string
        from edge_tts.constants import SEC_MS_GEC_VERSION, WSS_HEADERS, WSS_URL
        from edge_tts.drm import DRM
        self.session = aiohttp.ClientSession(trust_env=True)
        self.ws = await asyncio.wait_for(self.session.ws_connect(
            f"{WSS_URL}&ConnectionId={connect_id()}&Sec-MS-GEC={DRM.generate_sec_ms_gec()}"
            f"&Sec-MS-GEC-Version={SEC_MS_GEC_VERSION}",
            compress=15, headers=DRM.headers_with_muid(WSS_HEADERS), ssl=_SSL_CTX), 8)
        await self.ws.send_str(CONFIG.format(date_to_string()))
        self.utilise = time.time()

    async def _fermer(self):
        for chose in (self.ws, self.session):
            if chose is not None:
                try:
                    await chose.close()
                except Exception:  # noqa: BLE001
                    pass
        self.ws = self.session = None

    async def _dire(self, texte: str, voix: str, vitesse: str, paquets: queue.Queue):
        import aiohttp
        from xml.sax.saxutils import escape

        from edge_tts.communicate import (connect_id, date_to_string, mkssml, remove_incompatible_characters,
                                          ssml_headers_plus_data)
        from edge_tts.data_classes import TTSConfig
        reglage = TTSConfig(voix, vitesse, "+0%", "+0Hz", "SentenceBoundary")
        ssml = mkssml(reglage, escape(remove_incompatible_characters(texte)))
        for essai in range(2):
            donne = False
            try:
                await self._connecter(neuve=essai > 0)
                await self.ws.send_str(ssml_headers_plus_data(connect_id(), date_to_string(), ssml))
                while True:
                    m = await asyncio.wait_for(self.ws.receive(), 8)
                    if m.type == aiohttp.WSMsgType.BINARY:
                        taille = int.from_bytes(m.data[:2], "big")
                        son = m.data[taille + 2:]
                        if son:
                            paquets.put(son)
                            donne = True
                    elif m.type == aiohttp.WSMsgType.TEXT:
                        if "Path:turn.end" in m.data:
                            break
                    else:
                        raise ConnectionError("connexion fermée par le service")
                self.utilise = time.time()
                paquets.put(None)
                return
            except Exception as e:  # noqa: BLE001
                await self._fermer()
                if donne or essai == 1:
                    paquets.put(e)
                    return


_direct = None


def direct() -> EdgeDirect:
    global _direct
    if _direct is None:
        _direct = EdgeDirect()
    return _direct
