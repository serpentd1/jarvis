"""Pronote, mails, météo, minuteurs : Jarvis les lit dans la barre Givre (rien n'est refait ici)
et les donne à l'IA comme contexte. Il peut aussi ouvrir Pronote, un mail, les devoirs…
"""
import logging
import re
from datetime import date

from . import Resultat, masquer_codes, normaliser

log = logging.getLogger("givre_infos")
JOURS = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"]
MOIS = ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
        "novembre", "décembre"]


def _oui(v):
    return "oui" if v else "non"


def date_fr(iso: str) -> str:
    """« 2026-11-11 » -> « mercredi 11 novembre » (l'IA se trompe moins qu'avec le format ISO)."""
    try:
        d = date.fromisoformat(str(iso)[:10])
        return f"{JOURS[d.weekday()]} {d.day} {MOIS[d.month - 1]}"
    except ValueError:
        return str(iso)


def note_fr(note: str) -> str:
    """« Abs/20 » -> « absent (pas de note) », « 4,75/5 » reste tel quel."""
    n = str(note)
    if re.match(r"^\s*abs", n, re.I):
        return "absent (pas de note)"
    if re.match(r"^\s*(disp|nn|n\.?not)", n, re.I):
        return "dispensé ou non noté"
    return n


def nettoyer_nom(texte: str) -> str:
    """Enlève les caractères invisibles que Windows met autour des noms (⁨Tom⁩)."""
    return re.sub(r"[⁦-⁩‎‏]", "", str(texte or "")).strip()


def est_message_prive(de: str) -> bool:
    """Les messages de serveur ont un salon : « Pseudo (#général, Serveur) »."""
    return "(#" not in nettoyer_nom(de).replace("( #", "(#")


def element_telephone(t: dict) -> str:
    """Un SMS, appel ou notification du téléphone, pour l'IA (codes de vérification masqués)."""
    genre, de, heure = t.get("type", ""), nettoyer_nom(t.get("de")), t.get("heure", "")
    texte = masquer_codes(nettoyer_nom(t.get("texte", "")))[:120]
    if genre == "appel":
        return f"{heure} {texte.lower() or 'appel'} de {de}"
    if genre == "batterie":
        return f"{heure} batterie du téléphone faible ({texte})"
    if genre == "appli":
        return f"{heure} notification {t.get('appli') or 'd’une appli'} de {masquer_codes(de)} : « {texte} »"
    return f"{heure} SMS de {de} : « {texte} »"


