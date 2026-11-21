"""Modules de Jarvis : un fichier = une fonction (applis, minuteurs, médias, Discord…).

Pour ajouter une fonction, crée modules/mon_module.py avec :

    def installer(ctx, reg):
        reg.outil("dire_bonjour", "Dit bonjour à quelqu'un.", {"nom": "prénom"}, ["nom"], dire_bonjour)
        reg.regle(r"^salut (?P<nom>\\w+)$", lambda m, texte: [("dire_bonjour", {"nom": m["nom"]})])
        reg.contexte(lambda: "Info utile pour l'IA")      # facultatif
        reg.tache(ma_boucle_de_fond)                       # facultatif, lancée dans un fil à part

    def dire_bonjour(nom):
        return Resultat(f"Bonjour {nom} !")

- Un OUTIL est proposé à l'IA (appel d'outils d'Ollama) ; sa fonction renvoie un Resultat.
- Une RÈGLE est un raccourci sans IA (plus rapide) : une expression régulière sur ce que tu dis,
  qui renvoie la liste des outils à appeler, ou None pour laisser l'IA décider.
"""
import importlib
import logging
import pkgutil
import re
import unicodedata
from dataclasses import dataclass, field
from typing import Callable

log = logging.getLogger("modules")


@dataclass
class Resultat:
    dire: str                                  # phrase dite à voix haute
    ok: bool = True
    confirmer: Callable | None = None          # si présent : « dire » est une question oui/non
    annuler: str = "D'accord, j'annule."
    # Données pour l'IA (résultats web, texte d'une page…) : le cerveau les lui renvoie, et c'est elle qui
    # rédige la réponse parlée (2e tour). « dire » ne sert alors que si l'IA ne répond pas.
    pour_ia: str = ""
    images: list = field(default_factory=list)  # images en base64 (JPEG ou PNG) montrées à l'IA au 2e tour


@dataclass
class Outil:
    nom: str
    description: str
    parametres: dict                           # {"nom": "description"} ou {"nom": {schéma JSON}}
    requis: list
    fonction: Callable

    def schema(self) -> dict:
        proprietes = {}
        for nom, p in self.parametres.items():
            proprietes[nom] = p if isinstance(p, dict) else {"type": "string", "description": p}
        return {"type": "function", "function": {
            "name": self.nom, "description": self.description,
            "parameters": {"type": "object", "properties": proprietes, "required": self.requis}}}


@dataclass
class Regle:
    motif: re.Pattern
    fonction: Callable                         # (match, texte) -> list[(outil, args)] | None
    priorite: int = 50                         # plus petit = essayé en premier


@dataclass
class Registre:
    ctx: object
    outils: dict = field(default_factory=dict)
    regles: list = field(default_factory=list)
    contextes: list = field(default_factory=list)
    taches: list = field(default_factory=list)
    caches: set = field(default_factory=set)          # outils utilisables par les raccourcis seulement
    apis: dict = field(default_factory=dict)          # nom -> fonction(methode, donnees) pour la grande application
    apres_echanges: list = field(default_factory=list)  # fonction(question, reponse, source) après chaque échange
    intercepteurs: list = field(default_factory=list)   # fonction(texte, normalise) -> phrase | None, avant tout
    fins: list = field(default_factory=list)            # fonction() -> phrase | None, quand on dit « stop »

    def intercepteur(self, fonction):
        """Prend la main sur ce que dit l'utilisateur (ex. : ses réponses pendant une révision)."""
        self.intercepteurs.append(fonction)

    def fin_conversation(self, fonction):
        self.fins.append(fonction)

    def api(self, nom: str, fonction):
        """GET/POST /api/<nom> de la grande application : fonction(methode, donnees) -> dict."""
        self.apis[nom] = fonction

    def apres_echange(self, fonction):
        self.apres_echanges.append(fonction)

    def outil(self, nom, description, parametres, requis, fonction):
        self.outils[nom] = Outil(nom, description, parametres, requis, fonction)

    def regle(self, motif: str, fonction, priorite: int = 50):
        """Le motif s'applique au texte normalisé : minuscules, sans accents ni ponctuation."""
        self.regles.append(Regle(re.compile(motif, re.IGNORECASE), fonction, priorite))
        self.regles.sort(key=lambda r: r.priorite)

    def contexte(self, fonction):
        self.contextes.append(fonction)

    def tache(self, fonction):
        self.taches.append(fonction)

    def cacher_a_ia(self, nom: str):
        self.caches.add(nom)

    def schemas(self):
        return [o.schema() for o in self.outils.values() if o.nom not in self.caches]

    def executer(self, nom: str, args: dict) -> Resultat:
        outil = self.outils.get(nom)
        if not outil:
            return Resultat("Je ne sais pas encore faire ça.", ok=False)
        args = {k: v for k, v in (args or {}).items() if k in outil.parametres}
        if getattr(self.ctx, "simulation", False):
            self.ctx.appels.append((nom, args))
            return Resultat(f"(simulé : {nom} {args})")
        try:
            return outil.fonction(**args)
        except TypeError as e:
            log.warning("outil %s : mauvais paramètres %s (%s)", nom, args, e)
            return Resultat("Il me manque une précision pour faire ça.", ok=False)
        except Exception:  # noqa: BLE001
            log.exception("outil %s en erreur", nom)
            return Resultat("Désolé, ça n'a pas marché.", ok=False)


def charger(ctx) -> Registre:
    reg = Registre(ctx)
    for info in sorted(pkgutil.iter_modules(__path__), key=lambda m: m.name):
        if info.name.startswith("_"):
            continue
        try:
            module = importlib.import_module(f"{__name__}.{info.name}")
            if hasattr(module, "installer"):
                module.installer(ctx, reg)
        except Exception:  # noqa: BLE001
            log.exception("module %s non chargé", info.name)
    log.info("Modules : %d outils, %d raccourcis", len(reg.outils), len(reg.regles))
    return reg


