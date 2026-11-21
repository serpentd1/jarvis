"""Ouvrir et fermer les applications du PC : tout ce qui est dans le menu Démarrer (Discord, Chrome, Steam…)."""
import difflib
import json
import logging
import os
import random
import re
import subprocess
import threading
from pathlib import Path
import time

import psutil

from core.config import DONNEES
from . import Resultat, normaliser

log = logging.getLogger("applis")
CACHE = DONNEES / "applis.json"
SANS_FENETRE = 0x08000000
MOTS_INUTILES = {"le", "la", "les", "l", "l'", "un", "une", "application", "appli", "app", "logiciel", "jeu",
                 "programme", "moi", "mon", "ma", "mes", "stp", "svp", "s'il", "te", "plait", "vous", "pour"}
# Ce que d'autres modules gèrent mieux (l'île Givre, les médias, les minuteurs…).
PAS_UNE_APPLI = {"mail", "mails", "courrier", "boite mail", "pronote", "devoirs", "notes", "emploi du temps",
                 "agenda", "minuteur", "chrono", "chronometre", "alarme", "son", "volume", "video", "musique",
                 "pause", "lecture", "meteo", "messages", "message", "conversation"}
# Ne jamais fermer : Windows, Jarvis lui-même, la barre, Ollama, LM Studio.
PROTEGES = {"explorer", "svchost", "csrss", "winlogon", "wininit", "lsass", "services", "dwm", "system",
            "python", "pythonw", "jarvis", "givre", "ollama", "ollama app", "lm studio", "lms", "lm-studio",
            "taskmgr", "conhost"}


class Index:
    def __init__(self, alias: dict):
        self.applis = []                       # [(nom affiché, AppID, nom normalisé)]
        self.alias = {normaliser(k): v for k, v in alias.items()}
        self.pret = threading.Event()
        try:
            self._remplir(json.loads(CACHE.read_text(encoding="utf-8")))
        except Exception:  # noqa: BLE001
            pass
        threading.Thread(target=self.actualiser, daemon=True, name="index-applis").start()

    def actualiser(self):
        """Relit la liste des applis du menu Démarrer (≈1 s) et la garde en cache."""
        try:
            r = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress"],
                capture_output=True, text=True, encoding="utf-8", timeout=40, creationflags=SANS_FENETRE)
            donnees = json.loads(r.stdout or "[]")
            donnees = [donnees] if isinstance(donnees, dict) else donnees
            liste = [{"nom": d["Name"], "id": d["AppID"]} for d in donnees if d.get("Name")]
            self._remplir(liste)
            CACHE.parent.mkdir(parents=True, exist_ok=True)
            CACHE.write_text(json.dumps(liste, ensure_ascii=False), encoding="utf-8")
            log.info("%d applications trouvées dans le menu Démarrer", len(self.applis))
        except Exception:  # noqa: BLE001
            log.exception("liste des applications illisible")
        self.pret.set()

    def _remplir(self, liste):
        applis = []
        for a in liste:
            n = normaliser(a["nom"])
            if n.startswith(("desinstaller", "uninstall")) or a["id"].lower().startswith(("http", "mailto")):
                continue
            applis.append((a["nom"], a["id"], n))
        self.applis = applis

    def chercher(self, demande: str):
        """« discorde » -> ("Discord", "com.squirrel.Discord.Discord") ; None si rien de ressemblant."""
        cle = nettoyer(demande)
        if not cle or cle in PAS_UNE_APPLI:
            return None
        cle = normaliser(self.alias.get(cle, cle))
        exact = [a for a in self.applis if a[2] == cle]
        if exact:
            return exact[0][:2]
        contient = [a for a in self.applis if a[2].startswith(cle + " ") or f" {cle} " in f" {a[2]} "
                    or a[2].replace(" ", "") == cle.replace(" ", "")]
        if contient:
            return min(contient, key=lambda a: len(a[2]))[:2]
        noms = [a[2] for a in self.applis]
        proche = difflib.get_close_matches(cle, noms, n=1, cutoff=0.75)
        if not proche:      # ressemblance avec le premier mot (« spotifaille » -> « spotify »)
            premiers = {a[2].split()[0]: a for a in self.applis if a[2]}
            p = difflib.get_close_matches(cle, list(premiers), n=1, cutoff=0.75)
            return premiers[p[0]][:2] if p else None
        return next(a[:2] for a in self.applis if a[2] == proche[0])


