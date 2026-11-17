"""Mon PC : processeur, mémoire, carte graphique, disque, et quelques commandes (verrouiller, veille, écran…)."""
import ctypes
import subprocess
import time

import psutil

from . import Resultat

SANS_FENETRE = 0x08000000
ACTIONS = {
    "verrouiller": "Verrouiller", "ecran_off": "Éteindre l'écran", "veille": "Mettre en veille",
    "gestionnaire": "Gestionnaire des tâches", "parametres": "Paramètres Windows", "corbeille": "Vider la corbeille",
    "redemarrer": "Redémarrer", "eteindre": "Éteindre le PC",
}
A_CONFIRMER = {"veille", "corbeille", "redemarrer", "eteindre"}


def gpu():
    try:
        r = subprocess.run(["nvidia-smi", "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu",
                            "--format=csv,noheader,nounits"], capture_output=True, text=True, timeout=5,
                           creationflags=SANS_FENETRE)
        nom, *valeurs = [x.strip() for x in r.stdout.strip().splitlines()[0].split(",")]
        util, util_mem, total, temp = (round(float(v)) for v in valeurs)     # (nvidia-smi écrit « 37.00 »)
        return {"nom": nom, "utilisation": util, "memoire_mo": util_mem, "memoire_totale_mo": total, "temperature": temp}
    except Exception:  # noqa: BLE001
        return None


def faire(action: str):
    if action == "verrouiller":
        ctypes.windll.user32.LockWorkStation()
    elif action == "ecran_off":
        ctypes.windll.user32.PostMessageW(0xFFFF, 0x0112, 0xF170, 2)
    elif action == "veille":
        subprocess.Popen(["powershell", "-NoProfile", "-Command", "Add-Type -AssemblyName System.Windows.Forms; "
                          "[System.Windows.Forms.Application]::SetSuspendState('Suspend', $false, $false)"],
                         creationflags=SANS_FENETRE)
    elif action == "gestionnaire":
        subprocess.Popen(["taskmgr.exe"])
    elif action == "parametres":
        subprocess.Popen(["explorer.exe", "ms-settings:"])
    elif action == "corbeille":
        ctypes.windll.shell32.SHEmptyRecycleBinW(None, None, 0x07)
    elif action == "redemarrer":
        subprocess.Popen(["shutdown", "/r", "/t", "10"], creationflags=SANS_FENETRE)
    elif action == "eteindre":
        subprocess.Popen(["shutdown", "/s", "/t", "10"], creationflags=SANS_FENETRE)
    else:
        return False
    return True


def installer(ctx, reg):
    cache = {"t": 0, "gpu": None}
    psutil.cpu_percent(None)

    def stats():
        if time.time() - cache["t"] > 4:
            cache.update(t=time.time(), gpu=gpu())
        memoire, disque = psutil.virtual_memory(), psutil.disk_usage("C:\\")
        processus = []
        for p in psutil.process_iter(["name", "memory_info"]):
            try:
                processus.append((p.info["memory_info"].rss, p.info["name"]))
            except Exception:  # noqa: BLE001
                pass
        processus.sort(reverse=True)
        return {"ok": True, "cpu": psutil.cpu_percent(None), "coeurs": psutil.cpu_count(),
                "ram_utilisee": memoire.used, "ram_totale": memoire.total, "ram_pourcent": memoire.percent,
                "disque_utilise": disque.used, "disque_total": disque.total, "gpu": cache["gpu"],
                "allume_depuis": time.time() - psutil.boot_time(),
                "processus": [{"nom": n, "ram": r} for r, n in processus[:8]], "actions": ACTIONS}

    def api(methode, p):
        if methode == "POST" and p.get("action") in ACTIONS:
            return {"ok": faire(p["action"])}
        return stats()

    def etat_pc():
        s = stats()
        texte = f"Processeur à {round(s['cpu'])} %, mémoire à {round(s['ram_pourcent'])} %"
        if s["gpu"]:
            texte += f", carte graphique à {s['gpu']['utilisation']} % et {s['gpu']['temperature']} degrés"
        return Resultat(texte + ".")

    def action_pc(action: str):
        if action not in ACTIONS or action in ("verrouiller",):
            return Resultat("Je ne connais pas cette action.", ok=False) if action not in ACTIONS else \
                Resultat("Je verrouille." if faire(action) else "Ça n'a pas marché.")
        if action in A_CONFIRMER:
            return Resultat(f"{ACTIONS[action]} : tu confirmes ?", confirmer=lambda: Resultat(
                "C'est parti." if faire(action) else "Ça n'a pas marché."), annuler="D'accord, j'annule.")
        return Resultat("C'est fait." if faire(action) else "Ça n'a pas marché.")

    reg.api("pc", api)
    reg.outil("etat_pc", "Dit comment va le PC : processeur, mémoire, carte graphique.", {}, [], etat_pc)
    reg.outil("action_pc", "Commande du PC : " + ", ".join(f"{k} ({v})" for k, v in ACTIONS.items()) + ".",
              {"action": {"type": "string", "enum": list(ACTIONS)}}, ["action"], action_pc)
    reg.regle(r"^(?:comment va|etat du|l'etat du|l etat du) (?:le |mon )?(?:pc|ordi|ordinateur)|^mon pc$",
              lambda m, t: [("etat_pc", {})], priorite=30)
    reg.regle(r"^(?:eteins|coupe) l'?ecran$", lambda m, t: [("action_pc", {"action": "ecran_off"})], priorite=30)
