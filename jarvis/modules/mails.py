"""Mails : « résume-moi ce mail », « fais-moi un résumé du mail d'AliExpress », « qu'est-ce que je réponds ? ».
Le contenu complet vient de la barre Givre (GET /mail?n=…) ; l'IA locale le résume."""
import re
import time

from core import ollama
from . import Resultat, normaliser

VALIDITE_SELECTION = 300      # « ce mail » = celui ouvert dans l'île il y a moins de 5 min


def installer(ctx, reg):
    def selection():
        s = ctx.etat.selection
        if s and s.get("type") == "mail" and time.time() - s.get("t", 0) < VALIDITE_SELECTION:
            return s
        return None

    def trouver(n, de: str, texte: str):
        """Quel mail ? Le numéro donné, sinon l'expéditeur, sinon celui ouvert dans l'île, sinon le premier."""
        m = re.search(r"(?:n°|numero|numéro|mail)\s*(\d+)", texte or "", re.I)
        if (n is None or int(n) < 0) and m:
            n = int(m.group(1))
        mails = (ctx.givre.etat() or {}).get("mails")
        mails = mails if isinstance(mails, list) else []
        if (n is None or int(n) < 0) and de:
            n = next((x.get("n") for x in mails if normaliser(de) in normaliser(str(x.get("de", "")))), None)
        if n is None or int(n) < 0:
            s = selection()
            n = s.get("n") if s else (mails[0].get("n") if mails else None)
        return None if n is None else int(n), mails

    def resumer_mail(n: int = -1, de: str = "", demande: str = "Fais-moi un résumé"):
        n, mails = trouver(n, de, demande)
        if n is None:
            return Resultat("Je ne vois aucun mail : la barre Givre n'a pas de boîte mail connectée.", ok=False)
        complet = ctx.givre.mail(n)
        if complet:
            entete = f"De : {complet.get('de')}\nSujet : {complet.get('sujet')}\nDate : {complet.get('date')}"
            corps = str(complet.get("texte", ""))[:8000]
        else:                       # vieux Givre : on se contente de l'extrait
            x = next((x for x in mails if x.get("n") == n), None)
            if not x:
                return Resultat("Je ne trouve pas ce mail.", ok=False)
            entete = f"De : {x.get('de')}\nSujet : {x.get('sujet')}\nDate : {x.get('date')}"
            corps = f"(seulement le début du mail) {x.get('extrait', '')}"
        reponse = ollama.discuter(ctx.cfg["cerveau"], [
            {"role": "system", "content": "Tu es Jarvis, l'assistant d'un collégien. Tu réponds en français, en le "
             "tutoyant, en 2 à 4 phrases courtes faites pour être lues à voix haute (pas de liste, pas de markdown). "
             "Si le mail est une publicité ou une arnaque probable, dis-le. Si on te demande quoi répondre, propose "
             "une réponse courte qu'il pourrait envoyer lui-même (ou dis qu'il vaut mieux ne pas répondre)."},
            {"role": "user", "content": f"{entete}\n\n{corps}\n\n---\nÀ propos de ce mail : {demande}"},
        ])
        return Resultat(reponse) if reponse else Resultat("Je n'arrive pas à lire ce mail.", ok=False)

    def contexte():
        s = selection()
        if not s:
            return ""
        return (f"L'UTILISATEUR REGARDE en ce moment, dans la barre, le mail n°{s.get('n')} de {s.get('de')} "
                f"(« {s.get('sujet')} ») : « ce mail », « résume-le » parlent de celui-là (outil resumer_mail).")

    reg.contexte(contexte)
    reg.outil("resumer_mail", "Résume un mail, ou répond à une question sur un mail (que répondre, est-ce important…).",
              {"n": {"type": "integer", "description": "numéro du mail (n°) dans les données ; -1 = celui ouvert"},
               "de": "expéditeur, si l'utilisateur le cite",
               "demande": "ce que l'utilisateur veut savoir (par défaut : un résumé)"},
              [], resumer_mail)

    def regle(m, texte):
        de = re.search(r"\b(?:de|d'|du)\s*([A-Za-zÀ-ÿ0-9][\w&.\- ]{1,30})\s*[?.!]*$", texte or "")
        return [("resumer_mail", {"de": de.group(1).strip() if de and "mail" not in de.group(1).lower() else "",
                                  "demande": texte})]

    repondre = r"que repondre|quoi repondre|que dois je repondre|que je (?:lui )?reponds|comment (?:lui )?repondre|" \
               r"que lui repondre|reponse"
    reg.regle(r"\b(resume|resumer|resume moi|fais moi un resume|fais un resume|explique moi)\b.*\b(mail|mails|courriel)\b|"
              rf"\b(mail|mails)\b.*\b(resume|resumer|dit quoi|parle de quoi|important|{repondre})\b",
              regle, priorite=24)
    reg.regle(r"^(?:fais|fait) (?:moi )?un resume$|^resume (?:le|la|moi ca|ca)$|^(?:qu'est ce que|que) je (?:lui )?reponds$",
              lambda m, t: [("resumer_mail", {"demande": t})] if selection() else None, priorite=24)
