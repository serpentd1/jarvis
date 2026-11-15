"""Dessine les icônes de Jarvis : jarvis.ico (pour Jarvis.exe) et les PNG de l'extension Chrome."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from core.icone import COULEURS, dessiner  # noqa: E402

RACINE = Path(__file__).resolve().parent.parent

grande = dessiner(COULEURS["repos"], 256)
grande.save(RACINE / "lanceur" / "jarvis.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
for taille in (16, 48, 128):
    dessiner(COULEURS["repos"], taille * 4).resize((taille, taille)).save(RACINE / "extension-chrome" / f"icone-{taille}.png")
print("Icônes créées.")
