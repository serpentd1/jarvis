"""Sites web et recherches : « ouvre YouTube », « cherche des chats sur YouTube »."""
import json
import logging
import os
import re
import urllib.parse

from . import Resultat, normaliser

log = logging.getLogger("web")

RECHERCHES = {
    "google": "https://www.google.com/search?q={}",
    "youtube": "https://www.youtube.com/results?search_query={}",
    "wikipedia": "https://fr.wikipedia.org/w/index.php?search={}",
    "images": "https://www.google.com/search?tbm=isch&q={}",
    "maps": "https://www.google.com/maps/search/{}",
}
# sites qu'on ferme par leurs onglets même s'ils ne sont pas dans [sites] de config.toml
CONNUS = {"youtube", "pronote", "netflix", "twitch", "gmail", "instagram", "tiktok", "google", "wikipedia",
          "chatgpt", "ent", "vinted", "amazon", "outlook", "hotmail", "twitter", "facebook", "reddit"}
SUR_CHROME = re.compile(r" (?:sur|dans|avec|en) (?:google )?chrome$")
NAVIGATEUR = ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) "
              "Chrome/131.0 Safari/537.36")
# Dans la page de résultats de YouTube, les vraies vidéos sont des « videoRenderer » ; les pubs sont des
# « adSlotRenderer » / « promotedVideoRenderer » et les Shorts des « reelItemRenderer » : on ne les prend pas.
VIDEO = re.compile(r'"videoRenderer":\{"videoId":"(?P<id>[\w-]{11})".{0,3000}?"title":\{"runs":\[\{"text":"(?P<titre>(?:[^"\\]|\\.)*)"',
                   re.S)


def premieres_videos(html: str, n: int = 5) -> list[tuple[str, str]]:
    """[(id, titre)] des premières vraies vidéos (sans pubs ni Shorts) d'une page de résultats YouTube."""
    vus, res = set(), []
    for m in VIDEO.finditer(html or ""):
        if m["id"] in vus:
            continue
        vus.add(m["id"])
        try:
            titre = json.loads(f'"{m["titre"]}"')
        except ValueError:
            titre = m["titre"]
        res.append((m["id"], titre))
        if len(res) >= n:
            break
    return res


def chercher_youtube(requete: str) -> list[tuple[str, str]]:
    import requests
    r = requests.get("https://www.youtube.com/results", params={"search_query": requete, "hl": "fr"},
                     headers={"User-Agent": NAVIGATEUR, "Accept-Language": "fr-FR,fr;q=0.9"},
                     cookies={"CONSENT": "YES+cb", "SOCS": "CAI"}, timeout=8)
    r.raise_for_status()
    return premieres_videos(r.text)