def resume(e: dict) -> str:
    """État de Givre -> texte court et lisible pour l'IA."""
    L = [f"Aujourd'hui : {e.get('jour', '')}, il est {e.get('heure', '')}."]

    m = e.get("meteo") or {}
    a = m.get("actuelle")
    if a:
        L.append(f"MÉTÉO ({m.get('ville', '')}) : {a.get('temperature')}°C, ressenti {a.get('ressenti')}°C, "
                 f"{a.get('ciel')}, vent {a.get('vent_kmh')} km/h, humidité {a.get('humidite')} %, "
                 f"lever {a.get('lever', '?')}, coucher {a.get('coucher', '?')}.")
        heures = ", ".join(f"{h.get('heure')} {h.get('temperature')}° {h.get('ciel')}" for h in m.get("prochaines_heures", [])[:6])
        if heures:
            L.append(f"  Prochaines heures : {heures}.")
        jours = " ; ".join(f"{j.get('jour')} {j.get('min')}° à {j.get('max')}° {j.get('ciel')}" for j in m.get("prochains_jours", [])[:5])
        if jours:
            L.append(f"  Prochains jours : {jours}.")
    else:
        L.append("MÉTÉO : indisponible.")

    p = e.get("pronote") or {}
    if p.get("connecte"):
        L.append(f"PRONOTE (élève {p.get('eleve', '')}, {p.get('classe', '')}, mis à jour : {p.get('mis_a_jour') or '?'}) :")
        if p.get("erreur"):
            L.append(f"  ATTENTION : la connexion à Pronote ne marche plus ({p['erreur']}). Les données ci-dessous datent "
                     f"de la dernière mise à jour ({p.get('mis_a_jour') or '?'}) et peuvent être fausses aujourd'hui. "
                     "Pour la refaire : clic droit sur la barre, Paramètres, Comptes, Pronote (QR code).")
        if p.get("pai"):
            L.append("  PAI : l'utilisateur a un emploi du temps aménagé, rien qu'à lui. Les cours marqués « il n'y va pas » "
                     "ont lieu pour sa classe mais PAS pour lui : ne lui dis jamais qu'il a ces cours, ne lui rappelle pas d'y "
                     "aller, ne compte pas ses heures dessus. Pour un devoir d'un cours où il ne va pas, dis-le-lui (il devra "
                     "peut-être récupérer le cours). Dans un résumé de journée, ne cite que les cours où il va.")
        L.append("  RÈGLE : pour les cours et les devoirs, crois UNIQUEMENT ce qui suit (Pronote, à jour), jamais tes "
                 "souvenirs. Chaque cours a sa vraie date : il n'y a pas de semaine A ou B à deviner. Un jour qui n'est "
                 "pas dans la liste : dis que tu ne l'as pas, n'invente pas.")
        pc = p.get("prochain_cours")
        if pc:
            L.append(f"  Prochain cours : {pc.get('matiere')} à {pc.get('debut')} ({pc.get('dans', '')}).")
        for cle, titre in (("cours_aujourdhui", "Cours aujourd'hui"), ("cours_demain", "Cours demain")):
            cours = p.get(cle) or []
            if cours:
                L.append(f"  {titre} : " + " ; ".join(
                    f"{c.get('debut')}-{c.get('fin')} {c.get('matiere')}"
                    + (f" salle {c['salle']}" if c.get("salle") else "")
                    + (" (ANNULÉ)" if c.get("annule") else (f" ({c['statut']})" if c.get("statut") else ""))
                    + (f" (il n'y va pas : {c['n_y_va_pas']})" if c.get("n_y_va_pas") else "")
                    + (" (créneau à lui, pas un cours)" if c.get("perso") else "")
                    for c in cours) + ".")
            else:
                L.append(f"  {titre} : aucun cours.")
        for j in p.get("semaine") or []:
            va = [c for c in j.get("cours") or [] if not c.get("n_y_va_pas") and not c.get("annule")]
            sans = [c for c in j.get("cours") or [] if c.get("n_y_va_pas")]
            annules = [c for c in j.get("cours") or [] if c.get("annule") and not c.get("n_y_va_pas")]
            ligne = f"  Emploi du temps du {j.get('jour')} ({date_fr(j.get('date'))}) : "
            ligne += (", ".join(f"{c.get('debut')}-{c.get('fin')} {c.get('matiere')}"
                                + (" (créneau à lui)" if c.get("perso") else "") for c in va) or "aucun cours pour lui")
            if annules:
                ligne += ". Annulés : " + ", ".join(f"{c.get('debut')} {c.get('matiere')}" for c in annules)
            if sans:
                ligne += ". Cours de sa classe où il ne va PAS (PAI) : " + ", ".join(f"{c.get('debut')} {c.get('matiere')}" for c in sans)
            L.append(ligne + ".")
        devoirs = p.get("devoirs") or []
        if devoirs:
            L.append("  Devoirs à faire : " + " ; ".join(
                f"{d.get('matiere')} pour {d.get('pour_quand') or ''} ({date_fr(d.get('pour'))}) : "
                f"{str(d.get('texte', ''))[:160]}" + (" (déjà fait)" if d.get("fait") else "")
                + (" (il ne va pas à ce cours : PAI, il n'y était peut-être pas)" if d.get("cours_manque") else "")
                for d in devoirs[:12]) + ".")
        else:
            L.append("  Devoirs : rien à faire.")
        notes = p.get("notes") or []
        if p.get("notes_masquees"):
            L.append("  Notes : l'utilisateur a choisi de masquer ses notes. N'en parle pas et ne donne aucune note "
                     "ni moyenne, même si on te le demande : dis-lui qu'elles sont masquées dans les réglages de l'île.")
        elif notes:
            L.append("  Dernières notes (de la plus récente à la plus ancienne) : " + " ; ".join(
                f"{n.get('matiere')} : {note_fr(n.get('note'))}"
                + (f" (moyenne de la classe {n['moyenne_classe']})" if n.get("moyenne_classe") else "")
                + f", le {date_fr(n.get('date'))}" for n in notes) + ".")
        if not p.get("notes_masquees") and p.get("moyenne_generale"):
            L.append(f"  Moyenne générale : {p['moyenne_generale']}.")
        for msg in (p.get("messages_non_lus") or [])[:5]:
            L.append(f"  Message Pronote non lu de {msg.get('de')} : « {msg.get('sujet')} » ({msg.get('quand', '')}).")
        for act in (p.get("actualites_non_lues") or [])[:3]:
            L.append(f"  Actualité non lue : « {act.get('titre')} » de {act.get('de')} : {str(act.get('texte', ''))[:150]}")
    elif p.get("desactive"):
        L.append("PRONOTE : désactivé par l'utilisateur. N'en parle pas et ne propose pas de le connecter.")
    else:
        L.append("PRONOTE : pas connecté dans la barre Givre.")

    mails = e.get("mails")
    if isinstance(mails, dict) and mails.get("desactive"):
        L.append("MAILS : désactivés par l'utilisateur. N'en parle pas et ne propose pas de les connecter.")
    elif isinstance(mails, dict) and not mails.get("connecte", True):
        L.append("MAILS : aucun compte connecté dans la barre Givre.")
    elif mails:
        L.append(f"MAILS IMPORTANTS ({e.get('mails_non_lus', len(mails))} mails non lus en tout) :")
        for x in mails[:8]:
            L.append(f"  n°{x.get('n')} [{x.get('niveau')}] de {x.get('de')} le {x.get('date')} : « {x.get('sujet')} » — {str(x.get('extrait', ''))[:140]}")
    else:
        L.append(f"MAILS : aucun mail important ({e.get('mails_non_lus', 0)} non lus en tout).")

    minuteurs = e.get("minuteurs") or []
    if minuteurs:
        L.append("MINUTEURS EN COURS : " + " ; ".join(
            f"{t.get('nom') or 'sans nom'} : reste {int(t.get('restant_s', 0)) // 60} min {int(t.get('restant_s', 0)) % 60} s"
            + (" (en pause)" if t.get("pause") else "") for t in minuteurs) + ".")
    else:
        L.append("MINUTEURS : aucun.")
    al = e.get("alarme") or {}
    L.append(f"ALARME : {'réglée à ' + al.get('heure', '') if al.get('active') else 'désactivée'}"
             + (f", elle sonne en ce moment ({al['sonne']})" if al.get("sonne") else "") + ".")
    ch = e.get("chrono") or {}
    if ch.get("en_marche") or ch.get("secondes"):
        L.append(f"CHRONO : {int(ch.get('secondes', 0))} secondes, en marche : {_oui(ch.get('en_marche'))}.")

    discord = [d for d in (e.get("discord") or []) if est_message_prive(d.get("de", ""))]
    if discord:
        L.append("DISCORD, derniers messages privés reçus : " + " ; ".join(
            f"{d.get('heure')} {nettoyer_nom(d.get('de'))} : « {nettoyer_nom(d.get('texte', ''))[:120]} »"
            for d in discord[:6]) + ".")
    elif e.get("discord_non_lus_chrome"):
        L.append(f"DISCORD : {e['discord_non_lus_chrome']} message(s) non lu(s) dans Chrome.")

    tel = e.get("telephone")
    if isinstance(tel, list) and tel:
        L.append("TÉLÉPHONE (reçu par « Mobile connecté », du plus récent au plus ancien) : "
                 + " ; ".join(element_telephone(t) for t in tel[:8]) + ".")
    elif isinstance(tel, dict):
        L.append("TÉLÉPHONE : rien reçu (le téléphone n'est pas relié par « Mobile connecté », "
                 "ou ses notifications ne sont pas activées).")
    return "\n".join(L)


