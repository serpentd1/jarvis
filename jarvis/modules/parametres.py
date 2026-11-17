"""Paramètres de la grande application : voix, modèle d'IA, écoute, annonces, démarrage… (écrits dans config.toml).
Et à la voix : « passe sur Whisper », « utilise Parakeet pour la transcription »."""
import threading

import requests

from core import config, demarrage, stt
from core.tts import CATALOGUE
from . import Resultat, normaliser

# (section, clé, type, libellé) : ce que la grande application peut modifier
REGLAGES = [
    ("cerveau", "modele", "modele", "Modèle d'IA"),
    ("transcription", "modele", "choix", "Transcription de ta voix"),
    ("micro", "seuil_activation", "nombre", "Sensibilité du « Hey Jarvis » (petit = réagit plus facilement)"),
    ("micro", "conversation", "nombre", "Écoute après sa réponse (secondes, 0 = non)"),
    ("micro", "silence_fin", "nombre", "Silence qui termine ta phrase (secondes)"),
    ("micro", "ding", "booleen", "Petit « ding » quand il écoute"),
    ("voix", "lenteur", "nombre", "Lenteur de la voix (1 = normal)"),
    ("annonces", "actif", "booleen", "Annonces à voix haute"),
    ("annonces", "pronote", "booleen", "Annoncer Pronote"),
    ("annonces", "mail", "booleen", "Annoncer les mails importants"),
    ("annonces", "discord", "booleen", "Annoncer Discord"),
    ("annonces", "telephone", "booleen", "Annoncer les SMS et appels du téléphone"),
    ("annonces", "telephone_applis", "booleen", "Annoncer aussi les applis du téléphone"),
    ("annonces", "ne_pas_deranger", "texte", "Ne pas déranger (ex : 22:00-07:00)"),
    ("general", "lancer_givre", "booleen", "Lancer l'île Jarvis avec l'assistant vocal"),
    ("mise_a_jour", "auto", "booleen", "Mises à jour automatiques"),
]


CHOIX = {("transcription", "modele"): stt.CHOIX}


def cible_transcription(demande: str, actuelle: str) -> str | None:
    """« whisper » -> large-v3-turbo, « parakeet » -> parakeet, « » / « l'autre » -> celle qu'on n'utilise pas."""
    d = normaliser(demande or "").replace(" ", "")
    if any(m in d for m in ("whisp", "wisp", "ouisp", "visper")):
        return "large-v3-turbo"
    if any(m in d for m in ("parak", "parac", "paraq", "perak")):
        return "parakeet"
    if d in ("", "lautre", "autre", "changer", "change"):
        return "large-v3-turbo" if stt.est_parakeet({"modele": actuelle}) else "parakeet"
    return None


