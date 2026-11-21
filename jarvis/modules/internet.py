"""Internet pour l'IA : chercher sur le web, lire une page, lire ou regarder l'onglet ouvert dans Chrome.

Ces outils ne parlent pas eux-mêmes : ils renvoient des données (Resultat.pour_ia, Resultat.images) que le
cerveau redonne à l'IA, qui rédige ensuite la réponse parlée (2e tour, en direct).
"""
import html as html_lib
import json
import logging
import os
import re
import subprocess
import threading
import time
import zipfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import requests

from core import ollama
from . import Resultat

log = logging.getLogger("internet")

MAX_PAGE = 6000          # caractères d'une page donnés à l'IA (assez pour résumer, sans la noyer)
NAVIGATEUR = ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) "
              "Chrome/131.0 Safari/537.36")


def couper(texte: str, n: int = MAX_PAGE) -> str:
    texte = re.sub(r"\n{3,}", "\n\n", (texte or "").strip())
    return texte if len(texte) <= n else texte[:n].rsplit(" ", 1)[0] + " […]"


def chercher(requete: str, n: int = 5) -> list[dict]:
    """Recherche DuckDuckGo (sans clé) : [{"titre", "lien", "extrait"}]."""
    from ddgs import DDGS          # importé ici : le module est lourd et ne sert qu'aux recherches
    res = []
    for region in ("fr-fr", "wt-wt"):  # les moteurs ne répondent pas toujours en français : 2e essai sans région
        try:
            res = DDGS(timeout=8).text(requete, region=region, safesearch="moderate", max_results=n) or []
        except Exception as e:  # noqa: BLE001  (« No results found » est une exception)
            log.info("recherche « %s » (%s) : %s", requete, region, e)
        if res:
            break
    return [{"titre": r.get("title", ""), "lien": r.get("href", ""), "extrait": r.get("body", "")} for r in res]


def texte_de_page(url: str) -> str:
    """Le texte principal d'une page web (sans menus ni pubs), ou "" si on ne peut pas la lire."""
    import trafilatura
    r = requests.get(url, headers={"User-Agent": NAVIGATEUR, "Accept-Language": "fr-FR,fr;q=0.9"}, timeout=10)
    r.raise_for_status()
    return trafilatura.extract(r.text, url=url, include_comments=False, include_tables=True, favor_precision=True) or ""


# ── Obscura : navigateur sans fenêtre pour les agents (pages avec JavaScript), installé à la 1re recherche ──
OBSCURA_ZIP = "https://github.com/h4ckf0r0day/obscura/releases/download/v0.2.7/obscura-x86_64-windows-no-render.zip"
DOSSIER_OBSCURA = Path(os.environ.get("LOCALAPPDATA") or Path.home()) / "Jarvis" / "obscura"
SANS_FENETRE = 0x08000000
_verrou_obscura = threading.Lock()


def obscura() -> Path | None:
    """obscura.exe ; absent : téléchargé une fois (≈ 44 Mo) depuis GitHub. None s'il ne peut pas être installé."""
    with _verrou_obscura:
        exe = next(DOSSIER_OBSCURA.rglob("obscura.exe"), None) if DOSSIER_OBSCURA.exists() else None
        if exe:
            return exe
        try:
            DOSSIER_OBSCURA.mkdir(parents=True, exist_ok=True)
            archive = DOSSIER_OBSCURA / "obscura.zip"
            with requests.get(OBSCURA_ZIP, stream=True, timeout=30) as r:
                r.raise_for_status()
                with open(archive, "wb") as f:
                    for bloc in r.iter_content(1 << 20):
                        f.write(bloc)
            with zipfile.ZipFile(archive) as z:
                if any(n.startswith(("/", "\\")) or ".." in Path(n).parts for n in z.namelist()):
                    raise ValueError("archive suspecte")
                z.extractall(DOSSIER_OBSCURA)
            archive.unlink(missing_ok=True)
            log.info("Obscura installé dans %s", DOSSIER_OBSCURA)
        except Exception as e:  # noqa: BLE001
            log.warning("Obscura : installation impossible (%s)", e)
            return None
        return next(DOSSIER_OBSCURA.rglob("obscura.exe"), None)


