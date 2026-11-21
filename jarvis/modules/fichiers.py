"""« Trouve mon exposé d'histoire » : cherche un fichier par son nom, SEULEMENT dans les dossiers autorisés
([fichiers] dossiers de config.toml : bureau, documents, téléchargements… par défaut), puis propose de l'ouvrir."""
import logging
import os
import re
import time
from pathlib import Path

from . import Resultat, normaliser

log = logging.getLogger("fichiers")

IGNORES = {"node_modules", "appdata", "$recycle.bin", "__pycache__", "venv", ".venv", "target", "site-packages",
           "windows", "program files", "program files (x86)"}
MOTS_VIDES = {"mon", "ma", "mes", "le", "la", "les", "l", "un", "une", "de", "du", "des", "d", "fichier", "fichiers",
              "dossier", "sur", "dans", "pc", "ordi", "ordinateur", "en", "a", "au", "aux", "et", "qui", "est",
              "que", "j'ai", "fait", "fais"}
# mots dits -> extensions
TYPES = {"pdf": {".pdf"}, "powerpoint": {".pptx", ".ppt", ".odp"}, "diaporama": {".pptx", ".ppt", ".odp"},
         "presentation": {".pptx", ".ppt", ".odp"}, "word": {".docx", ".doc", ".odt"},
         "photo": {".jpg", ".jpeg", ".png", ".heic", ".webp"}, "image": {".jpg", ".jpeg", ".png", ".heic", ".webp"},
         "video": {".mp4", ".mov", ".mkv", ".avi"}, "musique": {".mp3", ".wav", ".flac", ".m4a"},
         "excel": {".xlsx", ".xls", ".ods", ".csv"}, "tableau": {".xlsx", ".xls", ".ods", ".csv"}}
CONNUS = {"bureau": ["Desktop", "Bureau"], "documents": ["Documents"], "telechargements": ["Downloads"],
          "images": ["Pictures", "Images"], "videos": ["Videos"], "musique": ["Music"]}


def dossiers_autorises(cfg: dict) -> list[Path]:
    """Les dossiers où Jarvis a le droit de chercher (noms connus ou chemins complets)."""
    maison = Path.home()
    res = []
    for d in cfg.get("dossiers") or []:
        cle = normaliser(str(d)).replace(" ", "")
        if cle in CONNUS:
            for nom in CONNUS[cle]:
                for base in (maison, maison / "OneDrive"):
                    p = base / nom
                    if p.is_dir():
                        res.append(p)
        else:
            p = Path(os.path.expandvars(os.path.expanduser(str(d))))
            if p.is_dir():
                res.append(p)
    vus, uniques = set(), []
    for p in res:
        k = str(p.resolve()).lower()
        if k not in vus:
            vus.add(k)
            uniques.append(p)
    return uniques


def mots_de(recherche: str) -> tuple[list[str], set]:
    mots, exts = [], set()
    for m in normaliser(recherche).replace("'", " ").split():
        if m in TYPES:
            exts |= TYPES[m]
        elif m not in MOTS_VIDES and len(m) > 1:
            mots.append(m)
    return mots, exts


def score(chemin: Path, racine: Path, mots: list[str], exts: set) -> float:
    if exts and chemin.suffix.lower() not in exts:
        return 0
    nom = normaliser(chemin.stem.replace("_", " ").replace("-", " ")).replace(" ", "")
    dossier = normaliser(str(chemin.parent.relative_to(racine)).replace("\\", " ")).replace(" ", "")
    s = 0.0
    for m in mots:
        racine_mot = m[:-1] if len(m) > 4 and m.endswith("s") else m      # « exposés » ~ « exposé »
        if racine_mot in nom:
            s += 2
        elif racine_mot in dossier:
            s += 1
    if mots and s < len(mots):            # il faut que la plupart des mots soient là
        return 0
    return s or (1 if exts and not mots else 0)


