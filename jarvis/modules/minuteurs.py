"""Minuteurs, alarme et chronomètre — affichés et gérés par l'île Givre.
Si Givre n'est pas lancé, Jarvis garde ses propres minuteurs et les annonce à voix haute.
"""
import logging
import re
import threading
import time

from . import Resultat, duree_en_secondes, nombre, normaliser, sans_accents

log = logging.getLogger("minuteurs")


def dire_duree(secondes: int) -> str:
    h, reste = divmod(int(secondes), 3600)
    m, s = divmod(reste, 60)
    morceaux = []
    if h:
        morceaux.append(f"{h} heure{'s' if h > 1 else ''}")
    if m:
        morceaux.append(f"{m} minute{'s' if m > 1 else ''}")
    if s and not h:
        morceaux.append(f"{s} seconde{'s' if s > 1 else ''}")
    return " et ".join(morceaux) or "0 seconde"


def heure_depuis_texte(texte: str):
    """« 7h30 », « 7 heures et demie », « sept heures quinze », « midi » -> "07:30"."""
    t = sans_accents(texte.lower())
    if "midi" in t:
        return "12:00"
    if "minuit" in t:
        return "00:00"
    m = re.search(r"(\d{1,2})\s*(?:h|heures?|:)\s*(\d{1,2})?", t)
    if m:
        h, mn = int(m.group(1)), int(m.group(2) or 0)
    else:       # en lettres : « sept heures quinze »
        mots = re.findall(r"[a-z]+", t.replace("-", " "))
        if not any(x.startswith("heure") for x in mots):
            return None
        i = next(k for k, x in enumerate(mots) if x.startswith("heure"))
        h = next((nombre(" ".join(mots[d:i])) for d in range(max(0, i - 3), i)
                  if nombre(" ".join(mots[d:i])) is not None), None)
        if h is None:
            return None
        h, mn = int(h), 0
        for f in range(min(len(mots), i + 3), i + 1, -1):
            n = nombre(" ".join(mots[i + 1:f]))
            if n is not None:
                mn = int(n)
                break
    if re.search(r"heures? et demie|h et demie", t):
        mn = 30
    elif re.search(r"et quart", t):
        mn = 15
    elif re.search(r"moins le quart", t):
        h, mn = (h - 1) % 24, 45
    if re.search(r"\b(du soir|de l'apres midi|ce soir)\b", t) and h < 12:
        h += 12
    return f"{h:02d}:{mn:02d}" if h < 24 and mn < 60 else None


