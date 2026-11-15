"""Discord (messages privés) via l'extension Chrome de Jarvis, dans ton onglet Discord habituel.

- Lecture : l'extension voit les notifications que Discord affiche, et les transmet à Jarvis.
- Réponse : Jarvis te relit le message, attend ton « oui », puis l'extension l'écrit et l'envoie
  dans la conversation, comme si tu le tapais.
- Pas de selfbot, pas de jeton : c'est ton onglet Chrome. Mais automatiser un compte reste contraire
  aux conditions de Discord : utilise-le avec modération.
"""
import logging
import re
import time
from collections import deque
from datetime import datetime

from . import Resultat, normaliser

log = logging.getLogger("discord")
INSTALLER_EXTENSION = ("Je ne vois pas encore Discord : il faut installer l'extension Chrome de Jarvis. "
                       "Dis-moi « installe l'extension » et je t'ouvre tout ce qu'il faut.")


def installer(ctx, reg):
    cfg = ctx.cfg["discord"]
    messages = deque(maxlen=int(cfg["memoire"]))
    deja_vus = deque(maxlen=200)

    def recu(evt: dict):
        """Appelé par l'extension Chrome à chaque nouveau message Discord."""
        auteur, texte = (evt.get("auteur") or "").strip(), (evt.get("texte") or "").strip()
        if not auteur or not texte:
            return
        cle = (auteur, texte)
        if cle in deja_vus:
            return
        deja_vus.append(cle)
        messages.append({"auteur": auteur, "texte": texte, "heure": datetime.now().strftime("%H:%M"),
                         "salon": evt.get("salon", ""), "t": time.time()})
        log.info("Discord : %s : %s", auteur, texte[:80])
        if ctx.givre.disponible():
            ctx.givre.notif(auteur, texte, "discord")     # l'île l'affiche, et le module annonces le lit
        elif ctx.cfg["annonces"]["actif"] and ctx.cfg["annonces"]["discord"]:
            ctx.etat.annoncer(f"Message Discord de {auteur} : {texte[:180]}")

    ctx.pont.abonner("discord_message", recu)

    def lire_messages_discord(auteur: str = "", nombre: int = 5):
        choix = [m for m in messages if not auteur or normaliser(auteur) in normaliser(m["auteur"])]
        if not choix:
            # Givre a peut-être vu des messages privés (notifications Windows) que l'extension n'a pas vus
            from .givre_infos import est_message_prive, nettoyer_nom
            e = ctx.givre.etat() or {}
            choix = [{"auteur": nettoyer_nom(d.get("de")), "texte": nettoyer_nom(d.get("texte")), "heure": d.get("heure", "")}
                     for d in reversed(e.get("discord") or []) if est_message_prive(d.get("de", ""))
                     and (not auteur or normaliser(auteur) in normaliser(nettoyer_nom(d.get("de"))))]
        if not choix:
            if not ctx.pont.connecte:
                return Resultat(INSTALLER_EXTENSION, ok=False)
            return Resultat(f"Pas de nouveau message privé Discord{' de ' + auteur if auteur else ''} "
                            "depuis que je suis lancé.")
        choix = list(choix)[-int(nombre or 5):]
        lu = " ".join(f"{m['auteur']} à {m['heure']} : {m['texte'][:200]}." for m in choix)
        return Resultat(lu)

    def repondre_discord(message: str, destinataire: str = ""):
        if not ctx.pont.connecte:
            return Resultat(INSTALLER_EXTENSION, ok=False)
        if not destinataire:
            if not messages:
                return Resultat("À qui veux-tu répondre ?", ok=False)
            destinataire = messages[-1]["auteur"]
        message = message.strip().strip("«»\"' ")
        if not message:
            return Resultat("Qu'est-ce que je dois écrire ?", ok=False)

        def envoyer():
            r = ctx.pont.commande({"commande": "discord_envoyer", "destinataire": destinataire, "texte": message},
                                  delai=20)
            if r and r.get("ok"):
                return Resultat(f"C'est envoyé à {destinataire}.")
            return Resultat(f"Je n'ai pas pu l'envoyer : {(r or {}).get('erreur', 'Chrome ne répond pas')}.", ok=False)

        if not cfg["confirmer_avant_envoi"]:
            return envoyer()
        return Resultat(f"J'envoie à {destinataire} : « {message} ». C'est bon ?", confirmer=envoyer,
                        annuler="D'accord, je ne l'envoie pas.")

    def contexte():
        if not ctx.pont.connecte:
            lignes = ["DISCORD : l'extension Chrome de Jarvis n'est PAS installée ou pas connectée, donc tu ne vois "
                      "PAS les messages Discord et tu ne peux pas en envoyer. Si on te parle de Discord, dis-le et "
                      "explique qu'il faut installer l'extension (dossier extension-chrome de Jarvis)."]
        else:
            lignes = ["DISCORD : extension Chrome connectée. Pour envoyer ou répondre à un message privé, appelle "
                      "l'outil repondre_discord."]
        if messages:
            lignes.append("Messages privés reçus : " + " ; ".join(
                f"{m['heure']} {m['auteur']} : « {m['texte'][:120]} »" for m in list(messages)[-6:]))
        elif ctx.pont.connecte:
            lignes.append("Aucun message privé reçu depuis le lancement de Jarvis.")
        return "\n".join(lignes)

    reg.contexte(contexte)
    reg.outil("lire_messages_discord", "Lit à voix haute les derniers messages privés Discord reçus.",
              {"auteur": "ne lire que les messages de cette personne (facultatif)",
               "nombre": {"type": "integer", "description": "combien de messages (5 par défaut)"}},
              [], lire_messages_discord)
    reg.outil("repondre_discord",
              "Envoie un message privé DISCORD dicté par l'utilisateur (Jarvis demande confirmation avant). "
              "UNIQUEMENT pour Discord : jamais pour Pronote, un professeur ou un mail.",
              {"message": "le texte exact à envoyer, tel que l'utilisateur l'a dicté",
               "destinataire": "à qui (pseudo) ; vide = la dernière personne qui a écrit"},
              ["message"], repondre_discord)
    reg.regle(r"^(?:lis|lit|lire|donne)(?: moi)? (?:mes |les )?(?:derniers |nouveaux )?messages? (?:sur |de |discord)",
              lambda m, t: [("lire_messages_discord", {})], priorite=30)
    # « On voit un message sur Discord ? », « j'ai des messages Discord ? », « Discord, j'ai des MP ? »
    pas_un_envoi = r"^(?!(?:envoie|envoyer|ecris|ecrire|reponds?|repondre|dis)\b)"
    reg.regle(pas_un_envoi + r"(?:.*\b(?:messages?|mp|dm)\b.*\bdiscord\b|.*\bdiscord\b.*\b(?:messages?|mp|dm)\b)",
              lambda m, t: [("lire_messages_discord", {})], priorite=35)

    # « Réponds à Tom que j'arrive », « dis à Léa : on joue ce soir », « réponds-lui que oui »,
    # « envoie un message à Claude qui dit salut », « envoie salut à Matt »
    separateur = r"(?:qui dit(?: que)?\s+|en (?:lui )?disant(?: que)?\s+|pour (?:lui )?dire(?: que)?\s+|que\s+|qu['’]\s*|:\s*|,\s*)"
    a_qui = re.compile(r"^\W*(?:r[ée]ponds?|dis|[ée]cris|envoie)(?:[- ]?(?:un|le|ce) message)?(?: discord)?\s+(?:à|a)\s+"
                       r"(?P<dest>[^\s,:]+(?:\s[^\s,:]+)?)\s*" + separateur + r"(?P<msg>.+)$", re.I)
    a_lui = re.compile(r"^\W*(?:r[ée]ponds?|dis|[ée]cris)[- ](?:lui|leur)\s*" + separateur + r"?(?P<msg>.+)$", re.I)
    message_a = re.compile(r"^\W*envoie\s+(?:le message\s+)?[«\"“]?\s*(?P<msg>.+?)\s*[»\"”]?\s+(?:à|a)\s+"
                           r"(?P<dest>[^\s,:]+(?:\s[^\s,:]+)?)\s*(?:sur discord)?\s*[.!?]?$", re.I)

    def regle_repondre(_m, texte):
        from core.cerveau import enlever_mot_activation
        t = enlever_mot_activation(texte)
        if re.search(r"\b(e-?mails?|mails?|sms|textos?|pronote)\b", t, re.I):
            return None             # pas Discord : l'IA expliquera ce qu'elle sait faire
        m = a_qui.match(t) or a_lui.match(t) or message_a.match(t)
        if not m:
            return None
        msg = m["msg"].strip().rstrip(".")
        msg = msg[:1].upper() + msg[1:]
        return [("repondre_discord", {"destinataire": m.groupdict().get("dest") or "", "message": msg})]

    reg.regle(r"^(?:reponds?|dis|ecris|envoie)(?: un message| le message| ce message)?(?: discord)? a \S+ |"
              r"^(?:reponds?|dis|ecris) (?:lui|leur) |^envoie .+ a \S+( \S+)?( sur discord)?$",
              regle_repondre, priorite=25)
