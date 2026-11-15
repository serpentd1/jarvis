"""Le cerveau de Jarvis : un seul pour la voix ET le chat de l'île, avec une mémoire commune.

1. Raccourcis instantanés (« lance Discord », « pause », « minuteur 5 minutes »…) : pas besoin de l'IA.
2. Sinon, l'IA locale (Ollama) choisit les outils à appeler ; Jarvis les exécute VRAIMENT,
   puis dit ce qui a été fait (jamais « je le fais » sans le faire).
3. Les actions sensibles (envoyer un message, forcer la fermeture…) attendent ton « oui ».
"""
import logging
import re
import threading
from collections import deque
from dataclasses import dataclass
from datetime import datetime

import requests

from modules import normaliser
from . import ollama

log = logging.getLogger("cerveau")

JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
MOIS = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
        "novembre", "décembre"]
OUI = re.compile(r"^(oui|ouais|ouai|wesh|vas y|ok|okay|d'accord|daccord|envoie|envoie le|envoie la|confirme|je confirme|"
                 r"c'est bon|go|let's go|yes|yep|carrement|bien sur|exactement|exact|parfait|ca marche|fais le|"
                 r"fais la|tres bien|bien|super|nickel|top|cool|genial|valide|c'est ca|c'est parti|absolument|"
                 r"evidemment|grave|bah oui|ben oui)\b")
NON = re.compile(r"^(non|nan|nope|annule|laisse tomber|oublie|stop|pas maintenant|attends|surtout pas|n'envoie pas|"
                 r"non merci|pas du tout|arrete|change)\b")
# L'IA dit avoir agi… sans avoir appelé d'outil : on ne la laisse pas mentir.
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
ACTION = re.compile(r"\b(lance|lancer|ouvr|ferm|quitt|mets|met|mettre|baiss|monte|monter|augment|diminu|coupe|"
                    r"couper|remet|reactiv|envoi|repond|ecri|cherch|recherch|trouve|avance|recule|pause|joue|"
                    r"jouer|passe a|suivant|precedent|reveill|alarme|minuteur|chrono|timer|annul|arret|stop|"
                    r"demarr|active|desactiv|allum|etein|verrouill|actualis|rafraich|lis|lire|volume|ecout|"
                    r"affich|montre|regle|dis a|dis lui|plus fort|moins fort|clique|clic|tape|appuie|presse|saisis|"
                    r"defile|descend|remonte|scroll|regarde|ecran|analyse|decris|voix|installe)", re.IGNORECASE)
MOT_ACTIVATION = re.compile(r"^\s*(?:(?:hey|hé|he|eh|et|ok|okay|dis|bonjour|salut|yo)[\s,]*)?"
                            r"(?:jarvis|jarvi|jarvy|djarvis|charvis|jervis|jarvisse)\b[\s,.!?:;-]*", re.IGNORECASE)


@dataclass
class Reponse:
    texte: str
    suivi: bool = False          # True : Jarvis a posé une question, il réécoute tout de suite


def enlever_mot_activation(texte: str) -> str:
    return MOT_ACTIVATION.sub("", texte or "", count=1).strip()


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
        "- QUESTION (météo, devoirs, notes, cours, mails, minuteurs, Discord, culture générale…) : réponds "
        "directement avec les DONNÉES ci-dessous, SANS appeler d'outil. Si l'information n'y est pas, dis-le "
        "au lieu d'inventer. « Et demain ? » continue la question précédente.\n"
        "- ACTION demandée clairement (ouvrir ou fermer une appli, un site, minuteur, alarme, chrono, vidéo, "
        "musique, volume, envoyer un message Discord, ouvrir Pronote ou un mail…) : appelle l'outil adapté. "
        "N'affirme JAMAIS avoir fait une action sans avoir appelé l'outil.\n"
        "- Si aucun outil ne permet de faire l'action, dis franchement que tu ne sais pas encore le faire, et ne "
        "propose JAMAIS une autre action qu'aucun de tes outils ne permet.\n"
    )


class Cerveau:
    def __init__(self, cfg: dict, nom: str, registre):
        self.cfg, self.nom, self.reg = cfg, nom, registre
        self.historique = deque(maxlen=2 * int(cfg["historique"]))
        self.attente = None                       # (action si oui, phrase si non)
        self.verrou = threading.Lock()

    # ── public ───────────────────────────────────────────────
    def repondre(self, texte: str, source: str = "voix") -> Reponse:
        with self.verrou:
            try:
                return self._repondre(texte, source)
            except Exception:  # noqa: BLE001
                log.exception("erreur du cerveau")
                return Reponse("Oups, j'ai eu un bug. Tu peux répéter ?")

    def oublier(self):
        with self.verrou:
            self.historique.clear()
            self.attente = None

    def prechauffer(self):
        """Charge le modèle en mémoire graphique en arrière-plan (sinon la 1re réponse prend ~20 s)."""
        def charger():
            if ollama.assurer_lance(self.cfg["ollama_url"]):
                self._appel_ollama([{"role": "user", "content": "Bonjour"}], outils=True, max_mots=1)
                log.info("Modèle %s prêt", self.cfg["modele"])
        threading.Thread(target=charger, daemon=True, name="prechauffage").start()

    # ── interne ──────────────────────────────────────────────
    def _repondre(self, texte: str, source: str) -> Reponse:
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

    def _contexte(self) -> str:
        morceaux = []
        for f in self.reg.contextes:
            try:
                t = f()
                if t:
                    morceaux.append(t)
            except Exception:  # noqa: BLE001
                log.exception("contexte en erreur")
        return "\n\n".join(morceaux)

    def _ia(self, texte: str) -> Reponse:
        messages = [{"role": "system", "content": consigne(self.nom) + "\n" + self._contexte()},
                    *self.historique, {"role": "user", "content": texte}]
        message = self._appel_ollama(messages, outils=bool(ACTION.search(normaliser(texte))))
        if message is None:
            return Reponse("Je n'arrive pas à joindre Ollama, mon cerveau. Vérifie qu'il est bien lancé.")
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


def nettoyer(texte: str) -> str:
    texte = re.sub(r"<think>.*?</think>", "", texte or "", flags=re.S)
    return texte.strip()