def installer(ctx, reg):
    givre = ctx.givre
    locaux = {}                  # minuteurs de secours si Givre n'est pas lancé : id -> (nom, fin, Timer)
    compteur = iter(range(1, 10**6))

    def minuteur(duree_secondes: int, nom: str = ""):
        secondes = int(float(duree_secondes))
        if secondes <= 0 or secondes > 24 * 3600:
            return Resultat("Je ne peux pas faire un minuteur de cette durée.", ok=False)
        nom = (nom or "").strip()
        if nom and duree_en_secondes(nom):          # l'IA appelle parfois le minuteur « 10 minutes »
            nom = ""
        dit = f"Minuteur de {dire_duree(secondes)}" + (f" pour {nom}" if nom else "") + " lancé."
        if givre.minuteur(secondes, nom):
            return Resultat(dit)
        id_ = next(compteur)

        def sonner():
            locaux.pop(id_, None)
            ctx.etat.annoncer(f"Ton minuteur{' ' + nom if nom else ''} de {dire_duree(secondes)} est terminé !")
        t = threading.Timer(secondes, sonner)
        t.daemon = True
        t.start()
        locaux[id_] = (nom, time.time() + secondes, t)
        return Resultat(dit)

    def annuler_minuteur(nom: str = "", tous: bool = False):
        etat = givre.etat(age_max=0) or {}
        liste = etat.get("minuteurs") or []
        if etat.get("ok"):
            if tous or len(liste) <= 1:
                givre.annuler_minuteur(tous=True)
                return Resultat("Minuteurs annulés." if len(liste) > 1 else "Minuteur annulé.")
            cible = next((m for m in liste if normaliser(nom) and normaliser(nom) in normaliser(str(m.get("nom", "")))),
                         liste[0])
            givre.annuler_minuteur(id_=cible.get("id"))
            return Resultat(f"Minuteur {cible.get('nom') or ''} annulé.".replace("  ", " "))
        for id_, (n, _fin, t) in list(locaux.items()):
            if tous or not nom or normaliser(nom) in normaliser(n):
                t.cancel()
                locaux.pop(id_, None)
        return Resultat("C'est annulé.")

    def alarme(heure: str, active: bool = True):
        hhmm = heure_depuis_texte(heure) or heure
        if not re.fullmatch(r"\d{2}:\d{2}", hhmm or ""):
            return Resultat("Je n'ai pas compris l'heure de l'alarme.", ok=False)
        if not givre.alarme(hhmm, active):
            return Resultat("Il faut que la barre Givre soit lancée pour l'alarme.", ok=False)
        h, m = hhmm.split(":")
        return Resultat(f"Alarme réglée à {int(h)} heures {m if m != '00' else ''}.".replace(" .", ".")
                        if active else "Alarme désactivée.")

    def chrono(commande: str):
        commande = {"start": "demarrer", "lance": "demarrer", "stop": "pause", "arrete": "pause",
                    "reset": "zero", "remise a zero": "zero"}.get(normaliser(commande), normaliser(commande))
        r = givre.chrono(commande)
        if not r:
            return Resultat("Il faut que la barre Givre soit lancée pour le chrono.", ok=False)
        if commande == "pause":
            return Resultat(f"Chrono arrêté à {dire_duree(r.get('secondes', 0))}.")
        return Resultat({"demarrer": "Chrono lancé.", "zero": "Chrono remis à zéro."}.get(commande, "C'est fait."))

    def contexte():
        if not locaux:
            return ""
        lignes = [f"- {n or 'sans nom'} : reste {dire_duree(fin - time.time())}" for n, fin, _ in locaux.values()]
        return "Minuteurs gérés par Jarvis :\n" + "\n".join(lignes)

    reg.outil("minuteur", "Lance un minuteur (compte à rebours).",
              {"duree_secondes": {"type": "integer", "description": "durée en secondes (5 minutes = 300)"},
               "nom": "à quoi sert le minuteur (facultatif, ex : pâtes)"}, ["duree_secondes"], minuteur)
    reg.outil("annuler_minuteur", "Annule un minuteur en cours (ou tous).",
              {"nom": "nom du minuteur (facultatif)", "tous": {"type": "boolean"}}, [], annuler_minuteur)
    reg.outil("alarme", "Règle l'alarme (réveil) à une heure précise, ou la désactive.",
              {"heure": "heure au format HH:MM", "active": {"type": "boolean", "description": "false pour désactiver"}},
              ["heure"], alarme)
    reg.outil("chrono", "Contrôle le chronomètre.",
              {"commande": {"type": "string", "enum": ["demarrer", "pause", "zero"]}}, ["commande"], chrono)
    reg.contexte(contexte)

    # ── raccourcis sans IA ──
    def regle_minuteur(_m, texte):
        secondes = duree_en_secondes(texte)
        if not secondes:
            return None
        nom = re.search(r"\bpour (?:les |le |la |l'|mes |mon |ma |des |du )?([^,.!?]+?)\s*[.!?]?$", texte, re.I)
        nom = nom.group(1).strip() if nom else ""
        if nom and duree_en_secondes(nom):      # « un minuteur pour 5 minutes » : ce n'est pas un nom
            nom = ""
        return [("minuteur", {"duree_secondes": secondes, "nom": nom})]

    def regle_alarme(_m, texte):
        h = heure_depuis_texte(texte)
        return [("alarme", {"heure": h})] if h else None

    reg.regle(r"\b(minuteur|timer|compte a rebours|minuterie)\b(?!.*\b(annule|arrete|stop|supprime|enleve)\b)",
              regle_minuteur, priorite=20)
    reg.regle(r"^(?:annule|arrete|stoppe|supprime|enleve)(?: moi)? (?:le |les |mon |mes |tous les )?(?:minuteurs?|timers?)",
              lambda m, t: [("annuler_minuteur", {"tous": "tous" in normaliser(t) or "les minuteurs" in normaliser(t)})],
              priorite=15)
    reg.regle(r"\b(alarme|reveil|reveille)\b.*(\d|\b(?:une|deux|trois|quatre|cinq|six|sept|huit|neuf|dix|onze|douze|"
              r"midi|minuit)\b)", regle_alarme, priorite=20)
    reg.regle(r"^(?:lance|demarre|declenche|mets|active)(?: moi)? (?:le |un )?chrono(?:metre)?$",
              lambda m, t: [("chrono", {"commande": "demarrer"})], priorite=15)
    reg.regle(r"^(?:arrete|stoppe|stop|pause|mets en pause)(?: le)? chrono(?:metre)?$",
              lambda m, t: [("chrono", {"commande": "pause"})], priorite=15)
    reg.regle(r"^(?:remets|remet|reinitialise|remise)(?: le)? chrono(?:metre)?(?: a zero)?$",
              lambda m, t: [("chrono", {"commande": "zero"})], priorite=15)