def installer(ctx, reg):
    def changer_transcription(nom: str = ""):
        changer = getattr(ctx, "changer_transcription", None)
        if not changer:
            return Resultat("Je ne peux pas changer ça en ce moment.", ok=False)
        actuelle = ctx.transcription_actuelle()
        cible = cible_transcription(nom, actuelle)
        if not cible:
            return Resultat("Pour t'écouter, je connais Parakeet et Whisper.", ok=False)
        if stt.titre(cible) == stt.titre(actuelle):
            return Resultat(f"J'utilise déjà {stt.titre(cible)}.")
        threading.Thread(target=changer, args=(cible, True), daemon=True).start()
        return Resultat(f"Je passe sur {stt.titre(cible)}, ça prend quelques secondes.")

    reg.outil("changer_transcription", "Change le modèle qui transcrit la voix de l'utilisateur (Parakeet ou Whisper).",
              {"nom": {"type": "string", "description": "« parakeet » ou « whisper » ; vide = l'autre"}},
              [], changer_transcription)
    reg.regle(r"^(?:passe|repasse|mets?|remets?|utilise|prends?|change)(?: la| de)? (?:transcription|reconnaissance"
              r"(?: vocale| de la voix)?)(?: (?:sur|en|avec|pour|a))?(?P<nom>.*)$",
              lambda m, t: [("changer_transcription", {"nom": m["nom"].strip()})], priorite=15)
    reg.regle(r"^(?:passe|repasse|utilise|prends?|mets?|remets?)(?: sur| en| a)? (?P<nom>(?:w|ou|v)isp\w*|whisp\w*"
              r"|para ?[kcq]\w*|perak\w*)(?: pour (?:la transcription|m\W?ecouter|t\W?ecouter))?$",
              lambda m, t: [("changer_transcription", {"nom": m["nom"]})], priorite=15)

    def modeles_ollama():
        url = config.charger()["cerveau"]["ollama_url"].rstrip("/")
        utilise = ctx.cfg["cerveau"]["modele"]
        try:
            charges = requests.get(f"{url}/api/ps", timeout=3).json().get("models", [])
            installes = [m["name"] for m in requests.get(f"{url}/api/tags", timeout=3).json().get("models", [])]
        except Exception:  # noqa: BLE001
            return Resultat("Ollama ne répond pas : il n'est peut-être pas lancé.", ok=False)

        def go(octets):
            return f"{octets / 1e9:.1f} Go".replace(".", ",")

        if charges:
            details = []
            for m in charges:
                vram, total = m.get("size_vram", 0), m.get("size", 0) or 1
                ou = "dans la carte graphique" if vram >= total * 0.95 else (
                    "dans la mémoire du PC" if vram == 0 else f"{round(100 * vram / total)} % dans la carte graphique")
                details.append(f"{m['name']} ({go(total)}, {ou})")
            s = "s" if len(charges) > 1 else ""
            phrase = f"{len(charges)} modèle{s} chargé{s} en ce moment : " + ", ".join(details) + "."
        else:
            phrase = "Aucun modèle n'est chargé en ce moment."
        s = "s" if len(installes) > 1 else ""
        return Resultat(f"{phrase} Tu en as {len(installes)} installé{s} : {', '.join(installes)}. "
                        f"Moi, j'utilise {utilise}.")

    reg.outil("modeles_ollama", "Dit quels modèles d'IA sont chargés dans Ollama (et où : carte graphique ou mémoire), "
              "lesquels sont installés, et lequel Jarvis utilise.", {}, [], modeles_ollama)
    reg.regle(r"\b(?:modeles?|ia)\b.*\b(?:charges?|installes?|ollama|olama)\b|\b(?:ollama|olama)\b.*\bmodeles?\b",
              lambda m, t: [("modeles_ollama", {})], priorite=20)

    def api(methode, p):
        cfg = config.charger()
        if methode == "POST":
            if p.get("cle") == "voix" and p.get("valeur") in CATALOGUE and hasattr(ctx, "changer_voix"):
                ctx.changer_voix(p["valeur"])
            elif (p.get("section"), p.get("cle")) == ("transcription", "modele") and hasattr(ctx, "changer_transcription"):
                cible = cible_transcription(str(p.get("valeur", "")), ctx.transcription_actuelle())
                if cible:
                    threading.Thread(target=ctx.changer_transcription, args=(cible, True), daemon=True).start()
            elif p.get("cle") == "demarrage_windows":
                demarrage.activer(bool(p.get("valeur")))
            else:
                r = next((r for r in REGLAGES if r[0] == p.get("section") and r[1] == p.get("cle")), None)
                if not r:
                    return {"ok": False, "erreur": "réglage inconnu"}
                config.ecrire_valeur(r[0], r[1], p.get("valeur"))
                return {"ok": True, "redemarrer": True}
            return {"ok": True}
        try:
            modeles = [m["name"] for m in requests.get(f"{cfg['cerveau']['ollama_url']}/api/tags", timeout=3).json()["models"]]
        except Exception:  # noqa: BLE001
            modeles = [cfg["cerveau"]["modele"]]
        return {"ok": True, "voix": {"actuelle": ctx.voix_actuelle() if hasattr(ctx, "voix_actuelle") else None,
                                     "liste": [{"nom": n, "titre": v["titre"]} for n, v in CATALOGUE.items()]},
                "modeles": modeles, "demarrage_windows": demarrage.est_active(),
                "reglages": [{"section": s, "cle": c, "type": t, "libelle": l, "valeur": cfg[s][c],
                              **({"choix": CHOIX[(s, c)]} if (s, c) in CHOIX else {})} for s, c, t, l in REGLAGES]}

    reg.api("parametres", api)
