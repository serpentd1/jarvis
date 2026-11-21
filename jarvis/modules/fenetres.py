"""Fenêtres de Windows, pour N'IMPORTE QUELLE appli ou page : « mets en petit la page », « réduis Discord »,
« agrandis la fenêtre », « mets-la en plein écran », « remets-la », « ferme cette fenêtre », « montre le bureau ».

Un seul outil général (`fenetre`) : il agit sur la fenêtre qui est devant toi (ou sur celle dont tu dis le nom),
sans avoir besoin d'un outil par application. Windows seulement (ctypes, sans dépendance)."""
import ctypes
import logging
import os
import re
import time
from ctypes import wintypes

from . import Resultat, normaliser

log = logging.getLogger("fenetres")

ACTIONS = ["reduire", "agrandir", "restaurer", "fermer", "premier_plan", "plein_ecran", "gauche", "droite",
           "tout_reduire"]
# Fenêtres qu'on ne touche jamais : le bureau, la barre des tâches, la barre (l'île) de Jarvis.
CLASSES_IGNOREES = {"progman", "workerw", "shell_traywnd", "shell_secondarytraywnd", "windows.ui.core.corewindow",
                    "notifyiconoverflowwindow", "tooltips_class32", "#32768"}
PROCESSUS_IGNORES = {"jarvisbarre.exe", "jarvis.exe", "shellexperiencehost.exe", "searchhost.exe",
                     "startmenuexperiencehost.exe", "textinputhost.exe", "lockapp.exe"}
NAVIGATEURS = {"chrome.exe", "msedge.exe", "firefox.exe", "brave.exe", "opera.exe", "vivaldi.exe"}

# ── comprendre la phrase (raccourci sans IA) ────────────────────────────────
# L'ordre compte : la première action reconnue gagne.
MOTIFS = [
    ("tout_reduire", r"\b(?:(?:redui[st]?|reduire|minimise[rz]?|cache[rz]?) (?:toutes les fenetres|tout)\b|"
                     r"(?:montre|affiche|va sur)(?: moi)? (?:le )?bureau\b)"),
    ("plein_ecran", r"\ben plein ecran\b|\bplein ecran\b"),
    ("gauche", r"\b(?:mets?|met|mettre|place|deplace|colle)\b(?: \S+){0,5} (?:a|sur la|sur le cote) gauche\b"),
    ("droite", r"\b(?:mets?|met|mettre|place|deplace|colle)\b(?: \S+){0,5} (?:a|sur la|sur le cote) droite\b"),
    ("premier_plan", r"\bau premier plan\b|\b(?:bascule|passe|va) sur la fenetre\b"),
    ("restaurer", r"\b(?:restaure[rz]?|reaffiche[rz]?|fais (?:re)?apparaitre)\b|"
                  r"\bremets?\b(?: \S+){0,4} (?:comme avant|en normal|normal(?:e|ement)?|a sa taille)\b|"
                  r"^(?:remets?|rouvre) (?:la|le|la page|la fenetre)$"),
    ("agrandir", r"\b(?:agrandi[st]?|agrandir|maximise[rz]?)\b|\b(?:mets?|met|mettre)\b(?: \S+){0,5} en grand\b"),
    ("reduire", r"\b(?:redui[st]?|reduire|minimise[rz]?)\b|\b(?:mets?|met|mettre)\b(?: \S+){0,5} en (?:tout )?petit\b"),
    ("fermer", r"^(?:tu peux |peux tu )?(?:ferme[rz]?|quitte[rz]?)\b"),
]
MOTIFS = [(a, re.compile(m)) for a, m in MOTIFS]
# mots qui ne désignent pas une fenêtre précise (« la page qui est sur l'écran » = celle de devant)
VIDES = set("""moi la le les l cette cet ce ca ta ma mon ton sa son page fenetre appli application logiciel programme
truc qui est sur ecran actuelle actuel ouverte ouvert en cours devant active actif du de des d s'il te plait stp jarvis
tu peux peut pourrais un une peu petit grand tout plein premier plan que j ai je vois regarde dessus la bas ici
mets met mettre remets remet reduis reduit reduire minimise minimiser agrandis agrandit agrandir maximise maximiser
restaure restaurer ferme fermer quitte quitter place deplace colle a gauche droite cote comme avant normal normale
normalement taille fais apparaitre reapparaitre reaffiche bascule passe va et elle il lui y au aux montre affiche
bureau toutes fenetres cache cacher""".split())
# ce n'est pas une fenêtre : le son, la luminosité… (d'autres outils s'en chargent)
PAS_FENETRE = re.compile(r"\b(?:son|volume|luminosite|bruit|musique|onglets?|minuteur|alarme|lumiere|police|zoom|"
                         r"texte|ecriture|vitesse|taille du texte)\b")


