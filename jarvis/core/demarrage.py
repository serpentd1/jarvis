"""« Démarrer avec Windows » : une entrée dans HKCU\\...\\Run qui lance Jarvis.exe à l'ouverture de session."""
import logging
import winreg

from .config import RACINE

log = logging.getLogger("demarrage")
CLE = r"Software\Microsoft\Windows\CurrentVersion\Run"
NOM = "Jarvis"
EXE = RACINE / "Jarvis.exe"


def est_active() -> bool:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, CLE) as k:
            winreg.QueryValueEx(k, NOM)
            return True
    except OSError:
        return False


def activer(oui: bool) -> bool:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, CLE, 0, winreg.KEY_SET_VALUE) as k:
            if oui:
                if not EXE.is_file():
                    log.error("Jarvis.exe introuvable (%s) : démarrage automatique impossible", EXE)
                    return False
                # --demarrage : Jarvis.exe sait que c'est Windows qui le lance (« voix au démarrage du PC »)
                winreg.SetValueEx(k, NOM, 0, winreg.REG_SZ, f'"{EXE}" --demarrage')
            else:
                try:
                    winreg.DeleteValue(k, NOM)
                except FileNotFoundError:
                    pass
        log.info("Démarrer avec Windows : %s", "oui" if oui else "non")
        return True
    except OSError:
        log.exception("registre")
        return False
