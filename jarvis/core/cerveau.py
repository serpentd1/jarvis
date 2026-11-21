"""Le cerveau de Jarvis : un seul pour la voix ET le chat de l'île, avec une mémoire commune.

DEPUIS LE PASSAGE EN RUST : le vrai cerveau est dans la barre (barre/src/cerveau.rs, POST :47802/cerveau).
Ce fichier lui sert de « registre » (outils, raccourcis et données des modules Python, routes /cerveau/…
de serveur.py) et garde l'ancien cerveau Python en SECOURS, si la barre ne répond pas.

1. Raccourcis instantanés (« lance Discord », « pause », « minuteur 5 minutes »…) : pas besoin de l'IA.
2. Sinon, l'IA locale (Ollama) choisit les outils à appeler ; Jarvis les exécute VRAIMENT,
   puis dit ce qui a été fait (jamais « je le fais » sans le faire).
3. Les actions sensibles (envoyer un message, forcer la fermeture…) attendent ton « oui ».
"""
import json
import logging
import re
import threading
import time
from collections import deque
from dataclasses import dataclass
from datetime import datetime

import requests

from modules import ARRET, normaliser
from . import config, ollama

log = logging.getLogger("cerveau")

JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
MOIS = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
        "novembre", "décembre"]
OUI = re.compile(r"^(oui|ouais|ouai|wesh|vas y|ok|okay|d'accord|daccord|envoie|envoie le|envoie la|confirme|je confirme|"
                 r"c'est bon|go|let's go|yes|yep|carrement|bien sur|exactement|exact|parfait|ca marche|fais le|"
                 r"fais la|tres bien|bien|super|nickel|top|cool|genial|valide|c'est ca|c'est parti|absolument|"
                 r"evidemment|grave|bah oui|ben oui|(?:tu as|t'as|je te donne) (?:mon accord|mon autorisation|"
                 r"l'autorisation)|je t'autorise|j'autorise|autorise|j'accepte|accepte|affirmatif)\b"
                 r"|^(?:execute|lance|envoie|fais)(?: la| le| les| ca)?(?: stp| s'il te plait)?$")
NON = re.compile(r"^(non|nan|nope|annule|laisse tomber|oublie|stop|pas maintenant|attends|surtout pas|n'envoie pas|"
                 r"non merci|pas du tout|arrete|change)\b")
# L'IA dit avoir agi… sans avoir appelé d'outil : on ne la laisse pas mentir.
# « Stop » pendant une conversation continue : on arrête de discuter (texte normalisé). La liste des mots
# d'arrêt est dans modules/__init__.py (ARRET, est_stop).
STOP = ARRET
PRETEND_AGIR = re.compile(r"\b(j'ai (bien )?(envoye|lance|ouvert|ferme|mis|regle|active|desactive|coupe|change|cree|"
                          r"programme|supprime|annule|demarre|transmis)|c'est (bien )?(envoye|fait|lance|ouvert|regle|"
                          r"parti|programme)|message (bien )?envoye|je (l')?envoie|j'envoie|j'ouvre|je lance|je ferme|"
                          r"je mets|je regle|je coupe|minuteur (lance|regle|cree))\b")
OUBLI = re.compile(r"^(oublie tout|nouvelle conversation|on change de sujet|efface (?:la|ta) memoire|reset)$")
# « ouvre Chrome ET mets un minuteur » : plusieurs ordres -> c'est l'IA qui découpe, pas un raccourci
PLUSIEURS_ORDRES = re.compile(r"\b(?:et|puis|ensuite|apres)\s+(?:(?:me|moi|le|la|les|un|une|aussi)\s+)?"
                              r"(?:mets?|lance|ouvre|ferme|baisse|monte|coupe|cherche|reveille|dis|reponds?|"
                              r"annule|arrete|joue|regle|active|demarre|envoie|lis|passe)\b")
