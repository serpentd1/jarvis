"""API locale de Jarvis (127.0.0.1:47801) — utilisée par l'île Givre et par l'extension Chrome.

  GET  /etat                      {"ok", "etat", "micro", "texte", "extension"}
  POST /demander {"texte","source"} -> {"reponse"}   (le même cerveau et la même mémoire que la voix)
  POST /ecouter                   Jarvis écoute tout de suite (bouton « parler »)
  POST /micro {"actif": bool}     coupe / rallume l'écoute du « Hey Jarvis »
  POST /oublier                   nouvelle conversation
  POST /dire {"texte"}            Jarvis dit la phrase à voix haute
  POST /quitter                   arrête Jarvis
  POST /redemarrer                relit config.toml et redémarre (avec Jarvis.exe)
  POST /couper-voix               coupe la voix (ce moteur) ; la barre reste et peut la rallumer
  GET  /voix                      voix disponibles et voix actuelle
  POST /voix {"nom"}              change de voix (téléchargée si besoin)
  POST /selection {"type":"mail","n","de","sujet"}   ce que tu regardes dans l'île
  GET  /extension                 WebSocket de l'extension Chrome
  POST /cerveau/…                 registre pour le cerveau Rust de la barre (cerveau.rs) : avant {"texte"},
                                  outils, outil {"nom","args"}, confirmer {"id"}, contexte {"question"},
                                  apres {"question","reponse"}

Sécurité : une page web ne peut pas piloter Jarvis (en-tête Origin refusé, sauf l'extension).
"""
import asyncio
import logging
import threading

from aiohttp import web

log = logging.getLogger("serveur")


