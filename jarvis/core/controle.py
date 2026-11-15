"""Écran, souris et clavier de Windows (ctypes, sans dépendance) : capture d'écran, clic, frappe, touches."""
import ctypes
import time
from ctypes import wintypes

user32 = ctypes.windll.user32
try:      # coordonnées en vrais pixels, même avec un zoom d'affichage à 125 % / 150 %
    ctypes.windll.shcore.SetProcessDpiAwareness(2)
except Exception:  # noqa: BLE001
    pass

ENTREE_SOURIS, ENTREE_CLAVIER = 0, 1
GAUCHE_BAS, GAUCHE_HAUT, DROIT_BAS, DROIT_HAUT, MOLETTE = 0x0002, 0x0004, 0x0008, 0x0010, 0x0800
TOUCHE_HAUT, UNICODE = 0x0002, 0x0004


class _Souris(ctypes.Structure):
    _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
                ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _Clavier(ctypes.Structure):
    _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class _Union(ctypes.Union):
    _fields_ = [("mi", _Souris), ("ki", _Clavier), ("pad", ctypes.c_byte * 32)]


class _Entree(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("u", _Union)]


def _envoyer(*entrees):
    tableau = (_Entree * len(entrees))(*entrees)
    user32.SendInput(len(entrees), tableau, ctypes.sizeof(_Entree))


def _souris(drapeau, donnee=0):
    e = _Entree(type=ENTREE_SOURIS)
    e.u.mi = _Souris(0, 0, ctypes.c_ulong(donnee & 0xFFFFFFFF).value, drapeau, 0, 0)
    return e


def _touche(vk=0, scan=0, drapeaux=0):
    e = _Entree(type=ENTREE_CLAVIER)
    e.u.ki = _Clavier(vk, scan, drapeaux, 0, 0)
    return e


# ── écran ───────────────────────────────────────────────────────────────────

def taille_ecran() -> tuple[int, int]:
    return user32.GetSystemMetrics(0), user32.GetSystemMetrics(1)


def capture(largeur_max: int = 1280):
    """Capture de l'écran principal, réduite (moins de travail pour l'IA)."""
    from PIL import ImageGrab
    image = ImageGrab.grab()
    if image.width > largeur_max:
        image = image.resize((largeur_max, round(image.height * largeur_max / image.width)))
    return image


# ── souris ──────────────────────────────────────────────────────────────────

def deplacer(x: int, y: int, duree: float = 0.25):
    """Déplace la souris en douceur (on voit où elle va)."""
    depart = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(depart))
    etapes = max(1, int(duree / 0.01))
    for i in range(1, etapes + 1):
        user32.SetCursorPos(int(depart.x + (x - depart.x) * i / etapes), int(depart.y + (y - depart.y) * i / etapes))
        time.sleep(duree / etapes)


def cliquer(x: int, y: int, droit: bool = False, double: bool = False):
    deplacer(x, y)
    bas, haut = (DROIT_BAS, DROIT_HAUT) if droit else (GAUCHE_BAS, GAUCHE_HAUT)
    for _ in range(2 if double else 1):
        _envoyer(_souris(bas), _souris(haut))
        time.sleep(0.06)


def molette(crans: int):
    """crans > 0 : vers le haut ; < 0 : vers le bas."""
    _envoyer(_souris(MOLETTE, 120 * crans))


# ── clavier ─────────────────────────────────────────────────────────────────

TOUCHES = {
    "ctrl": 0x11, "control": 0x11, "controle": 0x11, "alt": 0x12, "maj": 0x10, "shift": 0x10, "majuscule": 0x10,
    "win": 0x5B, "windows": 0x5B, "entree": 0x0D, "enter": 0x0D, "entrer": 0x0D, "echap": 0x1B, "echappe": 0x1B,
    "escape": 0x1B, "tab": 0x09, "tabulation": 0x09, "espace": 0x20, "retour": 0x08, "effacer": 0x08,
    "backspace": 0x08, "suppr": 0x2E, "supprimer": 0x2E, "haut": 0x26, "bas": 0x28, "gauche": 0x25, "droite": 0x27,
    "debut": 0x24, "fin": 0x23, "page haut": 0x21, "page bas": 0x22, "imprime ecran": 0x2C,
}


def code_touche(nom: str):
    nom = nom.strip().lower()
    if nom in TOUCHES:
        return TOUCHES[nom]
    if len(nom) >= 2 and nom[0] == "f" and nom[1:].isdigit() and 1 <= int(nom[1:]) <= 12:
        return 0x70 + int(nom[1:]) - 1
    if len(nom) == 1 and nom.isalnum():
        return ord(nom.upper())
    return None


def raccourci(touches: list[str]) -> bool:
    """Appuie ensemble sur des touches : ["ctrl", "c"], ["alt", "tab"], ["entree"]…"""
    codes = [code_touche(t) for t in touches]
    if not codes or None in codes:
        return False
    _envoyer(*[_touche(c) for c in codes], *[_touche(c, drapeaux=TOUCHE_HAUT) for c in reversed(codes)])
    return True


def taper(texte: str):
    """Tape le texte dans la fenêtre active, caractère par caractère (accents compris)."""
    for caractere in texte:
        if caractere == "\n":
            raccourci(["entree"])
            continue
        donnees = caractere.encode("utf-16-le")          # (les émojis prennent deux « morceaux »)
        for i in range(0, len(donnees), 2):
            code = int.from_bytes(donnees[i:i + 2], "little")
            _envoyer(_touche(scan=code, drapeaux=UNICODE), _touche(scan=code, drapeaux=UNICODE | TOUCHE_HAUT))
        time.sleep(0.008)