def page_obscura(url: str) -> str:
    """Le texte principal d'une page, ouverte par Obscura en arrière-plan (le JavaScript tourne, rien à l'écran)."""
    exe = obscura()
    if not exe:
        return ""
    try:
        r = subprocess.run([str(exe), "fetch", url, "--dump", "html", "--timeout", "20"], capture_output=True,
                           timeout=45, creationflags=SANS_FENETRE, cwd=str(exe.parent))
    except (OSError, subprocess.TimeoutExpired):
        return ""
    import trafilatura
    html = r.stdout.decode("utf-8", "replace")
    return trafilatura.extract(html, url=url, include_comments=False, include_tables=True, favor_precision=True) or ""


def lire_en_fond(url: str) -> str:
    """Une page lue sans bloquer l'écran : une simple requête d'abord, Obscura si la page a besoin de JavaScript."""
    try:
        t = texte_de_page(url)
    except Exception:  # noqa: BLE001
        t = ""
    if len(t) < 400:
        t2 = page_obscura(url)
        t = t2 if len(t2) > len(t) else t
    return t


def chrome_exe() -> str | None:
    for base in (os.environ.get("ProgramFiles"), os.environ.get("ProgramFiles(x86)"), os.environ.get("LOCALAPPDATA")):
        if base and (Path(base) / "Google" / "Chrome" / "Application" / "chrome.exe").is_file():
            return str(Path(base) / "Google" / "Chrome" / "Application" / "chrome.exe")
    return None


def bureau() -> Path:
    accueil = Path(os.environ.get("USERPROFILE") or Path.home())
    for d in (accueil / "Desktop", accueil / "OneDrive" / "Desktop", accueil / "OneDrive" / "Bureau", accueil / "Bureau"):
        if d.is_dir():
            return d
    return accueil


def markdown_vers_html(md: str) -> str:
    """Petit Markdown (titres, listes, gras, liens) → HTML, sans bibliothèque."""
    def en_ligne(t: str) -> str:
        t = html_lib.escape(t)
        t = re.sub(r"\*\*(.+?)\*\*", r"<b>\1</b>", t)
        t = re.sub(r"\[([^\]]+)\]\((https?://[^)\s]+)\)", r'<a href="\2">\1</a>', t)
        return re.sub(r"(?<![\"=>])(https?://[^\s<]+)", r'<a href="\1">\1</a>', t)
    out, liste = [], None
    for ligne in md.splitlines():
        s = ligne.strip()
        m = re.match(r"^(#{1,3})\s+(.*)", s)
        puce = re.match(r"^(?:[-*]|\d+[.)])\s+(.*)", s)
        if liste and not puce:
            out.append(f"</{liste}>")
            liste = None
        if m:
            n = len(m.group(1))
            out.append(f"<h{n}>{en_ligne(m.group(2))}</h{n}>")
        elif puce:
            genre = "ol" if s[0].isdigit() else "ul"
            if liste != genre:
                if liste:
                    out.append(f"</{liste}>")
                out.append(f"<{genre}>")
                liste = genre
            out.append(f"<li>{en_ligne(puce.group(1))}</li>")
        elif s:
            out.append(f"<p>{en_ligne(s)}</p>")
    if liste:
        out.append(f"</{liste}>")
    return "\n".join(out)


def enregistrer_rapport(sujet: str, md: str, sources: list) -> Path:
    if "source" not in md.lower()[-1500:]:
        md += "\n\n## Sources\n" + "\n".join(f"{i}. {t} — {u}" for i, (t, u) in enumerate(sources, 1))
    nom = re.sub(r'[\\/:*?"<>|]+', " ", sujet).strip()[:60] or "recherche"
    chemin = bureau() / f"Recherche - {nom} - {time.strftime('%d-%m-%Y %Hh%M')}.html"
    page = ("<!doctype html><html lang=\"fr\"><head><meta charset=\"utf-8\"><title>Recherche : "
            f"{html_lib.escape(sujet)}</title><style>body{{font:17px/1.6 system-ui,sans-serif;max-width:820px;"
            "margin:40px auto;padding:0 20px;color:#1d1d1f}h1{font-size:30px}h2{margin-top:32px;border-bottom:1px "
            "solid #ddd;padding-bottom:4px}a{color:#0a66c2}@media(prefers-color-scheme:dark){body{background:#1c1c1e;"
            "color:#eee}a{color:#6cb4ff}h2{border-color:#444}}</style></head><body>"
            f"{markdown_vers_html(md)}<p style=\"color:#888;margin-top:40px\">Rapport écrit par Jarvis le "
            f"{time.strftime('%d/%m/%Y à %H:%M')}.</p></body></html>")
    chemin.write_text(page, encoding="utf-8")
    return chemin


