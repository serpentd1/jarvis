"""Pont entre Jarvis et son extension Chrome (WebSocket ws://127.0.0.1:47801/extension).

- L'extension envoie des événements : {"type": "discord_message", "auteur": …, "texte": …}
- Jarvis envoie des commandes : {"commande": "video", "action": "pause", "id": 7}
  et l'extension répond : {"type": "reponse", "id": 7, "ok": true, …}
"""
import asyncio
import itertools
import json
import logging
import threading
from collections import defaultdict

log = logging.getLogger("pont")


class Pont:
    def __init__(self):
        self.ws = None
        self.boucle = None                   # boucle asyncio du serveur
        self._attentes = {}
        self._abonnes = defaultdict(list)
        self._ids = itertools.count(1)

    @property
    def connecte(self) -> bool:
        return self.ws is not None and not self.ws.closed

    def abonner(self, type_evenement: str, fonction):
        self._abonnes[type_evenement].append(fonction)

    def commande(self, donnees: dict, delai: float = 10) -> dict:
        """Envoie une commande à Chrome et attend sa réponse (appelable depuis n'importe quel fil)."""
        if not self.connecte or self.boucle is None:
            return {"ok": False, "erreur": "l'extension Chrome n'est pas connectée"}
        futur = asyncio.run_coroutine_threadsafe(self._envoyer(donnees, delai), self.boucle)
        try:
            return futur.result(delai + 2)
        except Exception as e:  # noqa: BLE001
            return {"ok": False, "erreur": str(e) or "pas de réponse de Chrome"}

    async def _envoyer(self, donnees: dict, delai: float) -> dict:
        id_ = next(self._ids)
        futur = asyncio.get_running_loop().create_future()
        self._attentes[id_] = futur
        try:
            await self.ws.send_str(json.dumps({**donnees, "id": id_}))
            return await asyncio.wait_for(futur, delai)
        except asyncio.TimeoutError:
            return {"ok": False, "erreur": "Chrome n'a pas répondu à temps"}
        finally:
            self._attentes.pop(id_, None)

    async def gerer(self, ws):
        """Boucle de vie d'une connexion de l'extension (la plus récente remplace l'ancienne)."""
        ancien, self.ws = self.ws, ws
        if ancien is not None and not ancien.closed:
            await ancien.close()
        log.info("Extension Chrome connectée")
        garde = asyncio.create_task(self._garder_en_vie(ws))
        try:
            async for msg in ws:
                if msg.type != 1:            # 1 = texte
                    continue
                try:
                    donnees = json.loads(msg.data)
                except ValueError:
                    continue
                type_ = donnees.get("type")
                if type_ == "reponse":
                    futur = self._attentes.get(donnees.get("id"))
                    if futur and not futur.done():
                        futur.set_result(donnees)
                elif type_ in ("pong", "bonjour"):
                    if type_ == "bonjour":
                        log.info("Extension version %s", donnees.get("version"))
                else:
                    for f in self._abonnes.get(type_, []):
                        threading.Thread(target=_appeler, args=(f, donnees), daemon=True).start()
        finally:
            garde.cancel()
            if self.ws is ws:
                self.ws = None
                log.info("Extension Chrome déconnectée")

    @staticmethod
    async def _garder_en_vie(ws):
        """Un petit message toutes les 20 s empêche Chrome d'endormir l'extension."""
        while not ws.closed:
            await asyncio.sleep(20)
            try:
                await ws.send_str('{"type":"ping"}')
            except Exception:  # noqa: BLE001
                return


def _appeler(f, donnees):
    try:
        f(donnees)
    except Exception:  # noqa: BLE001
        log.exception("événement de l'extension en erreur")
