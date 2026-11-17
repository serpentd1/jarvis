"""Batteries de tout ce qui est relié au PC : manettes Xbox, casques, souris, claviers et téléphones Bluetooth,
batterie du PC portable. Alerte à voix haute sous 15 %."""
import ctypes
import json
import logging
import subprocess
import threading
import time

import psutil

from . import Resultat

log = logging.getLogger("appareils")
SANS_FENETRE = 0x08000000
SCRIPT_BLUETOOTH = (
    "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-PnpDevice -PresentOnly | Where-Object { "
    "$_.InstanceId -like 'BTHENUM*' -or $_.InstanceId -like 'BTHLE*' -or $_.InstanceId -like 'BTHHFENUM*' } | "
    "ForEach-Object { $b = (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName "
    "'{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2' -ErrorAction SilentlyContinue).Data; "
    "if ($b -ne $null) { [pscustomobject]@{nom=$_.FriendlyName; batterie=[int]$b; classe=$_.Class} } } | ConvertTo-Json -Compress"
)


class _Batterie(ctypes.Structure):
    _fields_ = [("BatteryType", ctypes.c_ubyte), ("BatteryLevel", ctypes.c_ubyte)]


def manettes_xbox() -> list:
    try:
        xinput = ctypes.windll.xinput1_4
    except OSError:
        return []
    resultats = []
    for i in range(4):
        info = _Batterie()
        if xinput.XInputGetBatteryInformation(i, 0, ctypes.byref(info)) != 0 or info.BatteryType == 0:
            continue
        if info.BatteryType == 1:
            resultats.append({"nom": f"Manette {i + 1}", "type": "manette", "batterie": None, "branche": True})
        else:
            resultats.append({"nom": f"Manette {i + 1}", "type": "manette",
                              "batterie": {0: 5, 1: 25, 2: 60, 3: 100}.get(info.BatteryLevel, 50), "branche": False})
    return resultats


def type_appareil(nom: str, classe: str = "") -> str:
    n = (nom + " " + classe).lower()
    for mot, t in (("manette", "manette"), ("controller", "manette"), ("gamepad", "manette"), ("casque", "casque"),
                   ("headset", "casque"), ("buds", "ecouteurs"), ("airpods", "ecouteurs"), ("ecouteur", "ecouteurs"),
                   ("mouse", "souris"), ("souris", "souris"), ("keyboard", "clavier"), ("clavier", "clavier"),
                   ("phone", "telephone"), ("galaxy", "telephone"), ("pixel", "telephone"), ("iphone", "telephone"),
                   ("xiaomi", "telephone"), ("redmi", "telephone"), ("audio", "casque")):
        if mot in n:
            return t
    return "autre"


def installer(ctx, reg):
    cache = {"t": 0, "liste": [], "alertes": {}}
    verrou = threading.Lock()

    def bluetooth() -> list:
        try:
            r = subprocess.run(["powershell", "-NoProfile", "-Command", SCRIPT_BLUETOOTH], capture_output=True,
                               text=True, encoding="utf-8", timeout=25, creationflags=SANS_FENETRE)
            donnees = json.loads(r.stdout or "[]")
            donnees = [donnees] if isinstance(donnees, dict) else donnees
        except Exception:  # noqa: BLE001
            return []
        vus, liste = set(), []
        for x in donnees:
            nom = str(x.get("nom", "")).replace("Hands-Free AG", "").replace("Avrcp Transport", "").strip()
            if nom and nom not in vus:
                vus.add(nom)
                liste.append({"nom": nom, "type": type_appareil(nom, str(x.get("classe", ""))),
                              "batterie": int(x.get("batterie", 0)), "branche": False})
        return liste

    def tout(age_max: float = 90) -> list:
        with verrou:
            if time.time() - cache["t"] < age_max:
                return cache["liste"]
            liste = []
            pc = psutil.sensors_battery()
            if pc:
                liste.append({"nom": "Ce PC", "type": "pc", "batterie": round(pc.percent), "branche": bool(pc.power_plugged)})
            liste += manettes_xbox() + bluetooth()
            cache.update(t=time.time(), liste=liste)
            return liste

    def surveiller():
        while not ctx.etat.quitter.wait(180):
            for a in tout(age_max=0):
                b = a.get("batterie")
                if b is not None and b <= 15 and not a.get("branche") and time.time() - cache["alertes"].get(a["nom"], 0) > 3600:
                    cache["alertes"][a["nom"]] = time.time()
                    ctx.etat.annoncer(f"Attention, la batterie de {a['nom']} est à {b} %.")
                    ctx.givre.notif("Batterie faible", f"{a['nom']} : {b} %", "jarvis")

    def batteries():
        liste = [a for a in tout() if a.get("batterie") is not None]
        if not liste:
            return Resultat("Je ne vois aucun appareil avec une batterie.")
        return Resultat(" ; ".join(f"{a['nom']} : {a['batterie']} %" for a in liste) + ".")

    reg.api("appareils", lambda methode, p: {"ok": True, "appareils": tout(age_max=0 if p.get("forcer") else 60)})
    reg.outil("batteries", "Dit le niveau de batterie des appareils reliés (manette, casque, souris, téléphone…).",
              {}, [], batteries)
    reg.regle(r"\b(batterie|batteries)\b", lambda m, t: [("batteries", {})], priorite=30)
    if not getattr(ctx, "simulation", False):
        reg.tache(surveiller)