CONSIGNE_REQUETES = ("Tu prépares une recherche sur Internet. Donne 3 recherches courtes et différentes (des mots-clés, "
                     "en français, ou en anglais si c'est mieux) pour trouver ce qui MANQUE encore. Réponds en JSON : "
                     "{\"recherches\": [\"...\", \"...\", \"...\"]}.")
CONSIGNE_NOTES = ("Tu lis une page web pour une recherche. Garde seulement les faits utiles sur le sujet (chiffres, dates, "
                  "noms, explications), en français, 8 lignes au plus. Si la page ne dit rien d'utile, réponds RIEN.")
CONSIGNE_ASSEZ = ("Tu juges une recherche. Avec ces notes, peut-on écrire un rapport complet et fiable sur le sujet ? "
                  "Réponds en JSON : {\"assez\": true} ou {\"assez\": false}.")
CONSIGNE_RAPPORT = ("Tu écris un rapport de recherche en français pour un collégien : clair, précis, bien organisé. "
                    "Format Markdown : un titre (#), un résumé de 3 phrases, des parties (##) avec les faits importants "
                    "(listes à puces si utile), puis une partie « ## Sources » qui liste les sources numérotées avec leur "
                    "adresse. Cite les sources dans le texte avec [1], [2]… N'invente rien qui ne soit pas dans les notes.")
INUTILES = re.compile(r"youtube\.com|youtu\.be|tiktok\.com|instagram\.com|facebook\.com|twitter\.com|x\.com/", re.I)


