"""Sites web et recherches : « ouvre YouTube », « cherche des chats sur YouTube »."""
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


def installer(ctx, reg):
    sites = {normaliser(k): v for k, v in ctx.cfg["sites"].items()}

    def url_de(site: str):
        cle = normaliser(site).removeprefix("le site ").removeprefix("site ").strip()
        if cle in sites:
            return sites[cle]
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
        os.startfile(url)
        log.info("site : %s", url)
        return Resultat(f"J'ouvre {site.strip()}.")

    ctx.ouvrir_site = ouvrir_site     # utilisé par « lance … » quand ce n'est pas une application

    # ── onglets de Chrome (par l'extension de Jarvis) ──
    def commande_onglet(commande: str, delai: float = 5, **params) -> dict:
        pont = getattr(ctx, "pont", None)
        if pont is None or not pont.connecte:
            return {"ok": False, "erreur": "extension"}
        return pont.commande({"commande": commande, **params}, delai)

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
        os.startfile(modele.format(urllib.parse.quote_plus(requete)))
        nom = next((k for k in RECHERCHES if k in ou), "google").capitalize()
        return Resultat(f"Je cherche « {requete} » sur {nom}.")

    reg.outil("ouvrir_site", "Ouvre un site web dans le navigateur (YouTube, Gmail, Netflix, Twitch… ou une adresse).",
              {"site": "nom du site ou adresse"}, ["site"], ouvrir_site)
    reg.outil("recherche_web", "Lance une recherche sur Internet.",
              {"requete": "ce qu'il faut chercher",
               "ou": {"type": "string", "enum": list(RECHERCHES), "description": "où chercher (google par défaut)"}},
              ["requete"], recherche_web)

    def regle_site(m, _texte):
        return [("ouvrir_site", {"site": m["site"]})] if url_de(m["site"]) else None

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
