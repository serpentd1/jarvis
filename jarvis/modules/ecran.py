"""Jarvis regarde ton écran : « qu'est-ce qu'il y a sur mon écran ? », « explique-moi cette vidéo »,
« clique sur S'abonner ». L'IA locale (qwen3.5) lit la capture d'écran : rien ne part sur Internet."""
import base64
import io
import json
import logging
import re

import requests

from core import controle
from . import Resultat

log = logging.getLogger("ecran")
RISQUE = re.compile(r"supprim|effac|pay|achat|achet|command|envoy|publi|d[ée]sinstall|format|vider|r[ée]initialis",
                    re.IGNORECASE)


def en_base64(image) -> str:
    tampon = io.BytesIO()
    image.save(tampon, "PNG")
    return base64.b64encode(tampon.getvalue()).decode()


def installer(ctx, reg):
    cfg = ctx.cfg["cerveau"]

    def voir(consigne: str, image, temperature: float = 0.3) -> str | None:
        corps = {"model": cfg["modele"], "stream": False, "think": False, "keep_alive": cfg["garder_en_memoire"],
                 "options": {"temperature": temperature, "num_ctx": int(cfg["contexte"]), "num_predict": 300},
                 "messages": [{"role": "user", "content": consigne, "images": [en_base64(image)]}]}
        try:
            r = requests.post(f"{cfg['ollama_url'].rstrip('/')}/api/chat", json=corps, timeout=120)
            if r.status_code >= 400:
                log.error("vision : %s", r.text[:200])
                return None
            return re.sub(r"<think>.*?</think>", "", r.json().get("message", {}).get("content", ""), flags=re.S).strip()
        except Exception:  # noqa: BLE001
            log.exception("vision")
            return None

    def regarder_ecran(question: str = ""):
        image = controle.capture()
        q = question.strip() or "Qu'est-ce qu'il y a sur mon écran ? Explique-le-moi."
        reponse = voir(f"Voici une capture de mon écran. {q}\nRéponds en français, en me tutoyant, en 2 à 4 phrases "
                       "courtes faites pour être lues à voix haute (pas de liste, pas de markdown).", image)
        if not reponse:
            return Resultat("Je n'arrive pas à lire l'écran : le modèle d'IA ne sait peut-être pas lire les images.",
                            ok=False)
        return Resultat(reponse)

    def cliquer_sur(element: str, double: bool = False, droit: bool = False):
        image = controle.capture()
        reponse = voir(
            f"Sur cette capture d'écran, trouve : « {element} ». Réponds UNIQUEMENT par un objet JSON "
            '{"trouve": true, "x": X, "y": Y} où X et Y sont entre 0 et 1000 (0,0 = coin en haut à gauche, '
            '1000,1000 = coin en bas à droite), ou {"trouve": false} si tu ne le vois pas.', image, temperature=0.0)
        m = re.search(r"\{.*\}", reponse or "", re.S)
        try:
            j = json.loads(m.group(0)) if m else {}
        except ValueError:
            j = {}
        if not j.get("trouve") or "x" not in j or "y" not in j:
            return Resultat(f"Je ne vois pas « {element} » sur ton écran.", ok=False)
        largeur, hauteur = controle.taille_ecran()
        x = round(max(0, min(1000, float(j["x"]))) / 1000 * (largeur - 1))
        y = round(max(0, min(1000, float(j["y"]))) / 1000 * (hauteur - 1))
        log.info("clic sur « %s » en (%d, %d)", element, x, y)

        def clic():
            controle.cliquer(x, y, droit=droit, double=double)
            return Resultat(f"J'ai cliqué sur {element}.")
        if RISQUE.search(element):
            controle.deplacer(x, y)          # on montre où on va cliquer
            return Resultat(f"Je mets la souris sur {element}. Je clique ?", confirmer=clic,
                            annuler="D'accord, je ne clique pas.")
        return clic()

    ctx.cliquer_sur = cliquer_sur        # utilisé par Deezer pour appuyer sur « Écouter »
    reg.outil("regarder_ecran",
              "Regarde l'écran de l'utilisateur (capture d'écran) et répond à sa question sur ce qu'il voit : vidéo, "
              "page, exercice, message d'erreur, jeu…",
              {"question": "ce que l'utilisateur veut savoir sur son écran"}, [], regarder_ecran)
    reg.outil("cliquer_sur", "Trouve un bouton, un lien ou un élément sur l'écran et clique dessus avec la souris.",
              {"element": "ce sur quoi cliquer, tel que l'utilisateur le décrit (ex : le bouton S'abonner)",
               "double": {"type": "boolean", "description": "double-clic"},
               "droit": {"type": "boolean", "description": "clic droit"}},
              ["element"], cliquer_sur)

    reg.regle(r"\b(regarde|analyse|decris|lis|explique)\b.*\b(ecran|video|image|page|photo|ca)\b|"
              r"\b(sur|a) (mon |l'|l |cet |l')?ecran\b|^(?:qu'est ce que|c'est quoi) (?:je vois|ca|que je vois)$|"
              r"\btu vois (quoi|ce que|ce qu'il)",
              lambda m, t: [("regarder_ecran", {"question": t})], priorite=22)
    reg.regle(r"^(?:double )?(?:clique|clic|cliquer|clique droit)(?: moi)? (?:sur|dans) (?P<el>.+)$",
              lambda m, t: [("cliquer_sur", {"element": re.split(r"\bsur\b|\bdans\b", t, maxsplit=1)[-1].strip(" .!?"),
                                             "double": "double" in m.group(0), "droit": "droit" in m.group(0)})],
              priorite=20)