def chercher(recherche: str, dossiers: list[Path], profondeur: int = 6, maxi: int = 30000) -> list[Path]:
    mots, exts = mots_de(recherche)
    if not mots and not exts:
        return []
    trouves, vus = [], 0
    for racine in dossiers:
        base = len(racine.parts)
        for dossier, sous, fichiers in os.walk(racine):
            sous[:] = [d for d in sous if not d.startswith(".") and d.lower() not in IGNORES
                       and len(Path(dossier, d).parts) - base < profondeur]
            for f in fichiers:
                vus += 1
                if vus > maxi:
                    break
                if f.startswith(("~$", ".")):
                    continue
                p = Path(dossier, f)
                s = score(p, racine, mots, exts)
                if s:
                    try:
                        trouves.append((s, p.stat().st_mtime, p))
                    except OSError:
                        pass
    trouves.sort(key=lambda x: (-x[0], -x[1]))
    return [p for _, _, p in trouves[:5]]


def lieu(p: Path) -> str:
    maison = Path.home()
    try:
        rel = p.parent.relative_to(maison)
    except ValueError:
        return str(p.parent)
    noms = {"Desktop": "le Bureau", "Documents": "Documents", "Downloads": "Téléchargements", "Pictures": "Images",
            "OneDrive": "OneDrive"}
    parts = [noms.get(x, x) for x in rel.parts]
    return " > ".join(parts) or "ton dossier"


def installer(ctx, reg):
    cfg = ctx.cfg.get("fichiers", {}) if isinstance(getattr(ctx, "cfg", None), dict) else {}

    def trouver_fichier(recherche: str, ouvrir: bool = False):
        dossiers = dossiers_autorises(cfg)
        if not dossiers:
            return Resultat("Je n'ai aucun dossier où chercher : ajoute-en dans config.toml, section fichiers.", ok=False)
        debut = time.time()
        res = chercher(recherche, dossiers, int(cfg.get("profondeur", 6)))
        log.info("fichiers « %s » : %d trouvés en %.1f s", recherche, len(res), time.time() - debut)
        if not res:
            return Resultat(f"Je n'ai trouvé aucun fichier pour « {recherche} » dans tes dossiers autorisés.", ok=False)
        p = res[0]

        def ouvrir_le():
            os.startfile(str(p))
            return Resultat(f"J'ouvre {p.name}.")
        if ouvrir:
            return ouvrir_le()
        autres = f" J'en ai aussi trouvé {len(res) - 1} autre{'s' if len(res) > 2 else ''}." if len(res) > 1 else ""
        return Resultat(f"J'ai trouvé « {p.name} » dans {lieu(p)}.{autres} Je l'ouvre ?", confirmer=ouvrir_le,
                        annuler="D'accord, je ne l'ouvre pas.",
                        pour_ia="Fichiers trouvés : " + " ; ".join(str(x) for x in res))

    reg.outil("trouver_fichier", "Cherche un fichier de l'utilisateur par son nom (exposé, devoir, photo, PDF…), "
              "seulement dans ses dossiers autorisés (bureau, documents, téléchargements…), et propose de l'ouvrir.",
              {"recherche": "mots du nom du fichier (ex : exposé histoire, ou pdf maths)",
               "ouvrir": {"type": "boolean", "description": "vrai = l'ouvrir tout de suite sans demander"}},
              ["recherche"], trouver_fichier)

    def regle(ouvrir):
        def f(m, _t):
            q = m["q"].strip()
            if re.search(r"\b(?:telephone|portable|tel|souris|cle|lunettes|chargeur|casque)\b", q):
                return None
            return [("trouver_fichier", {"recherche": q, "ouvrir": ouvrir})]
        return f

    fin = r"(?: sur (?:mon |le )?(?:pc|ordi)| dans mes (?:fichiers|documents|dossiers))?$"
    reg.regle(r"^(?:trouve|retrouve|cherche|ou est|ou sont|ou j'ai mis)(?: moi)? (?:mon |ma |mes |le |la |les |l')?"
              r"(?P<q>(?:fichier|document|photo|pdf|devoir|expose|powerpoint|diaporama|presentation|word|redaction|"
              r"cv|image|video|dossier)s?\b.*?)" + fin, regle(False), priorite=70)
    reg.regle(r"^(?:trouve|retrouve|cherche)(?: moi)? (?:le fichier|le document|les fichiers) (?P<q>.+?)" + fin,
              regle(False), priorite=70)
    reg.regle(r"^(?:ouvre|retrouve et ouvre|trouve et ouvre)(?: moi)? (?:mon |ma |mes |le |la |l')?"
              r"(?P<q>(?:fichier|document|pdf|devoir|expose|powerpoint|diaporama|presentation|redaction|cv)s?\b.*?)" + fin,
              regle(True), priorite=70)
