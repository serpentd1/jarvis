"""« Hey Jarvis, qu'est-ce que Claude fait ? » : résume ce que Claude Code est en train de faire sur ce PC
(d'après sa conversation, enregistrée en local) et, si le relais de barre d'état est branché, tes limites
d'utilisation (5 heures et semaine). Tout est lu et résumé sur ton PC, par l'IA locale."""
import json
import os
import time
from datetime import datetime
from pathlib import Path

from core import ollama
from core.config import DONNEES
from . import Resultat

PROJETS = Path(os.environ.get("USERPROFILE", "~")) / ".claude" / "projects"
BARRE = DONNEES / "claude_barre.json"


def derniere_conversation():
    fichiers = [f for f in PROJETS.glob("*/*.jsonl")] if PROJETS.is_dir() else []
    return max(fichiers, key=lambda f: f.stat().st_mtime, default=None)


def evenements(fichier: Path, octets: int = 400_000) -> list:
    """Les derniers messages et actions de la conversation (seulement la fin du fichier)."""
    with open(fichier, "rb") as f:
        f.seek(max(0, f.seek(0, 2) - octets))
        lignes = f.read().decode("utf-8", "replace").splitlines()[1:]
    sortie = []
    for ligne in lignes:
        try:
            e = json.loads(ligne)
        except ValueError:
            continue
        message = e.get("message") or {}
        contenu = message.get("content")
        if e.get("type") == "user" and isinstance(contenu, str) and not contenu.startswith("<"):
            sortie.append(f"L'UTILISATEUR DEMANDE : {contenu[:300]}")
        elif e.get("type") == "assistant" and isinstance(contenu, list):
            for c in contenu:
                if c.get("type") == "text" and c.get("text", "").strip():
                    sortie.append(f"CLAUDE DIT : {c['text'].strip()[:300]}")
                elif c.get("type") == "tool_use":
                    entree = c.get("input") or {}
                    quoi = entree.get("description") or entree.get("file_path") or entree.get("summary") or ""
                    sortie.append(f"CLAUDE UTILISE {c.get('name')} : {str(quoi)[:160]}")
    return sortie[-25:]


def limites() -> str:
    """Les limites 5 h / semaine, d'après la dernière copie de la barre d'état (si le relais est branché)."""
    try:
        if time.time() - BARRE.stat().st_mtime > 3 * 3600:
            return ""
        d = json.loads(BARRE.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return ""
    trouves = []

    def chercher(x, chemin=""):
        if isinstance(x, dict):
            for k, v in x.items():
                chercher(v, f"{chemin}.{k}" if chemin else k)
        elif isinstance(x, (int, float, str)) and any(m in chemin.lower() for m in ("rate", "limit", "five", "seven", "week", "hour")):
            trouves.append(f"{chemin} = {x}")
    chercher(d)
    return "; ".join(trouves[:12])


def installer(ctx, reg):
    def activite_claude():
        fichier = derniere_conversation()
        if not fichier:
            return Resultat("Je ne trouve aucune conversation de Claude Code sur ce PC.", ok=False)
        age = time.time() - fichier.stat().st_mtime
        etat = "Il travaille en ce moment." if age < 90 else f"Il ne fait plus rien depuis {int(age // 60)} minutes."
        infos_limites = limites()
        resume = ollama.discuter(ctx.cfg["cerveau"], [
            {"role": "system", "content": "Tu es Jarvis. On te donne la fin du journal de Claude, l'assistant de "
             "programmation de l'utilisateur. Dis en 2 à 3 phrases courtes, en français, en tutoyant l'utilisateur, ce "
             "que Claude est en train de faire et ce qu'il vient de faire (lu à voix haute : pas de liste, pas de "
             "noms de fichiers compliqués). Si on te donne des limites d'utilisation, termine par une phrase claire "
             "sur ce qui est utilisé en pourcentage et quand ça se remet à zéro."},
            {"role": "user", "content": f"Maintenant : {datetime.now():%H:%M}. {etat}\n\n" + "\n".join(evenements(fichier))
             + (f"\n\nLIMITES D'UTILISATION (barre d'état de Claude Code) : {infos_limites}" if infos_limites else "")},
        ], max_mots=220)
        if not resume:
            return Resultat(etat)
        if not infos_limites:
            resume += " Pour tes limites, il faut d'abord brancher mon relais sur la barre d'état de Claude Code."
        return Resultat(resume)

    reg.outil("activite_claude", "Dit ce que Claude (l'assistant de programmation, Claude Code) est en train de "
              "faire sur le PC, et les limites d'utilisation (5 heures, semaine).", {}, [], activite_claude)
    reg.regle(r"^(?!(?:envoie|ecris|reponds?|dis a)\b)(?:.*\bclaude\b.*\b(fait|fais|travaille|en train|occupe|limite|"
              r"limites|quota|avance|ou en est)\b|.*\b(fait|fais|travaille|limite|limites|quota)\b.*\bclaude\b)",
              lambda m, t: [("activite_claude", {})], priorite=11)