# ── petits utilitaires partagés par les modules ─────────────────────────────

def sans_accents(texte: str) -> str:
    return "".join(c for c in unicodedata.normalize("NFD", texte) if unicodedata.category(c) != "Mn")


def normaliser(texte: str) -> str:
    """« Lance-moi Discord ! » -> « lance moi discord »"""
    texte = sans_accents(texte.lower()).replace("’", "'")
    texte = re.sub(r"[^a-z0-9' ]+", " ", texte)
    return re.sub(r"\s+", " ", texte).strip()


# Mots d'arrêt : ils font taire Jarvis pendant qu'il parle, et finissent la conversation (texte normalisé).
_ARRET = (r"(?:stop(?:pe|pez|per)?|arrete[sz]?(?: (?:toi|vous|ca|tout|de parler))?|tais toi|taisez vous|tait toi|"
          r"ch+u+t+|ca suffit|c'est bon|c'est tout|c'est fini|ca ira|silence|ferme la|laisse tomber|"
          r"on arrete(?: la)?|au revoir|bonne nuit|a plus(?: tard)?|fin de la conversation)")
# … avec, autour, seulement ces petits mots-là (« ok stop », « merci c'est tout », « Jarvis, tais-toi »)
_AUTOUR = (r"(?:hey|he|eh|oh|ah|ok|okay|bon|ben|bah|allez|non|jarvis|merci|beaucoup|maintenant|s'il te plait|"
           r"s'il vous plait|stp|svp|tout de suite)")
ARRET = re.compile(rf"^(?:{_AUTOUR} )*{_ARRET}(?: (?:{_ARRET}|{_AUTOUR}))*$")


def est_stop(texte: str) -> bool:
    """« Stop », « tais-toi », « merci c'est tout »… : un mot d'arrêt tout seul. « Arrête la musique » ou
    « c'est bon pour demain ? » sont de vraies demandes : ce n'est pas un stop."""
    n = re.sub(r"\bc est\b", "c'est", normaliser(texte or ""))
    return bool(n and ARRET.match(n))


# Codes de vérification : 4 à 8 chiffres d'affilée (« 482913 », « G-1234 ») ou « 123 456 ».
# Les numéros de téléphone (10 chiffres, ou groupés par 2) et les heures ne sont pas touchés.
CODES = re.compile(r"(?<!\d)(?:\d{4,8}|\d{3}[ -]\d{3})(?!\d)")


def masquer_codes(texte: str, remplacement: str = "(code masqué)") -> str:
    """Cache les codes reçus par SMS ou par mail avant de les dire à voix haute ou de les donner à l'IA."""
    return CODES.sub(remplacement, str(texte or ""))


NOMBRES = {
    "zero": 0, "un": 1, "une": 1, "deux": 2, "trois": 3, "quatre": 4, "cinq": 5, "six": 6, "sept": 7,
    "huit": 8, "neuf": 9, "dix": 10, "onze": 11, "douze": 12, "treize": 13, "quatorze": 14,
    "quinze": 15, "seize": 16, "vingt": 20, "trente": 30, "quarante": 40, "cinquante": 50,
    "soixante": 60, "cent": 100,
}


def nombre(mot: str):
    """« 5 », « 2,5 », « cinq », « vingt et un », « quatre-vingt-dix » -> nombre (ou None)."""
    brut = mot.strip().replace(",", ".")
    try:
        return float(brut)
    except ValueError:
        pass
    courant, ok = 0, False
    for m in sans_accents(mot.lower()).replace("-", " ").split():
        if m == "et":
            continue
        m = {"vingts": "vingt", "cents": "cent"}.get(m, m)
        if m not in NOMBRES:
            return None
        v, ok = NOMBRES[m], True
        if v == 100:
            courant = max(courant, 1) * 100
        elif v == 20 and courant == 4:
            courant = 80                      # quatre-vingt
        else:
            courant += v
    return courant if ok else None


UNITES = {"heure": 3600, "heures": 3600, "h": 3600, "minute": 60, "minutes": 60, "min": 60, "mn": 60,
          "seconde": 1, "secondes": 1, "sec": 1}


def duree_en_secondes(texte: str):
    """« 5 minutes », « une heure et demie », « 1h30 », « un quart d'heure » -> secondes (ou None)."""
    t = sans_accents(texte.lower()).replace("-", " ").replace("’", "'")
    t = re.sub(r"(\d+)\s*h\s*(\d+)", r"\1 heures \2 minutes", t)
    total = 0.0
    if re.search(r"\bquarts? d'?\s?heure\b", t):
        total += 900 * (3 if re.search(r"trois quarts? d'?\s?heure", t) else 1)
    if re.search(r"\bdemi(e)?\s?heure\b", t):
        total += 1800
    jetons = re.findall(r"\d+(?:\.\d+)?|[a-z]+", t.replace(",", "."))
    for i, jeton in enumerate(jetons):
        if jeton not in UNITES or (i and jetons[i - 1] in ("demi", "demie", "quart", "d")):
            continue
        for debut in range(max(0, i - 4), i):          # le nombre le plus long d'abord
            n = nombre(" ".join(jetons[debut:i]))
            if n is not None:
                total += n * UNITES[jeton]
                if " ".join(jetons[i + 1:i + 3]) in ("et demi", "et demie"):
                    total += UNITES[jeton] / 2
                elif " ".join(jetons[i + 1:i + 3]) == "et quart" and UNITES[jeton] == 3600:
                    total += 900
                break
    return int(total) if total > 0 else None