# Sans verbe d'action, c'est une question : l'IA répond avec les données, sans outils
# (sinon elle « ouvre la boîte mail » au lieu de dire s'il y a des mails importants).
ACTION = re.compile(r"\b(execut|fais (?:le|la|les|ca)|fait (?:le|la|les|ca)|vas y|va sur|va dans|va voir|aller sur|"
                    r"rends toi|utilise|autoris|essaie|essaye|reessai|refais|recommence|redui|agrandi|minimis|maximis|fenetre|restaur|plein ecran|premier plan|"
                    r"lance|lancer|ouvr|ferm|quitt|mets|met|mettre|baiss|monte|monter|augment|diminu|coupe|"
                    r"couper|remet|reactiv|envoi|repond|ecri|cherch|recherch|trouve|avance|recule|pause|joue|"
                    r"jouer|passe a|suivant|precedent|reveill|alarme|minuteur|chrono|timer|annul|arret|stop|"
                    r"demarr|active|desactiv|allum|etein|verrouill|actualis|rafraich|lis|lire|volume|ecout|"
                    r"affich|montre|regle|dis a|dis lui|plus fort|moins fort|clique|clic|tape|appuie|presse|saisis|"
                    r"defile|descend|remonte|scroll|regarde|ecran|analyse|decris|voix|installe|ajoute|rajoute|note (?:que|ca|le|la|les|dans|bien|moi)|prends note|"
                    r"retiens|souviens|rappelle|coche|termine|j'ai fait|j'ai fini|cree|oublie|routine|habitude|"
                    r"verrouille|batterie|supprim|efface|renomm|deplac|copie|modifi|corrige|fichier|dossier|"
                    r"page web|site web|powershell|commande|redemarr|veille|tue|processus)", re.IGNORECASE)
MOT_ACTIVATION = re.compile(r"^\s*(?:(?:hey|hé|he|eh|et|ok|okay|dis|bonjour|salut|yo)[\s,]*)?"
                            r"(?:jarvis|jarvi|jarvy|djarvis|charvis|jervis|jarvisse)\b[\s,.!?:;-]*", re.IGNORECASE)
MOT_ACTIVATION_DANS_PHRASE = re.compile(
    r"\b(?:h[eé]+y?|ok|eh)?[\s,]*d?j?arv[iy]s+\b[\s,.!?]*", re.IGNORECASE)


@dataclass
class Reponse:
    texte: str
    suivi: bool = False          # True : Jarvis a posé une question, il réécoute tout de suite
    reste: str | None = None     # ce qu'il reste à dire après les phrases déjà données en direct (None = tout)


def enlever_mot_activation(texte: str) -> str:
    return MOT_ACTIVATION.sub("", texte or "", count=1).strip()


def commande_autour_du_mot(texte: str) -> str:
    """Garde la commande prononcée autour du mot d'activation."""
    texte = texte or ""
    occurrences = list(MOT_ACTIVATION_DANS_PHRASE.finditer(texte))
    if not occurrences:
        return texte
    derniere = occurrences[-1]
    apres = texte[derniere.end():].strip(" \t\r\n,.;!?")
    if apres:
        return apres
    avant = texte[:derniere.start()].strip(" \t\r\n,.;!?")
    return avant or texte


def consigne(nom: str) -> str:
    m = datetime.now()
    return (
        f"Tu es {nom}, l'assistant vocal d'un collégien français, sur son PC Windows. Tu es efficace, poli, "
        "avec une pointe d'humour à la Jarvis d'Iron Man.\n"
        f"Nous sommes le {JOURS[m.weekday()]} {m.day} {MOIS[m.month - 1]} {m.year}, il est {m:%H:%M}.\n"
        "RÈGLES :\n"
        "- Réponds en français, en TUTOYANT l'utilisateur, en une ou deux phrases courtes et naturelles : elles "
        "sont LUES À VOIX HAUTE. Pas de markdown, pas de listes, pas d'émojis.\n"
        "- Tu ne sais pas encore envoyer de message sur Pronote ni de mail : si on te le demande, dis-le.\n"
        "- QUESTION sur la vie de l'utilisateur (météo, devoirs, notes, cours, mails, minuteurs, Discord, "
        "téléphone…) : réponds directement avec les DONNÉES ci-dessous, SANS appeler d'outil. Si l'information "
        "n'y est pas, dis-le au lieu d'inventer. « Et demain ? » continue la question précédente.\n"
        "- DISCUSSION ou CULTURE GÉNÉRALE (blague, histoire, devinette, conseil, explication, question de cours…) : "
        "réponds toi-même avec tes connaissances, volontiers et sans outil.\n"
        "- ACTION demandée clairement (ouvrir ou fermer une appli, un site, minuteur, alarme, chrono, vidéo, "
        "musique, volume, envoyer un message Discord, ouvrir Pronote ou un mail…) : appelle l'outil adapté. "
        "N'affirme JAMAIS avoir fait une action sans avoir appelé l'outil.\n"
        "- Si aucun outil ne permet de faire l'action, dis en une phrase « je ne sais pas encore faire ça, je l'ai "
        "noté pour l'apprendre », puis propose UNE chose proche que l'un de tes outils permet vraiment (ouvrir le "
        "site ou l'appli concernée, chercher sur Internet, mettre un rappel…). Ne propose JAMAIS une action qu'aucun "
        "de tes outils ne permet. « Qu'est-ce que tu sais faire ? » : appelle ce_que_je_sais_faire.\n"
    )