def nettoyer(demande: str) -> str:
    mots = normaliser(demande).split()
    while mots and mots[0] in MOTS_INUTILES:
        mots.pop(0)
    while mots and mots[-1] in MOTS_INUTILES:
        mots.pop()
    return " ".join(mots)


def nom_navigateur(prog_id: str) -> str:
    """ProgId de Windows (ChromeHTML, MSEdgeHTM…) -> nom lisible du navigateur."""
    p = (prog_id or "").strip()
    bas = p.lower()
    for debut, nom in (("chromehtml", "Google Chrome"), ("msedgehtm", "Microsoft Edge"), ("firefox", "Firefox"),
                       ("bravehtml", "Brave"), ("opera", "Opera"), ("vivaldi", "Vivaldi")):
        if bas.startswith(debut):
            return nom
    return p


def lire_navigateur_par_defaut() -> str:
    """Le ProgId du navigateur par défaut (registre de l'utilisateur), ou chaîne vide."""
    import winreg
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\Shell\Associations"
                        r"\UrlAssociations\https\UserChoice") as cle:
        return str(winreg.QueryValueEx(cle, "ProgId")[0])


def installer(ctx, reg):
    index = Index(ctx.cfg["applis"])
    ctx.index_applis = index

    def lancer_application(nom: str):
        trouve = index.chercher(nom)
        if not trouve and normaliser(nom) in ("chrome", "google chrome", "navigateur", "internet", "le navigateur"):
            # Chrome n'est pas toujours dans le menu Démarrer : on le lance directement
            for base in (os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)"), os.environ.get("LOCALAPPDATA")):
                exe = Path(base or "") / "Google" / "Chrome" / "Application" / "chrome.exe"
                if base and exe.is_file():
                    subprocess.Popen([str(exe)], creationflags=SANS_FENETRE)
                    return Resultat("J'ouvre Google Chrome.")
        if not trouve:
            site = getattr(ctx, "ouvrir_site", None)
            if site:
                r = site(nom, silencieux=True)
                if r:
                    return r
            return Resultat(f"Je ne trouve aucune application qui s'appelle {nom}.", ok=False)
        affiche, appid = trouve
        subprocess.Popen(["explorer.exe", f"shell:AppsFolder\\{appid}"], creationflags=SANS_FENETRE)
        log.info("lancement : %s (%s)", affiche, appid)
        return Resultat(random.choice([f"J'ouvre {affiche}.", f"C'est parti pour {affiche}.", f"{affiche} arrive."]))

    def fermer_application(nom: str):
        trouve = index.chercher(nom)
        cible = normaliser(trouve[0] if trouve else nom)
        processus = trouver_processus(cible, trouve[1] if trouve else "")
        if not processus:
            return Resultat(f"{trouve[0] if trouve else nom} n'est pas ouvert.", ok=False)
        noms = sorted({p.info["name"] for p in processus})
        for n in noms:      # fermeture « polie » : comme cliquer sur la croix
            subprocess.run(["taskkill", "/IM", n], capture_output=True, creationflags=SANS_FENETRE)
        time.sleep(2.5)
        restants = [n for n in noms if any(p.info["name"] == n for p in psutil.process_iter(["name"]))]
        affiche = trouve[0] if trouve else nom
        if not restants:
            return Resultat(f"J'ai fermé {affiche}.")

        def forcer():
            for n in restants:
                subprocess.run(["taskkill", "/F", "/T", "/IM", n], capture_output=True, creationflags=SANS_FENETRE)
            return Resultat(f"{affiche} est fermé de force.")
        return Resultat(f"{affiche} reste ouvert, peut-être en arrière-plan. Je le ferme de force ?",
                        confirmer=forcer, annuler=f"D'accord, je laisse {affiche} ouvert.")

    ctx.lancer_application = lancer_application
    reg.outil("lancer_application",
              "Ouvre une application ou un jeu installé sur le PC (Discord, Chrome, Spotify, Steam, Roblox, "
              "Minecraft, Bloc-notes…).",
              {"nom": "nom de l'application, tel que l'utilisateur l'a dit"}, ["nom"], lancer_application)
    reg.outil("fermer_application", "Ferme une application ouverte sur le PC.",
              {"nom": "nom de l'application à fermer"}, ["nom"], fermer_application)

    def regle_lancer(m, _texte):
        nom = m["nom"]
        if nettoyer(nom) in PAS_UNE_APPLI:
            return None
        if index.chercher(nom):
            return [("lancer_application", {"nom": nom})]
        return None     # pas une appli connue : un autre raccourci (site…) ou l'IA décidera

    def regle_fermer(m, _texte):
        return [("fermer_application", {"nom": m["nom"]})] if index.chercher(m["nom"]) else None

    reg.regle(r"^(?:est ce que )?(?:tu (?:peux|pourrais) |peux tu |pourrais tu )?(?:me )?"
              r"(?:lance|lancer|ouvre|ouvrir|demarre|demarrer|allume|allumer)(?: moi)? (?P<nom>.+)$",
              regle_lancer, priorite=80)
    reg.regle(r"^(?:tu peux |peux tu )?(?:ferme|fermer|quitte|quitter)(?: moi)? (?P<nom>.+)$", regle_fermer, 80)

    def navigateur_par_defaut():
        try:
            prog = lire_navigateur_par_defaut()
        except (OSError, ImportError):
            return Resultat("Je n'arrive pas à voir quel est ton navigateur par défaut.", ok=False)
        nom = nom_navigateur(prog)
        return Resultat(f"Ton navigateur par défaut est {nom}." if nom else "Je ne vois pas de navigateur par défaut.",
                        pour_ia=f"Navigateur par défaut de Windows : {nom} (ProgId {prog})")

    reg.outil("navigateur_par_defaut", "Dit quel est le navigateur Internet par défaut de Windows.", {}, [],
              navigateur_par_defaut)
    reg.regle(r"^(?:quel est|c'est quoi|quel)(?: est)? (?:mon |le )?(?:navigateur|browser)(?: internet| web)?(?: par defaut)?"
              r"(?: sur mon (?:pc|ordi|ordinateur))?$"
              r"|^(?:quel|c'est quoi mon) navigateur (?:j'ai|j'utilise|utilise tu|est utilise) ?(?:par defaut)?$",
              lambda m, t: [("navigateur_par_defaut", {})], priorite=40)


def trouver_processus(cible: str, appid: str = ""):
    """Processus qui correspondent à l'appli : « google chrome » -> chrome.exe, « discord » -> Discord.exe,
    « Bloc-notes » (Microsoft.WindowsNotepad_…!App) -> Notepad.exe."""
    mots = [m for m in cible.split() if len(m) >= 3 and m not in ("google", "microsoft", "player", "launcher")]
    exe_appid = appid.replace("/", "\\").rsplit("\\", 1)[-1].lower() if appid.lower().endswith(".exe") else ""
    morceaux_id = [t.removeprefix("windows") for t in re.split(r"[._!\\ {}-]+", appid.lower())
                   if len(t) >= 5 and t not in ("microsoft", "windows") and not t.startswith("8wekyb")]
    trouves = []
    for p in psutil.process_iter(["name"]):
        nom = (p.info["name"] or "").lower()
        base = normaliser(nom.removesuffix(".exe")).replace(" ", "")
        if not base or base in PROTEGES:
            continue
        if (nom == exe_appid or any(m in base for m in mots)
                or (len(base) >= 4 and any(t and (t in base or base in t) for t in morceaux_id))):
            trouves.append(p)
    return trouves
