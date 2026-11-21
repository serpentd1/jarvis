"""« Trouve mon exposé d'histoire » : cherche un fichier par son nom, SEULEMENT dans les dossiers autorisés
([fichiers] dossiers de config.toml : bureau, documents, téléchargements… par défaut), puis propose de l'ouvrir."""
import json
import logging
import os
import re
import time
from pathlib import Path

from core import ollama
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


def nettoyer_plan(plan, sujet: str, n_max: int = 12) -> dict:
    """Plan donné par l'IA -> plan propre : {"titre", "diapos": [{"titre", "points": [...]}]}. Lève ValueError si vide."""
    if not isinstance(plan, dict):
        raise ValueError("plan invalide")
    diapos = []
    for d in plan.get("diapos") or []:
        if not isinstance(d, dict):
            continue
        titre = str(d.get("titre") or "").strip()
        points = [str(x).strip() for x in (d.get("points") or []) if str(x).strip()][:6]
        if titre or points:
            diapos.append({"titre": titre or "…", "points": points})
    if not diapos:
        raise ValueError("plan vide")
    return {"titre": str(plan.get("titre") or sujet).strip() or sujet, "diapos": diapos[:n_max]}


def construire_pptx(plan: dict, chemin) -> Path:
    """Écrit le fichier PowerPoint (16:9) : une diapo de titre, puis une diapo par partie.
    Lève ImportError si python-pptx manque."""
    from pptx import Presentation
    from pptx.dml.color import RGBColor
    from pptx.util import Inches, Pt

    prez = Presentation()
    prez.slide_width, prez.slide_height = Inches(13.333), Inches(7.5)
    bleu = RGBColor(0x1F, 0x3A, 0x5F)

    def regler(paragraphe, taille, couleur=None, gras=False):
        for r in paragraphe.runs:
            r.font.name, r.font.size, r.font.bold = "Calibri", Pt(taille), gras
            if couleur is not None:
                r.font.color.rgb = couleur

    # Diapo de titre : fond bleu, gros titre blanc
    d = prez.slides.add_slide(prez.slide_layouts[6])
    d.background.fill.solid()
    d.background.fill.fore_color.rgb = bleu
    zone = d.shapes.add_textbox(Inches(1), Inches(2.4), Inches(11.3), Inches(2.6)).text_frame
    zone.word_wrap = True
    zone.text = plan["titre"]
    regler(zone.paragraphs[0], 48, RGBColor(255, 255, 255), True)

    # Une diapo par partie
    for part in plan["diapos"]:
        d = prez.slides.add_slide(prez.slide_layouts[6])
        t = d.shapes.add_textbox(Inches(0.8), Inches(0.5), Inches(11.7), Inches(1.2)).text_frame
        t.word_wrap = True
        t.text = part["titre"]
        regler(t.paragraphs[0], 36, bleu, True)
        corps = d.shapes.add_textbox(Inches(0.9), Inches(1.9), Inches(11.5), Inches(5)).text_frame
        corps.word_wrap = True
        for i, point in enumerate(part["points"]):
            para = corps.paragraphs[0] if i == 0 else corps.add_paragraph()
            para.text = "• " + point
            para.space_after = Pt(14)
            regler(para, 26)
    chemin = Path(chemin)
    prez.save(str(chemin))
    return chemin


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

    def creer_powerpoint(sujet: str, diapos: int = 6):
        """L'IA écrit le plan, puis un vrai .pptx est créé sur le Bureau et ouvert."""
        from .internet import bureau
        sujet = str(sujet or "").strip()
        if not sujet:
            return Resultat("Sur quel sujet je fais le PowerPoint ?", ok=False)
        try:
            n = max(3, min(12, int(diapos or 6)))
        except (TypeError, ValueError):
            n = 6
        try:
            import pptx  # noqa: F401
        except ImportError:
            return Resultat("Il me manque la bibliothèque python-pptx pour faire un PowerPoint. "
                            "Il faut l'installer dans l'environnement de Jarvis.", ok=False)
        brut = ollama.discuter(ctx.cfg["cerveau"], [
            {"role": "system", "content": "Tu écris le plan d'un diaporama pour un collégien, en français. Réponds "
             'UNIQUEMENT par du JSON : {"titre": "...", "diapos": [{"titre": "...", "points": ["...", "..."]}]}. '
             f"Exactement {n} diapos, sans compter le titre. Chaque diapo a 3 à 5 points : des phrases courtes et simples."},
            {"role": "user", "content": f"Sujet : {sujet}"}], temperature=0.4, max_mots=900, json_seulement=True)
        try:
            plan = nettoyer_plan(json.loads(brut or "{}"), sujet, n)
        except (ValueError, TypeError):
            return Resultat("Je n'ai pas réussi à écrire le plan, je n'ai pas eu une bonne réponse. Réessaie.", ok=False)
        nom = re.sub(r'[\/:*?"<>|]', " ", sujet).strip()[:60] or "diaporama"
        try:
            chemin = construire_pptx(plan, bureau() / f"Présentation - {nom}.pptx")
        except Exception as e:  # noqa: BLE001
            log.warning("powerpoint : %s", e)
            return Resultat("Je n'ai pas pu créer le fichier PowerPoint.", ok=False)
        try:
            os.startfile(str(chemin))
        except OSError:
            pass
        return Resultat(f"C'est fait : j'ai créé « {chemin.name} » sur ton Bureau, avec {len(plan['diapos'])} diapos. "
                        "Je l'ouvre.", pour_ia=f"PowerPoint créé : {chemin}")

    reg.outil("creer_powerpoint", "Crée un vrai PowerPoint (.pptx) sur le Bureau à partir d'un sujet, puis l'ouvre.",
              {"sujet": "le sujet du diaporama",
               "diapos": {"type": "integer", "description": "nombre de diapos (3 à 12, 6 par défaut)"}},
              ["sujet"], creer_powerpoint)

    def regle_powerpoint(m, _t):
        args = {"sujet": m["s"].strip()}
        n = m["n"] or m["n2"]
        if n:
            args["diapos"] = int(n)
        return [("creer_powerpoint", args)]

    reg.regle(r"^(?:peux tu |tu peux )?(?:me )?(?:cree|creer|fais|faire|genere|generer|prepare|fabrique)(?: moi)? "
              r"(?:un |une |le )?(?:power ?point|diaporama|presentation|pptx)(?: de (?P<n>\d+) (?:diapos|diapositives|slides))?"
              r" (?:sur|a propos de|concernant|de|d')\s?(?P<s>.+?)"
              r"(?: ?\(?(?:en )?(?P<n2>\d+) (?:diapos|diapositives|slides)\)?)?$", regle_powerpoint, priorite=60)