def note_coupure(dit: str, pas_dit: str = "") -> str:
    """Consigne pour l'IA quand l'utilisateur lui a parlé par-dessus (même texte que cerveau.rs)."""
    dit = (dit or "").strip().strip("…").strip()
    if not dit:
        return ""
    note = ("\nTu viens d'être coupé par l'utilisateur pendant que tu disais : « " + dit[-300:] + " ». "
            "C'est normal dans une discussion : ne le prends pas mal et ne t'excuse pas.")
    if pas_dit.strip():
        note += (" Tu n'as pas eu le temps de dire : « " + pas_dit.strip()[:400] + " ». Réponds D'ABORD à ce qu'il "
                 "vient de dire. Ensuite, seulement si c'est encore utile (même sujet, et il ne t'a pas demandé "
                 "d'arrêter ou de changer de sujet), ajoute UNE phrase très courte qui résume ce que tu n'avais pas "
                 "eu le temps de dire, par exemple « Et pour finir ce que je disais : … ». Sinon, n'en parle pas.")
    else:
        note += " Réponds simplement à ce qu'il vient de dire."
    return note


class Cerveau:
    def __init__(self, cfg: dict, nom: str, registre):
        self.cfg, self.nom, self.reg = cfg, nom, registre
        self.historique = deque(maxlen=2 * int(cfg["historique"]))
        self.attente = None                       # (action si oui, phrase si non)
        self.verrou = threading.RLock()           # une routine relance le cerveau depuis un de ses outils
        self.coupe = None                         # ce que Jarvis disait quand on lui a coupé la parole
        self.pas_dit = ""                         # … et ce qu'il n'a pas eu le temps de dire (mini-résumé ensuite)
        self._coupe, self._pas_dit = None, ""
        self._annule = threading.Event()          # on lui a coupé la parole : la réponse en cours est abandonnée
        self._flux = None
        self._confirmations, self._n = {}, 0      # questions oui/non posées par un outil (pour le cerveau Rust)
        self._barre_absente = 0.0                 # la barre ne répond pas : on n'insiste pas avant cette heure

    # ── public ───────────────────────────────────────────────
    def repondre(self, texte: str, source: str = "voix", sur_phrase=None) -> Reponse:
        """`sur_phrase(phrase)` reçoit les phrases dès qu'elles sont prêtes (Jarvis parle plus tôt) ;
        ce qu'il reste à dire ensuite est dans `Reponse.reste`."""
        rep = self._par_la_barre(texte, source, sur_phrase)
        if rep is not None:
            return rep
        with self.verrou:                          # secours : l'ancien cerveau Python
            try:
                return self._repondre(texte, source)
            except Exception:  # noqa: BLE001
                log.exception("erreur du cerveau")
                return Reponse("Oups, j'ai eu un bug. Tu peux répéter ?")

    def couper(self):
        """Arrête tout de suite la réponse en cours (la barre arrête alors l'IA)."""
        self._annule.set()
        r = self._flux
        if r is not None:
            try:
                r.close()
            except Exception:  # noqa: BLE001
                pass

    def oublier(self):
        with self.verrou:
            self.historique.clear()
            self.attente = None
        url = self._url_barre()
        if url:
            threading.Thread(target=_sans_erreur, args=(requests.post, f"{url}/cerveau/oublier"),
                             kwargs={"json": {}, "timeout": 2}, daemon=True).start()

    # ── le cerveau Rust (dans la barre) ──────────────────────
    def _url_barre(self):
        ctx = self.reg.ctx
        if getattr(ctx, "simulation", False) or not hasattr(ctx, "cfg"):
            return None                             # tests : tout reste en Python, rien n'est exécuté
        return ctx.cfg["givre"]["url"].rstrip("/")

    def _par_la_barre(self, texte: str, source: str, sur_phrase):
        """Pose la question au cerveau Rust. None s'il ne répond pas (on prend alors le secours)."""
        url = self._url_barre()
        if not url or time.time() < self._barre_absente:
            return None
        coupe, self.coupe = self.coupe, None
        pas_dit, self.pas_dit = self.pas_dit, ""
        conv = getattr(self.reg.ctx, "cfg", {}).get("conversation", {})
        corps = {"texte": texte, "source": source, "coupe": coupe or "", "pas_dit": pas_dit if coupe else "",
                 "reglages": {**self.cfg, "nom": self.nom, "consigne": config.prompt_systeme(),
                              "modele_api": str(conv.get("modele", ""))}}
        dites = []
        self._annule.clear()
        try:
            with requests.post(f"{url}/cerveau", json=corps, stream=True, timeout=(0.5, 400)) as r:
                self._flux = r
                if r.status_code != 200:
                    log.warning("le cerveau Rust a répondu %s : je prends le secours Python", r.status_code)
                    self._barre_absente = time.time() + 60
                    self.coupe, self.pas_dit = coupe, pas_dit
                    return None
                for ligne in r.iter_lines():
                    if self._annule.is_set():
                        break
                    if not ligne:
                        continue
                    d = json.loads(ligne.decode("utf-8"))
                    if d.get("phrase"):
                        dites.append(d["phrase"])
                        if sur_phrase:
                            sur_phrase(d["phrase"])
                    if d.get("fin"):
                        return Reponse(d.get("reponse", ""), bool(d.get("suivi")),
                                       d.get("reste", "") if sur_phrase else None)
        except requests.ConnectionError:
            if self._annule.is_set():
                return Reponse(" ".join(dites), False, "")
            if not dites:
                self._barre_absente = time.time() + 10
                self.coupe, self.pas_dit = coupe, pas_dit
                return None
        except Exception:  # noqa: BLE001
            if self._annule.is_set():
                return Reponse(" ".join(dites), False, "")
            log.exception("cerveau Rust")
            if not dites:
                self.coupe, self.pas_dit = coupe, pas_dit
                return None
        finally:
            self._flux = None
        return Reponse(" ".join(dites), False, "" if sur_phrase else None)   # coupé en route : on garde ce qui est dit

    # ── registre pour le cerveau Rust (routes /cerveau/… de serveur.py) ──
    def avant(self, texte: str) -> dict:
        """Ce qui passe avant l'IA : une révision en cours, « oublie tout », un raccourci instantané."""
        n = normaliser(texte)
        for intercepteur in self.reg.intercepteurs:
            dire = intercepteur(texte, n)
            if dire:
                return {"dire": dire}
        if OUBLI.match(n):
            self.historique.clear()
            return {"oubli": True}
        if not PLUSIEURS_ORDRES.search(n):
            for regle in self.reg.regles:
                m = regle.motif.search(n)
                if not m:
                    continue
                appels = regle.fonction(m, texte)
                if appels:
                    return {"appels": [[nom, args] for nom, args in appels]}
        return {}

    def _resultat(self, r) -> dict:
        d = {"ok": bool(r.ok), "dire": r.dire, "annuler": r.annuler}
        if getattr(r, "pour_ia", ""):
            d["pour_ia"] = r.pour_ia
        if getattr(r, "images", None):
            d["images"] = list(r.images)
        if r.confirmer:
            self._n += 1
            self._confirmations[self._n] = r.confirmer
            for vieux in [k for k in self._confirmations if k <= self._n - 20]:
                del self._confirmations[vieux]
            d["confirmer"] = self._n
        return d

    def executer_outil(self, nom: str, args: dict) -> dict:
        return self._resultat(self.reg.executer(nom, args if isinstance(args, dict) else {}))

    def confirmer(self, id_: int) -> dict:
        action = self._confirmations.pop(int(id_), None)
        if not action:
            return {"ok": False, "dire": "Il n'y a plus rien à confirmer."}
        return self._resultat(action())

    def apres_echange(self, question: str, reponse: str):
        for f in self.reg.apres_echanges:
            threading.Thread(target=_sans_erreur, args=(f, question, reponse), daemon=True).start()

    def fin_conversation(self) -> str | None:
        """« Stop » : la discussion est finie. Renvoie ce qu'il faut dire (ex. le bilan d'une révision)."""
        dire = None
        for f in self.reg.fins:
            try:
                dire = f() or dire
            except Exception:  # noqa: BLE001
                log.exception("fin de conversation")
        return dire

    def prechauffer(self):
        """Charge le modèle en mémoire graphique en arrière-plan (sinon la 1re réponse prend ~20 s)."""
        def charger():
            if ollama.barre_repond():          # la barre choisit l'IA (LM Studio, clé…) : rien à lancer ici
                return
            if ollama.assurer_lance(self.cfg["ollama_url"]):
                self._appel_ollama([{"role": "user", "content": "Bonjour"}], outils=True, max_mots=1)
                log.info("Modèle %s prêt", self.cfg["modele"])
        threading.Thread(target=charger, daemon=True, name="prechauffage").start()

    # ── interne ──────────────────────────────────────────────
    def _repondre(self, texte: str, source: str) -> Reponse:
        self._coupe, self.coupe = self.coupe, None     # ne sert qu'à la réponse qui suit la coupure
        self._pas_dit, self.pas_dit = self.pas_dit, ""
        texte = enlever_mot_activation(texte)
        n = normaliser(texte)
        if not n:
            return Reponse("Oui ?", suivi=source == "voix")

        if self.attente:                           # réponse à une question oui/non
            action, si_non = self.attente
            self.attente = None
            if OUI.match(n):
                r = action()
                return self._fin(texte, r.dire)
            if NON.match(n):
                return self._fin(texte, si_non)

        for intercepteur in self.reg.intercepteurs:    # ex. : une révision de leçon en cours
            dire = intercepteur(texte, n)
            if dire:
                return Reponse(dire)

        if OUBLI.match(n):
            self.historique.clear()
            return Reponse("C'est oublié. On repart de zéro.")

        for regle in ([] if PLUSIEURS_ORDRES.search(n) else self.reg.regles):   # raccourcis instantanés
            m = regle.motif.search(n)
            if not m:
                continue
            appels = regle.fonction(m, texte)
            if appels:
                log.info("raccourci : %s", appels)
                return self._executer(texte, appels, "")
        return self._ia(texte)

    def _contexte(self, question: str = "") -> str:
        import inspect
        morceaux = []
        for f in self.reg.contextes:
            try:
                t = f(question) if inspect.signature(f).parameters else f()
                if t:
                    morceaux.append(t)
            except Exception:  # noqa: BLE001
                log.exception("contexte en erreur")
        return "\n\n".join(morceaux)

    def _ia(self, texte: str) -> Reponse:
        note = ""
        if self._coupe:
            note = note_coupure(self._coupe, self._pas_dit)
        messages = [{"role": "system", "content": consigne(self.nom) + note + "\n" + self._contexte(texte)},
                    *self.historique, {"role": "user", "content": texte}]
        message = self._appel_ollama(messages, outils=bool(ACTION.search(normaliser(texte))))
        if message is None:
            return Reponse("Aucune IA ne répond. Ouvre LM Studio, ou ajoute une clé gratuite : clic droit sur la barre, "
                           "Paramètres, IA.")
        appels = [(c["function"]["name"], c["function"].get("arguments") or {})
                  for c in message.get("tool_calls") or []]
        contenu = nettoyer(message.get("content", ""))
        inconnus = [a for a in appels if a[0] not in self.reg.outils or a[0] in self.reg.caches]
        if inconnus:
            log.warning("l'IA a inventé des outils : %s", inconnus)
            appels = [a for a in appels if a not in inconnus]
            if not appels and not contenu:      # on lui redemande une vraie réponse, sans outils
                message = self._appel_ollama(messages, outils=False) or {}
                contenu = nettoyer(message.get("content", ""))
        if not appels and PRETEND_AGIR.search(normaliser(contenu)):
            log.warning("l'IA prétend avoir agi sans outil : « %s » — je lui redemande", contenu)
            relance = [*messages, {"role": "assistant", "content": contenu},
                       {"role": "system", "content": "ATTENTION : tu n'as appelé AUCUN outil, donc rien n'a été fait. "
                        "Si l'utilisateur demande une action, appelle maintenant l'outil adapté. Sinon, dis-lui "
                        "honnêtement que tu n'as rien fait."}]
            message = self._appel_ollama(relance, outils=True) or {}
            appels = [(c["function"]["name"], c["function"].get("arguments") or {})
                      for c in message.get("tool_calls") or [] if c["function"]["name"] in self.reg.outils
                      and c["function"]["name"] not in self.reg.caches]
            contenu = nettoyer(message.get("content", ""))
            if not appels and PRETEND_AGIR.search(normaliser(contenu)):
                contenu = "En fait, je n'ai rien fait. Redis-moi précisément ce que tu veux."
        if appels:
            log.info("outils choisis par l'IA : %s", appels)
            return self._executer(texte, appels, contenu)
        return self._fin(texte, contenu or "Je n'ai pas de réponse à ça.")

    def _executer(self, texte: str, appels, contenu: str) -> Reponse:
        resultats = [self.reg.executer(nom, args) for nom, args in appels]
        question = next((r for r in resultats if r.confirmer), None)
        phrase = " ".join(r.dire for r in resultats if r.dire).strip()
        if question:
            self.attente = (question.confirmer, question.annuler)
            return self._fin(texte, phrase, suivi=True)
        return self._fin(texte, phrase or contenu or "C'est fait.")

    def _fin(self, texte: str, reponse: str, suivi: bool = False) -> Reponse:
        self.historique.append({"role": "user", "content": texte})
        self.historique.append({"role": "assistant", "content": reponse})
        for f in self.reg.apres_echanges:          # ex : la mémoire retient ce qui compte (en arrière-plan)
            threading.Thread(target=_sans_erreur, args=(f, texte, reponse), daemon=True).start()
        return Reponse(reponse, suivi)

    def _appel_ollama(self, messages, outils: bool, max_mots: int = 300):
        corps = {
            "model": self.cfg["modele"], "messages": messages, "stream": False, "think": False,
            "keep_alive": self.cfg["garder_en_memoire"],
            "options": {"temperature": float(self.cfg["temperature"]), "num_ctx": int(self.cfg["contexte"]),
                        "num_predict": max_mots},
        }
        if outils:
            corps["tools"] = self.reg.schemas()
        url = f"{self.cfg['ollama_url'].rstrip('/')}/api/chat"
        for essai in range(2):
            try:
                r = requests.post(url, json=corps, timeout=120)
                if r.status_code == 400 and "tools" in corps and "tool" in r.text.lower():
                    log.warning("le modèle %s ne sait pas utiliser d'outils", self.cfg["modele"])
                    corps.pop("tools")
                    continue
                if r.status_code == 400 and "think" in r.text.lower():
                    corps.pop("think")              # vieux modèle qui ne « réfléchit » pas
                    continue
                r.raise_for_status()
                return r.json().get("message", {})
            except requests.ConnectionError:
                if essai == 0 and ollama.assurer_lance(self.cfg["ollama_url"]):
                    continue
                return None
            except Exception:  # noqa: BLE001
                log.exception("Ollama")
                return {"content": "J'ai eu un souci pour réfléchir à ça, désolé."}
        return None


def _sans_erreur(fonction, *args, **kwargs):
    try:
        fonction(*args, **kwargs)
    except Exception:  # noqa: BLE001
        log.exception("après-échange en erreur")


def nettoyer(texte: str) -> str:
    texte = re.sub(r"<think>.*?</think>", "", texte or "", flags=re.S)
    return texte.strip()