def installer(ctx, reg):
    sites = {normaliser(k): v for k, v in ctx.cfg["sites"].items()}

    def url_de(site: str):
        cle = normaliser(site).removeprefix("le site ").removeprefix("site ").strip()
        cle = SUR_CHROME.sub("", cle).strip()      # « youtube sur chrome » -> « youtube »
        site = re.sub(r"\s+(?:sur|dans|avec|en)\s+(?:google\s+)?chrome\s*$", "", site.strip(), flags=re.I)
        if cle in sites:
            return sites[cle]
        if re.match(r"^https?://\S+$", site.strip()):
            return site.strip()                 # adresse complète (ex. un lien trouvé par web_search) : telle quelle
        if re.fullmatch(r"[a-z0-9-]+(\.[a-z0-9-]+)+(/\S*)?", site.strip().lower()):
            return "https://" + site.strip().lower().removeprefix("https://").removeprefix("http://")
        return None

    def ouvrir_site(site: str, silencieux: bool = False):
        url = url_de(site)
        if not url:
            # « ouvre le site Chrome » : l'IA confond parfois site et application
            index = getattr(ctx, "index_applis", None)
            if not silencieux and index and index.chercher(site) and hasattr(ctx, "lancer_application"):
                return ctx.lancer_application(site)
            return None if silencieux else Resultat(f"Je ne connais pas le site {site}.", ok=False)
        # Le site est déjà ouvert dans Chrome ? On remet son onglet devant au lieu d'en ouvrir un deuxième.
        adresse = urllib.parse.urlparse(url)
        if adresse.path in ("", "/") and not adresse.query:
            r = commande_onglet("onglet_aller", cherche=adresse.netloc.removeprefix("www."), delai=3)
            if r.get("ok"):
                log.info("site déjà ouvert : %s", url)
                return Resultat(f"{site.strip()} est déjà ouvert : je te remets dessus.")
        ouvrir_url(url)
        log.info("site : %s", url)
        nom = adresse.netloc.removeprefix("www.") if re.match(r"^https?://", site.strip()) else site.strip()
        return Resultat(f"J'ouvre {nom}.")

    ctx.ouvrir_site = ouvrir_site     # utilisé par « lance … » quand ce n'est pas une application

    # ── onglets de Chrome (par l'extension de Jarvis) ──
    def commande_onglet(commande: str, delai: float = 5, **params) -> dict:
        pont = getattr(ctx, "pont", None)
        if pont is None or not pont.connecte:
            return {"ok": False, "erreur": "extension"}
        return pont.commande({"commande": commande, **params}, delai)

    def ouvrir_url(url: str):
        """Dans Chrome, par l'extension (nouvel onglet, Chrome devant) ; sinon dans le navigateur par défaut."""
        if commande_onglet("ouvrir", url=url).get("ok"):
            return
        os.startfile(url)

    def echec_onglet(r: dict, cherche: str) -> Resultat:
        erreur = str(r.get("erreur", ""))
        if erreur == "extension" or "pas connectée" in erreur:
            return Resultat("L'extension Jarvis n'est pas branchée dans Chrome : je ne vois pas tes onglets.", ok=False)
        if "commande inconnue" in erreur:
            return Resultat("Recharge l'extension Jarvis dans Chrome (page chrome://extensions, bouton ↻) : "
                            "elle ne sait pas encore gérer les onglets.", ok=False)
        return Resultat(f"Je ne trouve pas d'onglet {cherche} dans Chrome.", ok=False)

    def nettoyer_onglet(cherche: str) -> str:
        c = normaliser(cherche or "")
        c = re.sub(r"^(?:l'?\s?onglet|les onglets|la page|le site|l'?\s?onglet de|onglet)\s+", "", c)
        return re.sub(r"^(?:de |du |d'|sur )", "", c).strip()

    def aller_onglet(cherche: str):
        cherche = nettoyer_onglet(cherche)
        r = commande_onglet("onglet_aller", cherche=cherche)
        if not r.get("ok"):
            return echec_onglet(r, cherche)
        return Resultat(f"Je te remets sur l'onglet {r.get('titre') or cherche}.")

    def fermer_onglet(cherche: str = "", garder_un: bool = False):
        cherche = nettoyer_onglet(cherche)
        if not cherche or cherche in ("en double", "en trop", "doublons", "doubles"):
            r = commande_onglet("onglet_doublons")
            if not r.get("ok"):
                return echec_onglet(r, "")
            n = r.get("fermes", 0)
            return Resultat("Aucun onglet en double." if not n else f"J'ai fermé {n} onglet{'s' if n > 1 else ''} en double.")
        r = commande_onglet("onglet_fermer", cherche=cherche, garder_un=bool(garder_un))
        url = url_de(cherche)
        if not r.get("ok") and url and r.get("erreur") != "extension":
            # « pronote » : le titre de l'onglet ne dit pas toujours « Pronote », mais son adresse, oui
            hote = urllib.parse.urlparse(url).netloc.removeprefix("www.")
            if hote and hote != cherche:
                r = commande_onglet("onglet_fermer", cherche=hote, garder_un=bool(garder_un))
        if not r.get("ok"):
            return echec_onglet(r, cherche)
        n = r.get("fermes", 0)
        if garder_un:
            if not n:
                return Resultat(f"Tu n'as qu'un seul onglet {cherche}.")
            return Resultat(f"J'ai fermé {n} onglet{'s' if n > 1 else ''} {cherche} en trop : je garde « {r.get('garde', '')} ».")
        return Resultat(f"J'ai fermé l'onglet {cherche}." if n == 1 else f"J'ai fermé {n} onglets {cherche}.")

    reg.outil("aller_onglet", "Remet devant un onglet déjà ouvert dans Chrome (et Chrome au premier plan).",
              {"cherche": "nom du site ou mot du titre de l'onglet, ex. youtube"}, ["cherche"], aller_onglet)
    reg.outil("fermer_onglet", "Ferme des onglets de Chrome : ceux d'un site (ex. pronote), ou les onglets en double "
              "(cherche vide), ou tous ceux d'un site sauf un (garder_un).",
              {"cherche": "nom du site ou mot du titre ; vide = les onglets en double",
               "garder_un": {"type": "boolean", "description": "vrai = garder un seul onglet de ce site"}},
              [], fermer_onglet)
    reg.regle(r"^(?:ferme|fermer|enleve|supprime)(?: moi)? (?:les onglets|les pages) (?:en double|en trop|doubles?)$",
              lambda m, t: [("fermer_onglet", {"cherche": ""})], priorite=12)
    reg.regle(r"^(?:ferme|fermer|enleve)(?: moi)? les (?:autres onglets (?P<q1>.+)|onglets (?P<q2>.+?) en (?:double|trop))$",
              lambda m, t: [("fermer_onglet", {"cherche": m["q1"] or m["q2"], "garder_un": True})], priorite=12)
    reg.regle(r"^(?:ferme|fermer|enleve)(?: moi)? (?:tous )?les onglets (?P<q>.+)$",
              lambda m, t: [("fermer_onglet", {"cherche": m["q"]})], priorite=13)
    reg.regle(r"^(?:garde|laisse)(?: moi)? (?:qu'?\s?)?un seul (?:onglet )?(?P<q>.+)$",
              lambda m, t: [("fermer_onglet", {"cherche": m["q"], "garder_un": True})], priorite=13)
    reg.regle(r"^(?:ferme|fermer|enleve|supprime)(?: moi)? (?:l'?\s?onglet|la page) (?P<q>.+)$",
              lambda m, t: [("fermer_onglet", {"cherche": m["q"]})], priorite=13)
    reg.regle(r"^(?:va|vas|retourne|reviens|passe|remets?)(?: moi)?(?: sur| a)? l'?\s?onglet (?P<q>.+)$",
              lambda m, t: [("aller_onglet", {"cherche": m["q"]})], priorite=13)

    def recherche_web(requete: str, ou: str = "google"):
        ou = normaliser(ou or "google")
        modele = next((v for k, v in RECHERCHES.items() if k in ou), RECHERCHES["google"])
        ouvrir_url(modele.format(urllib.parse.quote_plus(requete)))
        nom = next((k for k in RECHERCHES if k in ou), "google").capitalize()
        return Resultat(f"Je cherche « {requete} » sur {nom}.")

    reg.outil("ouvrir_site", "Ouvre un site web dans Chrome (YouTube, Gmail, Netflix, Twitch… ou une adresse complète).",
              {"site": "nom du site ou adresse"}, ["site"], ouvrir_site)
    reg.outil("recherche_web", "Ouvre une recherche dans Chrome pour que l'utilisateur la voie (Google, YouTube…). "
              "Pour trouver toi-même une réponse, utilise plutôt web_search.",
              {"requete": "ce qu'il faut chercher",
               "ou": {"type": "string", "enum": list(RECHERCHES), "description": "où chercher (google par défaut)"}},
              ["requete"], recherche_web)

    def jouer_youtube(recherche: str = ""):
        """La première vraie vidéo (pas une pub, pas un Short) d'une recherche YouTube, ouverte directement.
        Sans recherche : celle de la page de résultats YouTube ouverte dans Chrome."""
        recherche = (recherche or "").strip()
        if not recherche:
            r = commande_onglet("onglet_texte", delai=6)
            q = urllib.parse.parse_qs(urllib.parse.urlparse(str(r.get("url", ""))).query).get("search_query")
            if not q:
                return Resultat("Dis-moi quelle vidéo chercher, par exemple : mets la première vidéo de Squeezie "
                                "sur YouTube.", ok=False)
            recherche = q[0]
        try:
            videos = chercher_youtube(recherche)
        except Exception as e:  # noqa: BLE001
            log.warning("YouTube « %s » : %s", recherche, e)
            videos = []
        if not videos:
            ouvrir_url(RECHERCHES["youtube"].format(urllib.parse.quote_plus(recherche)))
            return Resultat(f"Je n'arrive pas à lire les résultats de YouTube : je t'ouvre la recherche « {recherche} ».",
                            ok=False)
        vid, titre = videos[0]
        ouvrir_url(f"https://www.youtube.com/watch?v={vid}")
        log.info("YouTube « %s » -> %s", recherche, vid)
        return Resultat(f"Je mets « {titre} » sur YouTube." if titre else "Je mets la première vidéo sur YouTube.")

    reg.outil("jouer_youtube", "Ouvre directement la première VRAIE vidéo YouTube d'une recherche (en sautant les "
              "publicités et les Shorts). Sans recherche : prend la page de résultats YouTube ouverte dans Chrome.",
              {"recherche": "ce qu'il faut chercher sur YouTube (vide = la recherche déjà ouverte)"}, [], jouer_youtube)
    reg.regle(r"^(?:mets?|met|lance|joue|ouvre|prends?|clique sur|choisis)(?: moi)? (?:la )?premiere video"
              r"(?: qui n'?\s?est pas (?:une |de )?pub(?:licite)?| sans pub(?:licite)?| pas sponsorisee)?"
              r"(?: (?:de|sur|pour|avec) (?P<q>.+?))?(?: sur youtube)?$",
              lambda m, t: [("jouer_youtube", {"recherche": (m["q"] or "").strip()})], priorite=11)
    reg.regle(r"^(?:mets?|met|lance|joue)(?: moi)? (?:la video |une video |la musique |le clip )?(?P<q>.+?) sur youtube$",
              lambda m, t: [("jouer_youtube", {"recherche": m["q"]})], priorite=39)

    def regle_site(m, _texte):
        return [("ouvrir_site", {"site": m["site"]})] if url_de(m["site"]) else None

    # « Lance YouTube sur Chrome » (passe avant les applis : sinon on ouvrait juste Chrome)
    def regle_site_chrome(m, _texte):
        if normaliser(m["site"]) in ("pronote", "mon pronote"):
            return [("ouvrir_givre", {"quoi": "pronote"})]
        return [("ouvrir_site", {"site": m["site"]})]

    reg.regle(r"^(?:tu peux |peux tu )?(?:me )?(?:ouvre|ouvrir|va sur|vas sur|lance|lanche|lancer|mets?)(?: moi)? "
              r"(?:le site )?(?P<site>.+?) (?:sur|dans|avec|en) (?:google )?chrome$", regle_site_chrome, priorite=30)

    # « Ferme Pronote dans Chrome » / « ferme YouTube » (un site connu qui n'est pas une appli) : ferme ses onglets
    reg.regle(r"^(?:tu peux |peux tu )?(?:ferme|fermer|quitte|quitter|enleve)(?: moi)? (?:le site |l'onglet )?"
              r"(?P<q>.+?) (?:sur|dans|de) (?:google )?chrome$",
              lambda m, t: [("fermer_onglet", {"cherche": m["q"]})], priorite=12)

    def regle_fermer_site(m, _texte):
        index = getattr(ctx, "index_applis", None)
        if index and index.chercher(m["q"]):
            return None                          # une vraie appli : fermer_application s'en occupe
        return [("fermer_onglet", {"cherche": m["q"]})] if normaliser(m["q"]) in sites or m["q"] in CONNUS else None

    reg.regle(r"^(?:tu peux |peux tu )?(?:ferme|fermer|quitte|quitter)(?: moi)? (?P<q>[a-z0-9' ]+)$",
              regle_fermer_site, priorite=82)

    def regle_recherche(m, texte):
        # on reprend la recherche dans le texte d'origine (avec les accents)
        brut = re.search(r"(?:cherche|recherche|trouve)(?:[- ]moi)?\s+(.+?)\s+sur\s+(\S+(?:\s\S+)?)\s*[.!?]?$",
                         texte, re.IGNORECASE)
        requete = brut.group(1) if brut else m["q"]
        return [("recherche_web", {"requete": requete, "ou": m["ou"]})]

    reg.regle(r"^(?:tu peux |peux tu )?(?:me )?(?:ouvre|ouvrir|va sur|vas sur|lance)(?: moi)? (?:le site )?(?P<site>.+)$",
              regle_site, priorite=85)
    reg.regle(r"^(?:cherche|recherche|trouve)(?: moi)? (?P<q>.+) sur (?P<ou>google|youtube|wikipedia|google maps|maps|images)$",
              regle_recherche, priorite=40)
