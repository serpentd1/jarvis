"""Relais de la barre d'état de Claude Code : garde une copie de ce que Claude Code envoie (modèle, limites
5 h / semaine…) dans %APPDATA%\\Jarvis\\claude_barre.json pour que Jarvis puisse te le dire, puis passe la main
à ta barre d'état habituelle (les arguments suivants), dont il recopie l'affichage tel quel.

Réglage Claude Code (settings.json) : "statusLine": {"type": "command", "command":
  "\"<python.exe>\" -I -S \"<ce fichier>\" \"<ancienne commande>\" <ses arguments>"}
"""
import os
import subprocess
import sys

donnees = sys.stdin.buffer.read()
try:
    dossier = os.path.join(os.environ.get("APPDATA", "."), "Jarvis")
    os.makedirs(dossier, exist_ok=True)
    temporaire = os.path.join(dossier, "claude_barre.json.tmp")
    with open(temporaire, "wb") as f:
        f.write(donnees)
    os.replace(temporaire, os.path.join(dossier, "claude_barre.json"))
except OSError:
    pass
if len(sys.argv) > 1:
    try:
        r = subprocess.run(sys.argv[1:], input=donnees, capture_output=True, timeout=10, creationflags=0x08000000)
        sys.stdout.buffer.write(r.stdout)
    except Exception:  # noqa: BLE001
        pass
