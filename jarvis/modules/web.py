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
        os.startfile(url)
        log.info("site : %s", url)
        return Resultat(f"J'ouvre {site.strip()}.")

    ctx.ouvrir_site = ouvrir_site     # utilisé par « lance … » quand ce n'est pas une application

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