def comprendre(n: str):
    """« mets en petit la page qui est sur l'ecran » -> ("reduire", "") ; « reduis discord » -> ("reduire", "discord").
    None si la phrase ne parle pas d'une fenêtre."""
    if PAS_FENETRE.search(n):
        return None
    for action, motif in MOTIFS:
        if motif.search(n):
            break
    else:
        return None
    mots = [m for m in re.sub(r"[']", " ", n).split() if m not in VIDES]
    if len(mots) > 3:            # trop de choses en plus : ce n'est sans doute pas (seulement) une fenêtre
        return None
    nom = " ".join(mots)
    generique = re.search(r"\b(?:page|fenetre|ca|la|le|ecran)\b", n) is not None
    if action == "fermer" and (nom or not generique):
        return None               # « ferme Discord » : c'est fermer_application qui s'en charge
    return action, nom


# ── Windows ─────────────────────────────────────────────────────────────────
try:
    user32 = ctypes.windll.user32
    dwmapi = ctypes.windll.dwmapi
except AttributeError:          # pas sous Windows (tests ailleurs)
    user32 = dwmapi = None

SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE = 3, 6, 9
WM_CLOSE = 0x0010
GWL_EXSTYLE, WS_EX_TOOLWINDOW = -20, 0x00000080
DWMWA_CLOAKED = 14
_ENUM = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)


def _titre(h) -> str:
    n = user32.GetWindowTextLengthW(h)
    tampon = ctypes.create_unicode_buffer(n + 1)
    user32.GetWindowTextW(h, tampon, n + 1)
    return tampon.value


def _classe(h) -> str:
    tampon = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(h, tampon, 256)
    return tampon.value


def _processus(h) -> str:
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(h, ctypes.byref(pid))
    if pid.value == os.getpid():
        return "jarvis.exe"
    try:
        import psutil
        return psutil.Process(pid.value).name().lower()
    except Exception:  # noqa: BLE001
        return ""


def _cachee(h) -> bool:
    """Fenêtre « fantôme » (applis du Store en arrière-plan) : visible pour Windows, mais pas à l'écran."""
    val = ctypes.c_int(0)
    try:
        dwmapi.DwmGetWindowAttribute(h, DWMWA_CLOAKED, ctypes.byref(val), ctypes.sizeof(val))
    except Exception:  # noqa: BLE001
        return False
    return val.value != 0


def utilisable(h) -> bool:
    if not h or not user32.IsWindowVisible(h) or _cachee(h):
        return False
    if user32.GetWindowLongW(h, GWL_EXSTYLE) & WS_EX_TOOLWINDOW:
        return False
    classe = _classe(h).lower()
    if classe in CLASSES_IGNOREES or classe.startswith("jarvisbarre"):
        return False
    if not _titre(h).strip():
        return False
    return _processus(h) not in PROCESSUS_IGNORES


def fenetres():
    """Les vraies fenêtres ouvertes, de celle du dessus à celle du dessous."""
    liste = []

    def chaque(h, _):
        if utilisable(h):
            liste.append(h)
        return True
    user32.EnumWindows(_ENUM(chaque), 0)
    return liste


def devant():
    """La fenêtre que tu regardes : celle au premier plan, sinon la plus haute qui n'est pas réduite."""
    h = user32.GetForegroundWindow()
    if utilisable(h) and not user32.IsIconic(h):
        return h
    return next((f for f in fenetres() if not user32.IsIconic(f)), None)


def chercher(nom: str, reduite: bool | None = None):
    """Fenêtre dont le titre ou le programme ressemble à `nom` (« discord », « chrome », « youtube »)."""
    cle = normaliser(nom).replace(" ", "")
    if not cle:
        return None
    for h in fenetres():
        if reduite is not None and bool(user32.IsIconic(h)) != reduite:
            continue
        titre = normaliser(_titre(h)).replace(" ", "")
        prog = normaliser(_processus(h).removesuffix(".exe")).replace(" ", "")
        if cle in titre or (prog and (cle in prog or prog in cle)):
            return h
    return None if reduite is None else chercher(nom, None)


def activer(h):
    """Met la fenêtre au premier plan (Windows le refuse parfois à un programme en fond : l'appui sur Alt aide)."""
    if user32.IsIconic(h):
        user32.ShowWindow(h, SW_RESTORE)
    user32.keybd_event(0x12, 0, 0, 0)
    user32.keybd_event(0x12, 0, 2, 0)
    user32.SetForegroundWindow(h)
    time.sleep(0.15)


def nom_court(h) -> str:
    """« YouTube - Google Chrome » -> « YouTube » (ce qui se dit bien à voix haute)."""
    titre = _titre(h).strip()
    morceaux = [m.strip() for m in re.split(r"\s+[-–—|]\s+", titre) if m.strip()]
    court = morceaux[0] if morceaux else titre
    return court[:60] or "la fenêtre"


