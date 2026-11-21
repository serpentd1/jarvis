"""Jarvis regarde ton écran : « qu'est-ce qu'il y a sur mon écran ? », « explique-moi cette vidéo »,
« clique sur S'abonner ». L'IA lit la capture d'écran : celle que tu as choisie dans la barre (LM Studio sur ce PC
par défaut, rien ne part sur Internet ; une clé d'API en ligne, elle, envoie l'image à son service)."""
import base64
import io
import json
import logging
import os
import re
import subprocess
import tempfile
import threading
import time
from pathlib import Path

import requests

from core import controle, ollama
from . import Resultat

log = logging.getLogger("ecran")
RISQUE = re.compile(r"supprim|effac|pay|achat|achet|command|envoy|publi|d[ée]sinstall|format|vider|r[ée]initialis",
                    re.IGNORECASE)


def en_base64(image) -> str:
    tampon = io.BytesIO()
    image.save(tampon, "PNG")
    return base64.b64encode(tampon.getvalue()).decode()


# ── Lecture du texte de l'écran (OCR de Windows : rien à installer, marche sans Internet) ─────────────────────
OCR_PS = r'''
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$null = [Windows.Storage.StorageFile,Windows.Storage,ContentType=WindowsRuntime]
$null = [Windows.Media.Ocr.OcrEngine,Windows.Foundation,ContentType=WindowsRuntime]
$null = [Windows.Graphics.Imaging.BitmapDecoder,Windows.Foundation,ContentType=WindowsRuntime]
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
function Await($op, $type) { $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op)); $t.Wait(-1) | Out-Null; $t.Result }
$file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($env:JARVIS_OCR_IMAGE)) ([Windows.Storage.StorageFile])
$stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
$decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
$bitmap = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
$engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages()
$result = Await ($engine.RecognizeAsync($bitmap)) ([Windows.Media.Ocr.OcrResult])
foreach ($l in $result.Lines) { $l.Text }
'''


def ocr(image) -> str:
    """Le texte d'une image (capture ou zone), lu par l'OCR de Windows."""
    chemin = Path(tempfile.gettempdir()) / f"jarvis-ocr-{os.getpid()}-{threading.get_ident()}.png"
    image.save(chemin, "PNG")
    try:
        code = base64.b64encode(OCR_PS.encode("utf-16-le")).decode()
        r = subprocess.run(["powershell", "-NoProfile", "-NonInteractive", "-EncodedCommand", code], capture_output=True,
                           timeout=40, env={**os.environ, "JARVIS_OCR_IMAGE": str(chemin)}, creationflags=0x08000000)
        return r.stdout.decode("utf-8", "replace").strip()
    except (OSError, subprocess.TimeoutExpired):
        return ""
    finally:
        chemin.unlink(missing_ok=True)


def mode_puissant() -> bool:
    """Mode ultra performant (clé NVIDIA) : réglé dans la grande application, lu dans la barre."""
    try:
        return bool(requests.get(f"{ollama.url_barre()}/ia/etat", timeout=2).json().get("puissant"))
    except Exception:  # noqa: BLE001
        return False


def presse_papiers_image():
    from PIL import ImageGrab
    try:
        im = ImageGrab.grabclipboard()
    except Exception:  # noqa: BLE001
        return None
    return im if hasattr(im, "size") else None


# Pilotage de l'écran (mode ultra performant seulement) : l'IA voit l'écran et appelle UN outil par tour.
def _outil(nom: str, desc: str, props: dict, requis: list) -> dict:
    return {"type": "function", "function": {"name": nom, "description": desc,
                                             "parameters": {"type": "object", "properties": props, "required": requis}}}


_N = {"type": "number"}
_T = {"type": "string"}
OUTILS_ACTION = [
    _outil("cliquer", "Clique à un endroit de l'écran (x et y de 0 à 1000).",
           {"x": _N, "y": _N, "quoi": {"type": "string", "description": "ce sur quoi tu cliques"},
            "double": {"type": "boolean"}, "droit": {"type": "boolean"}}, ["x", "y", "quoi"]),
    _outil("taper", "Tape du texte là où est le curseur.", {"texte": _T}, ["texte"]),
    _outil("touches", "Appuie sur une touche ou un raccourci : entree, echap, tab, ctrl+l, alt+tab…", {"touches": _T}, ["touches"]),
    _outil("defiler", "Fait défiler (molette).", {"direction": {"type": "string", "enum": ["haut", "bas"]}}, ["direction"]),
    _outil("attendre", "Attend que la page ou l'appli charge.", {"secondes": _N}, ["secondes"]),
    _outil("demander", "Demande la permission AVANT une action risquée : envoyer un message, supprimer, payer, acheter, "
           "publier.", {"question": _T}, ["question"]),
    _outil("fini", "L'objectif est atteint (ou impossible) : une phrase pour l'utilisateur.", {"reponse": _T}, ["reponse"]),
]
CONSIGNE_ACTION = ("Tu pilotes le PC de l'utilisateur pour atteindre son objectif. À chaque tour tu vois l'écran "
                   "(coordonnées de 0 à 1000 : 0,0 en haut à gauche, 1000,1000 en bas à droite) et tu appelles UN seul "
                   "outil. Vise le milieu de ce que tu veux cliquer. Avant d'envoyer un message, de supprimer, de payer, "
                   "d'acheter ou de publier, appelle « demander ». Quand l'objectif est atteint ou impossible, appelle "
                   "« fini » avec une phrase courte, en français, en tutoyant.")