def installer(ctx, reg):
    givre = ctx.givre

    def contexte():
        e = givre.etat()
        if not e or not e.get("ok"):
            return "La barre Givre n'est pas lancée : pas d'accès à Pronote, aux mails ni aux minuteurs de l'île."
        return "DONNÉES DE LA BARRE GIVRE (à jour) :\n" + resume(e)

    def ouvrir_givre(quoi: str, n: int = 0):
        quoi = normaliser(quoi)
        quoi = {"mail": "mail", "mails": "mails", "boite mail": "mails", "devoir": "devoirs", "notes": "pronote",
                "emploi du temps": "pronote"}.get(quoi, quoi)
        if givre.ouvrir(quoi, int(n or 0)):
            return Resultat({"pronote": "J'ouvre Pronote.", "mails": "J'ouvre ta boîte mail.",
                             "mail": "J'ouvre ce mail.", "devoirs": "Voici tes devoirs."}.get(quoi, "C'est ouvert."))
        return Resultat("Il faut que la barre Givre soit lancée pour ça.", ok=False)

    def actualiser_meteo():
        return Resultat("Je mets la météo à jour." if givre.actualiser_meteo() else "La barre Givre n'est pas lancée.")

    def etat_pronote():
        e = givre.etat() or {}
        p = e.get("pronote") or {}
        if not e.get("ok"):
            return Resultat("Ma barre n'est pas lancée, donc je n'ai pas accès à Pronote.", ok=False)
        if p.get("desactive"):
            return Resultat("Pronote est désactivé. Pour le remettre : grande île, page Comptes, bouton Réactiver.")
        if not p.get("connecte"):
            return Resultat("Non, Pronote n'est pas connecté. Fais un clic droit sur la barre, puis Paramètres, Comptes, "
                            "Pronote.", ok=False)
        if p.get("erreur"):
            return Resultat(f"Pas vraiment : la connexion à Pronote ne marche plus, mes infos datent du "
                            f"{p.get('mis_a_jour')}. Refais le QR code : clic droit sur la barre, puis Paramètres, Comptes, "
                            "Pronote.", ok=False)
        return Resultat(f"Oui, je suis connecté à Pronote. Dernière mise à jour : {p.get('mis_a_jour')}.")

    reg.outil("etat_pronote", "Dit si Pronote est bien connecté.", {}, [], etat_pronote)
    reg.cacher_a_ia("etat_pronote")
    reg.regle(r"^(?:es tu|t'es|tu es|est ce que tu es) (?:bien )?connecte (?:a|sur|avec) pronote$"
              r"|^(?:est ce que )?pronote (?:marche|fonctionne|est connecte)(?: bien)?$",
              lambda m, t: [("etat_pronote", {})], priorite=25)

    reg.contexte(contexte)
    reg.outil("ouvrir_givre", "Ouvre à l'écran Pronote, la boîte mail, un mail précis ou la liste des devoirs "
              "(seulement si on te demande de les OUVRIR ou de les AFFICHER, pas pour répondre à une question).",
              {"quoi": {"type": "string", "enum": ["pronote", "mails", "mail", "devoirs"]},
               "n": {"type": "integer", "description": "numéro du mail (n°) quand quoi = mail"}},
              ["quoi"], ouvrir_givre)
    # Pas proposé à l'IA (elle l'appelait au lieu de LIRE la météo) : seulement « actualise la météo ».
    reg.outil("actualiser_meteo", "Télécharge de nouveau la météo.", {}, [], actualiser_meteo)
    reg.cacher_a_ia("actualiser_meteo")
    reg.regle(r"^(?:actualise|rafraichis|mets a jour|recharge) la meteo$",
              lambda m, t: [("actualiser_meteo", {})], priorite=30)

    reg.regle(r"^(?:ouvre|ouvrir|affiche|montre)(?: moi)? (?:pronote|mon pronote|l'appli pronote)$",
              lambda m, t: [("ouvrir_givre", {"quoi": "pronote"})], priorite=30)
    reg.regle(r"^(?:ouvre|ouvrir|affiche|montre)(?: moi)? (?:mes mails|ma boite mail|mes e ?mails|mes courriers|ma messagerie)$",
              lambda m, t: [("ouvrir_givre", {"quoi": "mails"})], priorite=30)
    reg.regle(r"^(?:ouvre|ouvrir|affiche|montre)(?: moi)? (?:mes devoirs|les devoirs|mon cahier de textes?)$",
              lambda m, t: [("ouvrir_givre", {"quoi": "devoirs"})], priorite=30)