def installer(ctx, reg):
    def contexte_fenetre(_question: str = "") -> str:
        h = devant()
        if not h:
            return ""
        titre = _titre(h).strip()
        return f"DEVANT L'UTILISATEUR (fenêtre au premier plan) : {nom_court(h)}" + (f" — « {titre[:120]} »" if titre else "") + "."

    reg.contexte(contexte_fenetre)
    dernier = {"h": None}             # la dernière fenêtre réduite (« remets-la »)

    def fenetre(action: str, nom: str = "", tout: bool = False):
        from core import controle
        tout = tout is True or str(tout).lower() in ("true", "1", "oui")
        action = (action or "").strip().lower().replace(" ", "_").replace("-", "_")
        action = {"minimiser": "reduire", "mettre_en_petit": "reduire", "maximiser": "agrandir",
                  "premier": "premier_plan", "afficher": "premier_plan", "plein": "plein_ecran"}.get(action, action)
        if action not in ACTIONS:
            return Resultat("Je ne connais pas cette action sur les fenêtres.", ok=False)
        if user32 is None:
            return Resultat("Je ne sais gérer les fenêtres que sous Windows.", ok=False)
        if action == "tout_reduire":
            controle.raccourci(["win", "m"])
            return Resultat("Toutes les fenêtres sont réduites.")
        h = None
        if nom:
            h = chercher(nom, reduite=True if action in ("restaurer", "premier_plan") else False)
            if h is None and action in ("fermer", "premier_plan", "restaurer"):
                return Resultat(f"Je ne trouve pas de fenêtre « {nom} » ouverte.", ok=False)
        if h is None and action == "restaurer":
            h = dernier["h"] if dernier["h"] and user32.IsWindow(dernier["h"]) else None
            h = h or next((f for f in fenetres() if user32.IsIconic(f)), None)
            if h is None:
                return Resultat("Il n'y a pas de fenêtre réduite à remettre.", ok=False)
        if h is None:
            h = devant()
        if h is None:
            return Resultat("Je ne vois aucune fenêtre ouverte.", ok=False)
        quoi = nom_court(h)
        log.info("fenêtre : %s sur « %s » (%s)", action, _titre(h), _processus(h))
        if action == "reduire":
            user32.ShowWindow(h, SW_MINIMIZE)
            dernier["h"] = h
            return Resultat(f"J'ai mis {quoi} en petit.")
        if action == "agrandir":
            user32.ShowWindow(h, SW_MAXIMIZE)
            return Resultat(f"{quoi} est en grand.")
        if action == "restaurer":
            user32.ShowWindow(h, SW_RESTORE)
            activer(h)
            return Resultat(f"Voilà, {quoi} est revenu.")
        if action == "premier_plan":
            activer(h)
            return Resultat(f"Voilà {quoi}.")
        if action == "plein_ecran":
            activer(h)
            controle.raccourci(["f11"])
            return Resultat(f"{quoi} est en plein écran.")
        if action in ("gauche", "droite"):
            activer(h)
            controle.raccourci(["win", action])
            return Resultat(f"J'ai mis {quoi} à {action}.")
        # fermer : dans un navigateur, « la page » = l'onglet (pas toute la fenêtre et ses autres onglets)
        if _processus(h) in NAVIGATEURS and not tout:
            activer(h)
            controle.raccourci(["ctrl", "w"])
            return Resultat("J'ai fermé la page.")
        user32.PostMessageW(h, WM_CLOSE, 0, 0)
        return Resultat(f"J'ai fermé {quoi}.")

    reg.outil("fenetre",
              "Agit sur N'IMPORTE QUELLE fenêtre ou page affichée à l'écran, quelle que soit l'application (Chrome, "
              "YouTube, Discord, jeu, explorateur…) : la réduire / mettre en petit / minimiser / cacher, l'agrandir "
              "/ mettre en grand / maximiser, la mettre en plein écran, la restaurer / remettre, la fermer, la mettre "
              "au premier plan, à gauche ou à droite de l'écran, ou tout réduire (montrer le bureau). Sans nom : la "
              "fenêtre que l'utilisateur a devant lui.",
              {"action": {"type": "string", "enum": ACTIONS, "description": "ce qu'il faut faire"},
               "nom": "nom de l'appli ou de la page visée (vide = la fenêtre à l'écran)",
               "tout": {"type": "boolean", "description": "fermer : true = toute la fenêtre du navigateur, "
                                                          "false (défaut) = seulement la page (l'onglet)"}},
              ["action"], fenetre)

    def regle(_m, texte):
        n = normaliser(texte)
        trouve = comprendre(n)
        if not trouve:
            return None
        action, nom = trouve
        args = {"action": action, "nom": nom}
        if action == "fermer" and "fenetre" in n:
            args["tout"] = True
        return [("fenetre", args)]

    # avant « ferme la page X » (onglets, priorité 13) et « ferme Discord » (applis, 80)
    reg.regle(r"\b(?:redui[st]?|reduire|minimise[rz]?|agrandi[st]?|agrandir|maximise[rz]?|restaure[rz]?|en petit|"
              r"en grand|plein ecran|premier plan|bureau|a gauche|a droite|comme avant|reaffiche|apparaitre|"
              r"^(?:tu peux |peux tu )?(?:ferme[rz]?|quitte[rz]?) (?:la |cette |ta )?(?:page|fenetre)|"
              r"^(?:remets?|rouvre) (?:la|le|la page|la fenetre)$)", regle, priorite=11)
