"""Paramètres de la grande application : voix, modèle d'IA, écoute, annonces, démarrage… (écrits dans config.toml).
Et à la voix : « passe sur Whisper », « utilise Parakeet pour la transcription », « quelle IA ? »."""
import threading

import requests

from core import config, demarrage, ollama, stt
from core.tts import CATALOGUE
from . import Resultat, normaliser

AUCUNE_IA = ("Aucune IA ne répond. Ouvre LM Studio, ou ajoute une clé gratuite : clic droit sur la barre, "
             "Paramètres, IA.")

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

    def quelle_ia():
        """Quelle IA sert Jarvis, et lesquelles sont configurées (gratuites d'abord). Lit GET /ia/etat de la barre."""
        try:
            etat = requests.get(f"{ollama.url_barre()}/ia/etat", timeout=3).json()
        except Exception:  # noqa: BLE001
            return Resultat(AUCUNE_IA, ok=False)
        configures = [f for f in etat.get("fournisseurs") or [] if f.get("cle") or (f.get("local") and f.get("on"))]
        configures.sort(key=lambda f: not f.get("gratuit"))          # gratuites d'abord
        actif = etat.get("actif")
        if not actif and not configures:
            return Resultat(AUCUNE_IA, ok=False)
        phrase = f"Je me sers de {actif}." if actif else "Aucune IA n'est active en ce moment."
        if configures:
            liste = ", ".join(f"{f.get('nom')} ({'gratuit' if f.get('gratuit') else 'payant'})" for f in configures)
            phrase += f" Tu as configuré : {liste}."
        return Resultat(phrase)

    reg.outil("quelle_ia", "Dit quelle IA Jarvis utilise en ce moment (LM Studio, Ollama, Google Gemini, OpenAI…) "
              "et quelles IA sont configurées (clés d'API comprises).", {}, [], quelle_ia)
    reg.regle(r"\bquel(?:le)? (?:ia|modele|service)\b|\blm ?studio\b|\blms\b|\b(?:ollama|olama)\b|"
              r"\bcles? d'?api\b|\b(?:modeles?|ia)\b.*\b(?:charges?|installes?)\b",
              lambda m, t: [("quelle_ia", {})], priorite=20)

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
            etat = requests.get(f"{ollama.url_barre()}/ia/etat", timeout=3).json()
            modeles = list(dict.fromkeys(f["modele"] for f in etat.get("fournisseurs") or [] if f.get("modele")))
        except Exception:  # noqa: BLE001
            modeles = []
        modeles = modeles or [cfg["cerveau"]["modele"]]
        return {"ok": True, "voix": {"actuelle": ctx.voix_actuelle() if hasattr(ctx, "voix_actuelle") else None,
                                     "liste": [{"nom": n, "titre": v["titre"]} for n, v in CATALOGUE.items()]},
                "modeles": modeles, "demarrage_windows": demarrage.est_active(),
                "reglages": [{"section": s, "cle": c, "type": t, "libelle": l, "valeur": cfg[s][c],
                              **({"choix": CHOIX[(s, c)]} if (s, c) in CHOIX else {})} for s, c, t, l in REGLAGES]}

    reg.api("parametres", api)
