"""Copiés : l'historique de ton presse-papiers (les 50 derniers textes copiés).
Gardé en mémoire SEULEMENT (jamais écrit sur le disque) : on copie parfois des mots de passe."""
import ctypes
import time
import uuid
from collections import deque

user32, kernel32 = ctypes.windll.user32, ctypes.windll.kernel32
user32.GetClipboardData.restype = ctypes.c_void_p
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [ctypes.c_void_p]
kernel32.GlobalUnlock.argtypes = [ctypes.c_void_p]
kernel32.GlobalAlloc.restype = ctypes.c_void_p
user32.SetClipboardData.argtypes = [ctypes.c_uint, ctypes.c_void_p]
TEXTE_UNICODE, MOBILE = 13, 0x0002


def lire_presse_papiers():
    for _ in range(5):                     # une autre appli peut le tenir une fraction de seconde
        if user32.OpenClipboard(None):
            break
        time.sleep(0.05)
    else:
        return None
    try:
        h = user32.GetClipboardData(TEXTE_UNICODE)
        if not h:
            return None
        p = kernel32.GlobalLock(h)
        try:
            return ctypes.wstring_at(p) if p else None
        finally:
            kernel32.GlobalUnlock(h)
    finally:
        user32.CloseClipboard()


def ecrire_presse_papiers(texte: str) -> bool:
    donnees = ctypes.create_unicode_buffer(texte)
    taille = ctypes.sizeof(donnees)
    h = kernel32.GlobalAlloc(MOBILE, taille)
    p = kernel32.GlobalLock(h)
    ctypes.memmove(p, donnees, taille)
    kernel32.GlobalUnlock(h)
    if not user32.OpenClipboard(None):
        return False
    try:
        user32.EmptyClipboard()
        return bool(user32.SetClipboardData(TEXTE_UNICODE, h))
    finally:
        user32.CloseClipboard()


def installer(ctx, reg):
    historique = deque(maxlen=50)
    etat = {"ignorer": None}

    def boucle():
        dernier = user32.GetClipboardSequenceNumber()
        while not ctx.etat.quitter.wait(0.7):
            numero = user32.GetClipboardSequenceNumber()
            if numero == dernier:
                continue
            dernier = numero
            texte = lire_presse_papiers()
            if not texte or not texte.strip() or texte == etat["ignorer"]:
                continue
            for x in list(historique):             # pas de doublon : on remonte l'ancien en haut
                if x["texte"] == texte:
                    historique.remove(x)
            historique.appendleft({"id": uuid.uuid4().hex[:8], "texte": texte[:20000], "heure": time.strftime("%H:%M")})

    def api(methode, p):
        if methode == "POST":
            a = p.get("action")
            if a == "copier":
                x = next((x for x in historique if x["id"] == p.get("id")), None)
                if x:
                    etat["ignorer"] = x["texte"]
                    ecrire_presse_papiers(x["texte"])
            elif a == "supprimer":
                for x in list(historique):
                    if x["id"] == p.get("id"):
                        historique.remove(x)
            elif a == "effacer":
                historique.clear()
        return {"ok": True, "copies": list(historique)}

    reg.api("copies", api)
    if not getattr(ctx, "simulation", False):
        reg.tache(boucle)