def installer(ctx, reg):
    cfg = ctx.cfg["cerveau"]

    def voir(consigne: str, image, temperature: float = 0.3, max_jetons: int = 300) -> str | None:
        # Passe par la barre (/ia/discuter) : l'image va dans "images" du message.
        return ollama.discuter(cfg, [{"role": "user", "content": consigne, "images": [en_base64(image)]}],
                               temperature=temperature, max_mots=max_jetons)

    ctx.voir = voir          # aussi pour lire une leçon à l'écran (modules/lecons.py)

    def regarder_ecran(question: str = ""):
        image = controle.capture()
        q = question.strip() or "Qu'est-ce qu'il y a sur mon écran ? Explique-le-moi."
        reponse = voir(f"Voici une capture de mon écran. {q}\nRéponds en français, en me tutoyant, en 2 à 4 phrases "
                       "courtes faites pour être lues à voix haute (pas de liste, pas de markdown).", image)
        if not reponse:
            # le petit modèle ne voit pas les images : on lit au moins le texte de l'écran (OCR)
            texte = ocr(controle.capture(4000))
            if texte:
                return Resultat("Voilà ce qui est écrit à l'écran.", pour_ia=f"Texte lu à l'écran (OCR) :\n{texte[:6000]}"
                                f"\n\nSa question : {q}")
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


    # ── OCR : lire le texte de l'écran (léger, marche aussi en mode local) ──
    def lire_texte_ecran(question: str = ""):
        texte = ocr(controle.capture(4000))
        if not texte:
            return Resultat("Je ne lis aucun texte sur ton écran.", ok=False)
        return Resultat("Voilà ce qui est écrit à l'écran.", pour_ia=f"Texte lu à l'écran (OCR, de haut en bas) :\n"
                        f"{texte[:6000]}\n\nCe que l'utilisateur demande : {question or 'lis-le-lui ou résume-le'}")

    # ── Zone ciblée : l'outil Capture de Windows s'ouvre, tu sélectionnes un bout d'écran, je réponds dessus ──
    def zone_ecran(question: str = ""):
        from PIL import ImageChops
        avant = presse_papiers_image()
        subprocess.Popen(["explorer.exe", "ms-screenclip:"])
        image, fin = None, time.time() + 45
        while time.time() < fin:
            time.sleep(0.4)
            im = presse_papiers_image()
            if im is None:
                continue
            if avant is None or im.size != avant.size or ImageChops.difference(im.convert("RGB"), avant.convert("RGB")).getbbox():
                image = im
                break
        if image is None:
            return Resultat("Je n'ai pas reçu de zone. Redemande-moi, puis sélectionne un bout d'écran avec la souris.",
                            ok=False)
        q = question.strip() or "Explique-moi ce qu'il y a dans cette zone."
        if mode_puissant():
            reponse = voir(f"Voici un bout de mon écran que je viens de sélectionner. {q}\nRéponds en français, en me "
                           "tutoyant, en 2 à 4 phrases courtes faites pour être lues à voix haute.", image.convert("RGB"))
            if reponse:
                return Resultat(reponse)
        texte = ocr(image.convert("RGB"))
        if not texte:
            return Resultat("Je ne lis aucun texte dans cette zone.", ok=False)
        return Resultat("Voilà ce que je lis dans ta zone.", pour_ia=f"Texte lu (OCR) dans la zone que l'utilisateur a "
                        f"sélectionnée :\n{texte[:6000]}\n\nSa question : {q}")

    # ── Clic et action : il voit l'écran et clique ou tape à ta place (mode ultra performant seulement) ──
    def faire(nom: str, a: dict) -> str:
        largeur, hauteur = controle.taille_ecran()
        if nom == "cliquer":
            x = round(max(0, min(1000, float(a.get("x", 500)))) / 1000 * (largeur - 1))
            y = round(max(0, min(1000, float(a.get("y", 500)))) / 1000 * (hauteur - 1))
            controle.cliquer(x, y, droit=bool(a.get("droit")), double=bool(a.get("double")))
            return f"cliqué sur {a.get('quoi', '?')}"
        if nom == "taper":
            controle.taper(str(a.get("texte", "")))
            return f"tapé « {str(a.get('texte', ''))[:60]} »"
        if nom == "touches":
            controle.raccourci([t.strip() for t in str(a.get("touches", "")).split("+") if t.strip()])
            return f"appuyé sur {a.get('touches')}"
        if nom == "defiler":
            controle.molette(5 if a.get("direction") == "haut" else -5)
            return f"défilé vers le {a.get('direction', 'bas')}"
        if nom == "attendre":
            time.sleep(max(0.5, min(5.0, float(a.get("secondes", 1)))))
            return "attendu"
        return ""

    def action_ecran(objectif: str, historique: list | None = None, etape: int = 0):
        if not mode_puissant():
            return Resultat("Je ne pilote l'écran qu'avec le gros modèle NVIDIA (grande application, Paramètres, Modèle "
                            "de l'IA) : le petit modèle n'est pas assez précis pour cliquer à ta place.", ok=False)
        historique = historique if historique is not None else []
        for n in range(etape, 15):
            image = controle.capture(1280)
            messages = [{"role": "system", "content": CONSIGNE_ACTION},
                        {"role": "user", "content": f"Objectif : {objectif}\nDéjà fait : {'; '.join(historique[-8:]) or 'rien'}"
                                                    "\nVoici l'écran maintenant. Quelle est la prochaine action ?",
                         "images": [en_base64(image)]}]
            try:
                r = requests.post(f"{ollama.url_barre()}/ia/discuter", json={"messages": messages, "outils": OUTILS_ACTION,
                                  "max_mots": 200, "temperature": 0.1}, timeout=90).json()
            except Exception:  # noqa: BLE001
                return Resultat("Je n'arrive pas à joindre l'IA pour piloter l'écran.", ok=False)
            appels = r.get("appels") or []
            if not appels:
                return Resultat(r.get("texte") or "Je ne sais pas quoi faire ensuite.", ok=bool(r.get("texte")))
            nom, a = appels[0][0], appels[0][1] if isinstance(appels[0][1], dict) else {}
            log.info("action écran %d : %s %s", n + 1, nom, a)
            if nom == "fini":
                return Resultat(str(a.get("reponse") or "C'est fait."))
            if nom == "demander" or (nom == "cliquer" and RISQUE.search(str(a.get("quoi", "")))):
                question = str(a.get("question") or f"Je vais cliquer sur {a.get('quoi')}.")

                def suite(nom=nom, a=a, n=n):
                    if nom == "cliquer":
                        historique.append(faire(nom, a) + " (avec ton accord)")
                    else:
                        historique.append(f"l'utilisateur a dit oui à : {question}")
                    return action_ecran(objectif, historique, n + 1)
                return Resultat(f"{question} Je continue ?", confirmer=suite, annuler="D'accord, j'arrête là.")
            fait = faire(nom, a)
            if fait:
                historique.append(fait)
            time.sleep(0.8)
        return Resultat("J'ai fait 15 actions sans finir, je m'arrête là. Dis-moi la suite.", ok=False)

    reg.outil("lire_texte_ecran", "Lit tout le texte affiché à l'écran (OCR, rapide, marche sans Internet) : pour lire, "
              "résumer ou répondre sur un texte, un exercice, un message, une erreur.",
              {"question": "ce que l'utilisateur veut savoir sur ce texte"}, [], lire_texte_ecran)
    reg.outil("zone_ecran", "L'utilisateur sélectionne un bout d'écran avec la souris (outil Capture de Windows), puis "
              "Jarvis répond à sa question sur cette zone. Pour « regarde cette zone », « explique ce bout d'écran ».",
              {"question": "sa question sur la zone"}, [], zone_ecran)
    reg.outil("action_ecran", "Pilote le PC à la place de l'utilisateur : voit l'écran, clique, tape, jusqu'à atteindre "
              "l'objectif (plusieurs étapes). Demande la permission avant d'envoyer, supprimer, payer ou publier.",
              {"objectif": "ce qu'il faut faire, en entier (ex. : ouvre les paramètres Bluetooth et active-le)"},
              ["objectif"], action_ecran)
    reg.regle(r"\b(?:lis|lire|lit)\b.*\b(?:ecrit|texte)\b.*\becran\b|^lis (?:moi )?(?:l'|l )?ecran$",
              lambda m, t: [("lire_texte_ecran", {"question": t})], priorite=21)
    reg.regle(r"\b(?:zone|bout|morceau|partie)\b.*\becran\b|\bselectionne(?:r)?\b.*\becran\b",
              lambda m, t: [("zone_ecran", {"question": t})], priorite=21)

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
              r"\btu vois (quoi|ce que|ce qu'il)|"
              r"\b(?:photo|capture|screen|screenshot|copie)\b(?: d'?\s?ecran\b| (?:de|d') (?:mon |l'|l |cet )?ecran\b)",
              lambda m, t: [("regarder_ecran", {"question": t})], priorite=22)
    reg.regle(r"^(?:double )?(?:clique|clic|cliquer|clique droit)(?: moi)? (?:sur|dans) (?P<el>.+)$",
              lambda m, t: [("cliquer_sur", {"element": re.split(r"\bsur\b|\bdans\b", t, maxsplit=1)[-1].strip(" .!?"),
                                             "double": "double" in m.group(0), "droit": "droit" in m.group(0)})],
              priorite=20)