def installer(ctx, reg):
    # ── Recherche approfondie : un agent qui cherche, lit, recommence tant qu'il manque des infos, puis écrit un
    # rapport sur le Bureau et l'ouvre dans Chrome. Tout se passe en arrière-plan : l'écran reste libre.
    def ia(consigne: str, texte: str, mots: int = 500, js: bool = False) -> str:
        return ollama.discuter(ctx.cfg["cerveau"], [{"role": "system", "content": consigne},
                                                    {"role": "user", "content": texte}],
                               temperature=0.2, max_mots=mots, json_seulement=js) or ""

    def ouvrir_dans_chrome(cible: str):
        """Le résultat s'ouvre TOUJOURS dans Chrome : par l'extension, sinon chrome.exe, sinon le navigateur par défaut."""
        url = cible if cible.startswith("http") else Path(cible).resolve().as_uri()
        pont = getattr(ctx, "pont", None)
        try:
            if pont is not None and getattr(pont, "connecte", False) and (pont.commande({"commande": "ouvrir", "url": url}, 5) or {}).get("ok"):
                return
        except Exception:  # noqa: BLE001
            pass
        exe = chrome_exe()
        if exe:
            subprocess.Popen([exe, url], creationflags=SANS_FENETRE)
        else:
            os.startfile(url)

    def annoncer(phrase: str):
        etat = getattr(ctx, "etat", None)
        if etat is not None and hasattr(etat, "annoncer"):
            etat.annoncer(phrase)

    def chercher_en_fond(sujet: str):
        vus, notes, sources = set(), [], []
        for tour in range(4):
            r = ia(CONSIGNE_REQUETES, f"Sujet : {sujet}\nDéjà trouvé :\n" + ("\n".join(n[:200] for n in notes[-8:]) or "rien"),
                   120, True)
            try:
                requetes = [q for q in json.loads(r).get("recherches", []) if isinstance(q, str) and q.strip()][:3]
            except (ValueError, AttributeError):
                requetes = []
            requetes = requetes or ([sujet] if tour == 0 else [])
            liens = []
            for q in requetes:
                for x in chercher(q, 6):
                    if x["lien"] and x["lien"] not in vus and not INUTILES.search(x["lien"]):
                        vus.add(x["lien"])
                        liens.append(x)
            liens = liens[:4]
            if not liens:
                break
            with ThreadPoolExecutor(4) as ex:
                textes = list(ex.map(lambda x: lire_en_fond(x["lien"]), liens))
            for x, t in zip(liens, textes):
                if len(t) < 200:
                    continue
                note = ia(CONSIGNE_NOTES, f"Sujet : {sujet}\nPage : {x['titre']}\n\n{couper(t, 7000)}", 350).strip()
                if note and not note.upper().startswith("RIEN"):
                    sources.append((x["titre"], x["lien"]))
                    notes.append(f"[{len(sources)}] {note}")
            log.info("recherche approfondie « %s » : tour %d, %d sources", sujet, tour + 1, len(sources))
            if len(sources) >= 12:
                break
            if len(sources) >= 4:
                try:
                    if json.loads(ia(CONSIGNE_ASSEZ, f"Sujet : {sujet}\nNotes :\n" + "\n".join(notes), 30, True)).get("assez"):
                        break
                except (ValueError, AttributeError):
                    pass
        if not notes:
            annoncer(f"Je n'ai rien trouvé de solide sur {sujet}, désolé.")
            return
        md = ia(CONSIGNE_RAPPORT, f"Sujet : {sujet}\n\nNotes :\n" + "\n\n".join(notes) + "\n\nSources :\n"
                + "\n".join(f"[{i}] {t} — {u}" for i, (t, u) in enumerate(sources, 1)), 1800)
        if not md.strip():
            md = f"# {sujet}\n\n" + "\n\n".join(notes)
        chemin = enregistrer_rapport(sujet, md, sources)
        log.info("recherche approfondie « %s » : rapport %s", sujet, chemin)
        ouvrir_dans_chrome(str(chemin))
        annoncer(f"Ta recherche sur {sujet} est finie : {len(sources)} sources. Je t'ouvre le rapport dans Chrome, "
                 "il est aussi sur ton Bureau.")

    def recherche_approfondie(sujet: str):
        sujet = str(sujet).strip(" .?!")
        if not sujet:
            return Resultat("Sur quoi tu veux que je fasse la recherche ?", ok=False)

        def fil():
            try:
                chercher_en_fond(sujet)
            except Exception:  # noqa: BLE001
                log.exception("recherche approfondie")
                annoncer(f"Ma recherche sur {sujet} a planté, désolé. Tu peux la relancer.")
        threading.Thread(target=fil, daemon=True).start()
        return Resultat(f"Je lance une recherche approfondie sur {sujet}. Je lis plusieurs sites en arrière-plan : "
                        "je te préviens quand le rapport est prêt, sur ton Bureau.")

    reg.outil("recherche_approfondie", "Recherche approfondie (agent) : cherche sur plusieurs sites en arrière-plan, "
              "continue tant qu'il manque des infos, écrit un rapport sur le Bureau et l'ouvre dans Chrome. Pour « fais une "
              "recherche approfondie sur… », « fais-moi un rapport sur… », « renseigne-toi à fond sur… ».",
              {"sujet": "le sujet de la recherche, en entier"}, ["sujet"], recherche_approfondie)
    reg.regle(r"^(?:fais|fait|lance|faire)(?: moi)? (?:une |un )?(?:recherche approfondie|grosse recherche|recherche "
              r"complete|rapport(?: de recherche)?|dossier) (?:sur|a propos de|a propos du|de|du|des) (?P<s>.+)$",
              lambda m, t: [("recherche_approfondie", {"sujet": re.split(r"(?i)\b(?:sur|à propos d[eu]|a propos d[eu])\s+", t, maxsplit=1)[-1]})],
              priorite=25)

    def web_search(query: str):
        try:
            res = chercher(query)
        except Exception as e:  # noqa: BLE001
            log.warning("recherche « %s » : %s", query, e)
            return Resultat("Je n'arrive pas à chercher sur Internet pour l'instant.", ok=False)
        log.info("recherche « %s » : %d résultats", query, len(res))
        if not res:
            return Resultat(f"Je n'ai rien trouvé sur Internet pour « {query} ».", ok=False)
        lignes = [f"{i}. {r['titre']} — {r['extrait']} ({r['lien']})" for i, r in enumerate(res, 1)]
        # la recherche finie, le 1er résultat s'ouvre TOUJOURS dans Chrome (après le début de la réponse parlée)
        premier = next((r["lien"] for r in res if r["lien"].startswith("http")), "")
        if premier:
            threading.Timer(2.0, lambda: ouvrir_dans_chrome(premier)).start()
        return Resultat(f"D'après Internet : {res[0]['extrait']}",
                        pour_ia=f"Résultats de la recherche « {query} » :\n" + "\n".join(lignes))

    def read_page(url: str):
        url = url.strip()
        if not re.match(r"^https?://", url):
            url = "https://" + url
        try:
            texte = texte_de_page(url)
        except Exception as e:  # noqa: BLE001
            log.warning("page %s : %s", url, e)
            return Resultat("Je n'arrive pas à ouvrir cette page.", ok=False)
        if not texte.strip():
            return Resultat("Cette page n'a pas de texte que je puisse lire.", ok=False)
        return Resultat("J'ai lu la page.", pour_ia=f"Texte de la page {url} :\n{couper(texte)}")

    # ── l'onglet ouvert dans Chrome (par l'extension de Jarvis) ──
    def onglet(commande: str, delai: float = 10) -> dict:
        pont = getattr(ctx, "pont", None)
        if pont is None or not pont.connecte:
            return {"ok": False, "erreur": "extension"}
        return pont.commande({"commande": commande}, delai)

    def echec(r: dict) -> Resultat:
        erreur = str(r.get("erreur", ""))
        if erreur == "extension" or "pas connectée" in erreur:
            return Resultat("L'extension Jarvis n'est pas branchée dans Chrome : je ne vois pas ton onglet.", ok=False)
        if "activeTab" in erreur or "Cannot access" in erreur or "chrome://" in erreur:
            return Resultat("Cet onglet est une page spéciale de Chrome (nouvel onglet, réglages, extension) : "
                            "je ne peux regarder que les sites web.", ok=False)
        if "commande inconnue" in erreur:
            return Resultat("Recharge l'extension Jarvis dans Chrome (page chrome://extensions, bouton ↻) : "
                            "elle ne sait pas encore lire tes onglets.", ok=False)
        log.warning("onglet Chrome : %s", erreur)
        return Resultat("Je n'arrive pas à lire l'onglet ouvert dans Chrome.", ok=False)

    def lire_onglet():
        r = onglet("onglet_texte")
        if not r.get("ok"):
            return echec(r)
        titre, url, texte = r.get("titre", ""), r.get("url", ""), r.get("texte", "")
        log.info("onglet lu : %s (%d caractères)", url, len(texte))
        if not re.match(r"^(https?|file):", url):
            return Resultat(f"L'onglet « {titre} » est une page spéciale de Chrome (nouvel onglet, réglages, "
                            "extension) : je ne peux lire que les sites web.", ok=False)
        if len(texte.strip()) < 200 and url.startswith("http"):
            try:                       # page vide pour l'extension (PDF, page protégée) : on la lit nous-mêmes
                texte = texte_de_page(url) or texte
            except Exception:  # noqa: BLE001
                pass
        if not texte.strip():
            return Resultat(f"L'onglet « {titre} » n'a pas de texte que je puisse lire.", ok=False)
        return Resultat(f"J'ai lu l'onglet {titre}.",
                        pour_ia=f"Onglet ouvert dans Chrome : « {titre} » ({url}). Son texte :\n{couper(texte)}")

    def capture_onglet():
        r = onglet("onglet_capture", 15)
        if not r.get("ok") or not r.get("image"):
            return echec(r)
        image = re.sub(r"^data:image/\w+;base64,", "", r["image"])
        return Resultat("J'ai regardé ton onglet.", images=[image],
                        pour_ia=f"Capture de l'onglet ouvert dans Chrome : « {r.get('titre', '')} » "
                                f"({r.get('url', '')}). L'image est jointe juste après.")

    reg.outil("web_search", "Cherche sur Internet (DuckDuckGo) et te donne titres, liens et extraits. À utiliser pour "
              "l'actualité, les résultats sportifs, les prix, les sorties, et tout ce qui a pu changer récemment ou "
              "que tu ne sais pas sûrement.",
              {"query": "ce qu'il faut chercher, en mots-clés précis (ex. vainqueur coupe du monde rugby 2023)"},
              ["query"], web_search)
    reg.outil("read_page", "Lit le texte d'une page web à partir de son adresse (ex. un lien trouvé par web_search).",
              {"url": "adresse de la page"}, ["url"], read_page)
    reg.outil("lire_onglet", "Lit le texte de la page ouverte dans Chrome (l'onglet affiché), pour la résumer ou "
              "répondre à une question dessus.", {}, [], lire_onglet)
    reg.outil("capture_onglet", "Fait une capture d'écran de l'onglet affiché dans Chrome et te la montre : pour "
              "décrire une image, un graphique, une vidéo ou la mise en page.", {}, [], capture_onglet)