class Serveur:
    def __init__(self, port: int, etat, pont, obtenir_cerveau, lire_reponses_ile, voix_actuelle=None,
                 changer_voix=None):
        self.port, self.etat, self.pont = port, etat, pont
        self.obtenir_cerveau = obtenir_cerveau
        self.lire_reponses_ile = lire_reponses_ile
        self.voix_actuelle, self.changer_voix = voix_actuelle, changer_voix
        self.pret = threading.Event()
        self.erreur = None

    def demarrer(self):
        threading.Thread(target=self._fil, daemon=True, name="serveur").start()
        self.pret.wait(5)
        return self.erreur is None

    def _fil(self):
        boucle = asyncio.new_event_loop()
        asyncio.set_event_loop(boucle)
        self.pont.boucle = boucle
        appli = web.Application(middlewares=[self._securite], client_max_size=256 * 1024)
        appli.add_routes([
            web.get("/", self._accueil), web.get("/etat", self._etat), web.post("/demander", self._demander),
            web.post("/ecouter", self._ecouter), web.post("/micro", self._micro), web.post("/oublier", self._oublier),
            web.post("/dire", self._dire), web.post("/quitter", self._quitter), web.post("/redemarrer", self._redemarrer),
            web.post("/couper-voix", self._couper_voix),
            web.get("/voix", self._voix), web.post("/voix", self._choisir_voix),
            web.post("/selection", self._selection),
            web.get("/api/{nom}", self._api), web.post("/api/{nom}", self._api),
            web.get("/extension", self._extension),
            web.post("/cerveau/{quoi}", self._cerveau),
        ])
        lanceur = web.AppRunner(appli, access_log=None)
        try:
            boucle.run_until_complete(lanceur.setup())
            boucle.run_until_complete(web.TCPSite(lanceur, "127.0.0.1", self.port).start())
            log.info("API locale : http://127.0.0.1:%d", self.port)
        except OSError as e:
            self.erreur = e
            log.error("Port %d indisponible : %s (Jarvis tourne déjà ?)", self.port, e)
            self.pret.set()
            return
        self.pret.set()
        boucle.run_forever()

    @web.middleware
    async def _securite(self, request, handler):      # (aiohttp impose ces deux noms)
        origine = request.headers.get("Origin", "")
        if origine and not origine.startswith("chrome-extension://"):
            return web.json_response({"ok": False, "erreur": "requêtes venant d'une page web refusées"}, status=403)
        hote = request.host.rsplit(":", 1)[0].lower()
        if hote not in ("127.0.0.1", "localhost", "[::1]"):
            return web.json_response({"ok": False, "erreur": "hôte refusé"}, status=403)
        return await handler(request)

    async def _json(self, requete) -> dict:
        try:
            donnees = await requete.json()
            return donnees if isinstance(donnees, dict) else {}
        except Exception:  # noqa: BLE001
            return {}

    async def _accueil(self, _r):
        return web.json_response({"ok": True, "nom": "Jarvis", "routes": [
            "GET /etat", "POST /demander", "POST /ecouter", "POST /micro", "POST /oublier", "POST /dire",
            "POST /quitter", "GET /extension (WebSocket)"]})

    async def _etat(self, _r):
        e = self.etat
        return web.json_response({"ok": True, "nom": "Jarvis", "etat": e.etat, "micro": e.micro,
                                  "texte": e.dernier_texte, "extension": self.pont.connecte})

    async def _demander(self, requete):
        d = await self._json(requete)
        texte = str(d.get("texte", "")).strip()
        if not texte:
            return web.json_response({"ok": False, "erreur": "« texte » manquant"}, status=400)
        cerveau = self.obtenir_cerveau()
        if cerveau is None:
            return web.json_response({"ok": False, "reponse": "Je démarre encore, réessaie dans quelques secondes."})
        source = str(d.get("source", "ile"))
        rep = await asyncio.get_running_loop().run_in_executor(None, cerveau.repondre, texte, source)
        if source != "voix" and self.lire_reponses_ile():
            self.etat.annoncer(rep.texte)
        return web.json_response({"ok": True, "reponse": rep.texte, "attend_confirmation": rep.suivi})

    async def _ecouter(self, _r):
        self.etat.ecouter_maintenant.set()
        return web.json_response({"ok": True})

    async def _micro(self, requete):
        d = await self._json(requete)
        self.etat.micro = bool(d.get("actif", not self.etat.micro))
        return web.json_response({"ok": True, "micro": self.etat.micro})

    async def _oublier(self, _r):
        cerveau = self.obtenir_cerveau()
        if cerveau:
            cerveau.oublier()
        return web.json_response({"ok": True})

    async def _dire(self, requete):
        d = await self._json(requete)
        self.etat.annoncer(str(d.get("texte", "")).strip())
        return web.json_response({"ok": True})

    async def _quitter(self, _r):
        self.etat.quitter.set()
        return web.json_response({"ok": True})

    async def _voix(self, _r):
        from .tts import CATALOGUE
        return web.json_response({"ok": True, "actuelle": self.voix_actuelle() if self.voix_actuelle else None,
                                  "voix": [{"nom": n, "titre": v["titre"]} for n, v in CATALOGUE.items()]})

    async def _choisir_voix(self, requete):
        from .tts import CATALOGUE
        nom = str((await self._json(requete)).get("nom", ""))
        if nom not in CATALOGUE or not self.changer_voix:
            return web.json_response({"ok": False, "erreur": f"voix inconnue : {nom}"}, status=400)
        threading.Thread(target=self.changer_voix, args=(nom,), daemon=True).start()
        return web.json_response({"ok": True, "voix": nom})

    async def _selection(self, requete):
        """L'île dit ce que tu regardes (ex : un mail) : « fais-moi un résumé » parlera de ça."""
        import time
        d = await self._json(requete)
        self.etat.selection = {**d, "t": time.time()} if d.get("type") else None
        return web.json_response({"ok": True})

    # ── données de l'appli (la grande île de Givre les lit et les modifie) ──
    async def _api(self, requete):
        cerveau = self.obtenir_cerveau()
        fonction = cerveau.reg.apis.get(requete.match_info["nom"]) if cerveau else None
        if not fonction:
            return web.json_response({"ok": False, "erreur": "inconnu ou Jarvis démarre encore"}, status=404)
        donnees = await self._json(requete) if requete.method == "POST" else dict(requete.query)
        try:
            resultat = await asyncio.get_running_loop().run_in_executor(None, fonction, requete.method, donnees)
        except Exception as e:  # noqa: BLE001
            log.exception("api %s", requete.match_info["nom"])
            return web.json_response({"ok": False, "erreur": str(e)}, status=500)
        return web.json_response(resultat if isinstance(resultat, dict) else {"ok": True, "donnees": resultat})

    async def _cerveau(self, requete):
        """Le cerveau Rust (dans la barre) lit et appelle ici les outils, raccourcis et données des modules."""
        cerveau = self.obtenir_cerveau()
        if cerveau is None:
            return web.json_response({"ok": False, "erreur": "Jarvis démarre encore"}, status=503)
        d = await self._json(requete)
        quoi = requete.match_info["quoi"]
        actions = {
            "avant": lambda: cerveau.avant(str(d.get("texte", ""))),
            "outils": lambda: {"outils": cerveau.reg.schemas()},
            "outil": lambda: cerveau.executer_outil(str(d.get("nom", "")), d.get("args") or {}),
            "confirmer": lambda: cerveau.confirmer(d.get("id") or 0),
            "contexte": lambda: {"texte": cerveau._contexte(str(d.get("question", "")))},
            "apres": lambda: cerveau.apres_echange(str(d.get("question", "")), str(d.get("reponse", ""))) or {},
        }
        if quoi not in actions:
            return web.json_response({"ok": False, "erreur": "route inconnue"}, status=404)
        try:
            resultat = await asyncio.get_running_loop().run_in_executor(None, actions[quoi])
        except Exception as e:  # noqa: BLE001
            log.exception("cerveau/%s", quoi)
            return web.json_response({"ok": False, "erreur": str(e)}, status=500)
        return web.json_response({"ok": True, **resultat})

    async def _couper_voix(self, _r):
        """Coupe la voix (ce moteur Python) pour libérer la mémoire : la barre reste, Jarvis.exe attend
        qu'elle redemande la voix."""
        self.etat.couper_voix = True
        self.etat.quitter.set()
        return web.json_response({"ok": True})

    async def _redemarrer(self, _r):
        """Relit config.toml et recharge tout (Jarvis.exe relance le moteur)."""
        self.etat.redemarrer = True
        self.etat.quitter.set()
        return web.json_response({"ok": True})

    async def _extension(self, requete):
        if not requete.headers.get("Origin", "").startswith("chrome-extension://"):
            return web.json_response({"ok": False, "erreur": "réservé à l'extension Chrome"}, status=403)
        ws = web.WebSocketResponse(heartbeat=None, max_msg_size=1024 * 1024)
        await ws.prepare(requete)
        await self.pont.gerer(ws)
        return ws
