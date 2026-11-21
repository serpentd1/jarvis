"""Mémoire à long terme en GRAPHE : chaque chose retenue est un nœud (toi, tes potes, tes projets, tes goûts…)
relié aux autres par des liens (« aime », « est en classe avec », « prépare »…).

- Après chaque échange, l'IA locale repère ce qui vaut la peine d'être retenu et l'ajoute au graphe.
- « Souviens-toi que… », « qu'est-ce que tu sais sur moi ? », « oublie… ».
- Avant de répondre, Jarvis relit les nœuds liés à ta question.
- La grande application affiche ce graphe en carte mentale interactive.
- La mémoire est un COFFRE OBSIDIAN : une note .md par souvenir (Documents\\Jarvis\\Mémoire), rangée dans le
  dossier de son type (Personnes, Goûts, École…), avec ses liens « - aime [[Le foot]] ». Ouvre ce dossier avec
  Obsidian pour voir le graphe ; ce que tu y modifies, Jarvis le relit.
- Elle se range toute seule (doublons, dates, bouts de phrases) ; « range » la réorganise entièrement avec l'IA.
Tout reste sur ton PC. Jamais de mots de passe ni de codes.
"""
import json
import logging
import os
import re
import threading
import time
from pathlib import Path

from core import ollama
from . import Resultat, normaliser

# Souvenirs qui parlent de notes scolaires (cachés quand l'utilisateur masque ses notes dans l'île).
NOTES = re.compile(r"moyenne|bulletin|/\s?20\b|sur 20|\bune note\b|\bmes notes\b|\bses notes\b|\bnote de\b")

log = logging.getLogger("memoire")
TYPES = ["moi", "personne", "lieu", "ecole", "projet", "preference", "habitude", "activite", "objet", "evenement", "autre"]
DONNEES_SENSIBLES = re.compile(
    r"\b(?:mot de passe|password|passphrase|code pin|code de verification|code de securite|code sms|"
    r"cles? api|api key|access token|token|jeton d acces|bearer)\b",
    re.IGNORECASE,
)
JETON_SECRET = re.compile(
    r"\b(?:gh[pousr]_[a-z0-9_]{20,}|github_pat_[a-z0-9_]{20,}|glpat-[a-z0-9_-]{20,})\b",
    re.IGNORECASE,
)
# « retiens ÇA », « c'est CE lien » : un renvoi à la conversation, que seule l'IA peut comprendre (texte normalisé)
RENVOI = re.compile(r"\b(?:ca|cela|celui ci|celui la|celle ci|celle la|ce lien|cette adresse|cette page|ce site|"
                    r"cet onglet|cette commande|ce truc|ce que|ce qu'|comme ca|la meme chose|la dessus|ici)\b")
# Le souvenir parle d'une page sans en donner l'adresse : on prend celle de l'onglet ouvert (texte normalisé)
PARLE_D_UNE_PAGE = re.compile(r"\b(?:ce|cet|cette|le|la|l')\s?(?:lien|page|site|onglet|adresse|url)\b")
# Une adresse web ou une commande : à garder mot pour mot
ADRESSE_OU_COMMANDE = re.compile(r"https?://|www\.|\b[\w-]+\.(?:fr|com|net|org|io|be|ch|eu)\b|start-process|"
                                 r"\.exe\b|powershell", re.IGNORECASE)
MOTS_VIDES = {"le", "la", "les", "un", "une", "des", "de", "du", "d", "l", "et", "a", "au", "aux", "en", "mon", "ma",
              "mes", "ton", "ta", "tes", "son", "sa", "ses", "je", "tu", "il", "elle", "on", "que", "qui", "quoi",
              "est", "ce", "ca", "pour", "avec", "sur", "dans", "pas", "ne", "plus", "moi", "toi"}


def contient_secret(*textes: str) -> bool:
    brut = " ".join(textes)
    return bool(DONNEES_SENSIBLES.search(normaliser(brut)) or JETON_SECRET.search(brut))
CONSIGNE = (
    "Tu construis la mémoire à long terme de l'assistant d'un collégien. À partir de l'échange ci-dessous, extrais "
    "UNIQUEMENT les informations DURABLES sur l'utilisateur : identité, goûts et préférences, personnes de son "
    "entourage (amis, famille, profs), école et matières, projets, habitudes, activités, objets à lui, lieux, "
    "événements prévus. Ignore les questions ponctuelles (météo, heure, calculs, commandes), les infos venant "
    "seulement de l'assistant, les dates ou heures seules, les bouts de phrases, et JAMAIS de mots de passe, codes "
    "ou numéros secrets. Dans le doute, n'ajoute rien. Trois faits au plus, chacun compréhensible seul. N'invente rien.\n"
    "Réponds en JSON : {\"faits\": [{\"sujet\": \"Moi\", \"relation\": \"aime\", \"objet\": \"la musique électro\", "
    "\"type_objet\": \"preference\"}]}. sujet = « Moi » pour l'utilisateur. type_objet parmi : "
    + ", ".join(TYPES[1:]) + ". Phrases courtes. {\"faits\": []} s'il n'y a rien à retenir."
)


def cle(nom: str) -> str:
    return normaliser(nom).replace(" ", "_")[:60] or "?"


# ── Tri de la mémoire : repérer le bruit et les doublons ────────────────────
_MOIS = "janvier|fevrier|mars|avril|mai|juin|juillet|aout|septembre|octobre|novembre|decembre"
_JOURS = "lundi|mardi|mercredi|jeudi|vendredi|samedi|dimanche"
BRUIT = [
    ("date seule", re.compile(
        rf"^(?:(?:le|du|au|en|depuis|jusqu au) )?(?:(?:{_JOURS}) ?)?(?:\d{{1,2}}(?:er)? (?:{_MOIS})(?: \d{{2,4}})?|"
        rf"\d{{1,2}} \d{{1,2}}(?: \d{{2,4}})?|\d{{4}}|{_JOURS}|{_MOIS}(?: \d{{4}})?)$|"
        r"^(?:aujourd'?hui|aujourd hui|demain|hier|apres demain|ce soir|ce matin|cet apres midi|cette semaine|"
        r"la semaine prochaine|ce week end|le week end)$")),
    ("heure ou durée seule", re.compile(
        r"^(?:(?:a|vers|pour|dans|pendant|de) )?(?:\d{1,2} ?h(?: ?\d{2})?|\d{1,2} heures?(?: \d{1,2})?|"
        r"\d+ ?(?:min|minutes?|secondes?|sec|s|heures?|jours?|semaines?|mois|ans?))$")),
    ("commande ponctuelle", re.compile(r"\b(?:alarme|minuteur|chrono|chronometre|reveil|rappel|timer)\b.*\d|"
                                       r"\d.*\b(?:alarme|minuteur|chrono|reveil|rappel|timer)\b")),
    ("nombre seul", re.compile(r"^\d+(?:[ ,.]\d+)?(?: ?%| ?euros?)?$")),
]
NOTES_EXPLICITES = {"a dit", "a noté", "a note"}
# Emploi du temps, cours d'un jour, devoirs, semaine A/B : ça change sans arrêt et Pronote le sait mieux.
# Jamais retenu tout seul, et jamais donné à l'IA comme souvenir (texte normalisé).
SCOLAIRE = re.compile(r"\bemploi du temps\b|\bsemaine [ab]\b|\bdevoirs?\b|\binterro|\bcontrole\b|"
                      r"\bcours\b.*\b(?:lundi|mardi|mercredi|jeudi|vendredi|matin|apres midi)\b|"
                      r"\b(?:lundi|mardi|mercredi|jeudi|vendredi)\b.*\b(?:matin|apres midi|cours)\b")
JOUR = 86400


def _cle_doublon(nom: str) -> str:
    """« Les livres » et « livre » -> « livre » : pour repérer les doublons."""
    mots = []
    for m in normaliser(nom).replace("'", " ").split():
        if m in MOTS_VIDES:
            continue
        if len(m) > 3 and m[-1] in "sx":
            m = m[:-1]
        mots.append(m)
    return " ".join(sorted(mots))


def analyser(noeuds: list, liens: list, maintenant: float | None = None) -> list:
    """Souvenirs qui ressemblent à du bruit (dates, heures, bouts de phrases, doublons…), jamais « Moi »
    ni les souvenirs gardés. Chaque suggestion : {id, nom, raison, vers (doublon à fusionner) }."""
    maintenant = maintenant or time.time()
    nb = {}
    explicites = set()
    for l in liens:
        for k in (l.get("a"), l.get("b")):
            nb[k] = nb.get(k, 0) + 1
        if l.get("a") == "moi" and str(l.get("relation", "")).lower() in NOTES_EXPLICITES:
            explicites.add(l.get("b"))
    candidats = [n for n in noeuds if n.get("id") != "moi" and not n.get("texte")]   # notes écrites dans Obsidian : jamais
    suggestions, deja = [], set()
    # doublons : on garde le souvenir épinglé, sinon le plus relié, sinon le plus ancien
    groupes = {}
    for n in candidats:
        k = _cle_doublon(n.get("nom", ""))
        if k:
            groupes.setdefault(k, []).append(n)
    for groupe in groupes.values():
        if len(groupe) < 2:
            continue
        groupe.sort(key=lambda n: (not n.get("garde"), -nb.get(n["id"], 0), n.get("cree", 0)))
        garde = groupe[0]
        for n in groupe[1:]:
            if not n.get("garde"):
                suggestions.append({"id": n["id"], "nom": n.get("nom", ""), "raison": f"doublon de « {garde.get('nom', '')} »",
                                    "vers": garde["id"]})
                deja.add(n["id"])
    for n in candidats:
        if n["id"] in deja or n.get("garde"):
            continue
        nom = n.get("nom", "")
        norm = normaliser(nom)
        raison = next((r for r, motif in BRUIT if motif.search(norm)), None)
        explicite = n["id"] in explicites
        if not raison and not explicite:
            utiles = [m for m in norm.replace("'", " ").split() if m not in MOTS_VIDES]
            if not utiles or len("".join(utiles)) < 2:
                raison = "trop court"
            elif len(nom) > 45 or len(norm.split()) > 7:
                raison = "phrase trop longue"
            elif nb.get(n["id"], 0) == 0:
                raison = "relié à rien"
            elif (not n.get("utilise") and n.get("type", "autre") == "autre" and nb.get(n["id"], 0) <= 1
                  and maintenant - n.get("vu", n.get("cree", maintenant)) > 60 * JOUR):
                raison = "jamais utilisé depuis 2 mois"
        if raison:
            suggestions.append({"id": n["id"], "nom": nom, "raison": raison})
    return suggestions


def resumer_suggestions(suggestions: list) -> str:
    """« 5 dates ou heures, 2 doublons… par exemple « 3h », « 7 mai 2022 » »."""
    familles = {}
    for s in suggestions:
        r = s["raison"]
        f = ("doublons" if r.startswith("doublon") else "dates ou heures" if r in ("date seule", "heure ou durée seule")
             else "commandes ponctuelles" if r == "commande ponctuelle" else "bouts de phrases" if r in
             ("trop court", "phrase trop longue", "nombre seul") else "souvenirs inutiles")
        familles[f] = familles.get(f, 0) + 1
    parties = [f"{n} {f}" for f, n in sorted(familles.items(), key=lambda x: -x[1])]
    exemples = ", ".join(f"« {s['nom']} »" for s in suggestions[:4])
    return ", ".join(parties) + (f", par exemple {exemples}" if exemples else "")


# ── Coffre Obsidian : la mémoire est un dossier de notes Markdown ─────────────
DOSSIERS = {"moi": "", "personne": "Personnes", "lieu": "Lieux", "ecole": "École", "projet": "Projets",
            "preference": "Goûts", "habitude": "Habitudes", "activite": "Activités", "objet": "Objets",
            "evenement": "Événements", "autre": "Autres"}
TYPE_DU_DOSSIER = {normaliser(v): k for k, v in DOSSIERS.items() if v}
LIGNE_LIEN = re.compile(r"^- (?P<r>.+?) \[\[(?P<b>[^\]|#]+)(?:\|[^\]]*)?\]\](?: %%(?P<t>[\d.]+)%%)?\s*$")
INTERDITS = re.compile(r'[\\/:*?"<>|#^\[\]%]')


def nom_de_fichier(nom: str) -> str:
    s = re.sub(r"\s+", " ", INTERDITS.sub(" ", str(nom))).strip().strip(".")[:80].strip()
    return s or "Sans nom"


def _cle_fichier(chemin) -> str:
    return os.path.normcase(str(chemin))


def _frontmatter(texte: str) -> tuple[dict, str]:
    if not texte.startswith("---"):
        return {}, texte
    fin = texte.find("\n---", 3)
    if fin < 0:
        return {}, texte
    meta = {}
    for ligne in texte[3:fin].splitlines():
        if ":" in ligne:
            k, v = ligne.split(":", 1)
            meta[k.strip()] = v.strip()
    return meta, texte[fin + 4:].lstrip("\n")


def _rendu(n: dict, liens: list, noms: dict) -> str:
    meta = [f"id: {n['id']}", f"type: {n.get('type', 'autre')}", f"cree: {round(float(n.get('cree') or time.time()))}"]
    for k in ("vu", "utilise"):
        if n.get(k):
            meta.append(f"{k}: {round(float(n[k]))}")
    if n.get("garde"):
        meta.append("garde: true")
    lignes = ["---", *meta, "---", f"# {n.get('nom', '')}", ""]
    if n.get("texte"):
        lignes += [n["texte"].strip(), ""]
    for l in liens:
        cible = noms.get(l["b"])
        if not cible:
            continue
        stem, nom_b = cible
        lien = f"[[{stem}]]" if stem == nom_b else f"[[{stem}|{nom_b}]]"
        lignes.append(f"- {l['relation']} {lien} %%{round(float(l.get('maj') or l.get('cree') or time.time()))}%%")
    return "\n".join(lignes).rstrip() + "\n"


class Coffre:
    """S'utilise comme Stockage (« with coffre as d: » → {"noeuds", "liens"}), mais la mémoire est un coffre
    Obsidian : une note par souvenir dans le dossier de son type. Les notes changées dans Obsidian sont relues."""

    def __init__(self, dossier):
        self.dossier = Path(dossier)
        self.verrou = threading.RLock()
        self.donnees = None
        self.empreinte = None
        self.textes = {}          # fichier -> texte lu ou écrit par Jarvis

    def _notes(self) -> list:
        if not self.dossier.exists():
            return []
        return sorted(p for p in self.dossier.rglob("*.md")
                      if not any(x.startswith(".") for x in p.relative_to(self.dossier).parts))

    @staticmethod
    def _empreinte(notes) -> tuple:
        out = []
        for p in notes:
            try:
                st = p.stat()
                out.append((_cle_fichier(p), st.st_mtime_ns, st.st_size))
            except OSError:
                pass
        return tuple(out)

    def _lire(self) -> dict:
        notes = self._notes()
        emp = self._empreinte(notes)
        if self.donnees is not None and emp == self.empreinte:
            return self.donnees
        noeuds, par_id, par_nom, bruts, self.textes = [], {}, {}, [], {}
        for p in notes:
            try:
                texte = p.read_text(encoding="utf-8")
                quand = p.stat().st_mtime
            except OSError:
                continue
            self.textes[_cle_fichier(p)] = texte
            meta, corps = _frontmatter(texte)
            parties = p.relative_to(self.dossier).parts
            type_ = meta.get("type") or (TYPE_DU_DOSSIER.get(normaliser(parties[0])) if len(parties) > 1 else None)
            titre, libre, liens = p.stem, [], []
            for ligne in corps.splitlines():
                m = LIGNE_LIEN.match(ligne.strip())
                if ligne.startswith("# ") and titre == p.stem and not libre and not liens:
                    titre = ligne[2:].strip() or p.stem
                elif m:
                    liens.append((m["r"].strip(), m["b"].strip(), m["t"]))
                else:
                    libre.append(ligne)
            id_ = meta.get("id") or ("moi" if normaliser(titre) == "moi" else cle(titre))
            if id_ == "moi":
                type_ = "moi"
            n = par_id.get(id_)
            if n is None:
                n = {"id": id_, "nom": titre, "type": type_ if type_ in TYPES else "autre"}
                try:
                    n["cree"] = float(meta.get("cree") or quand)
                except ValueError:
                    n["cree"] = quand
                for k in ("vu", "utilise"):
                    try:
                        if meta.get(k):
                            n[k] = float(meta[k]) if k == "vu" else int(float(meta[k]))
                    except ValueError:
                        pass
                if meta.get("garde", "").lower() in ("true", "oui", "1"):
                    n["garde"] = True
                noeuds.append(n)
                par_id[id_] = n
            texte_libre = "\n".join(libre).strip()
            if texte_libre:
                n["texte"] = (n.get("texte", "") + "\n\n" + texte_libre).strip()
            par_nom.setdefault(normaliser(p.stem), id_)
            par_nom.setdefault(normaliser(titre), id_)
            bruts += [(id_, r, b, t) for r, b, t in liens]
        if "moi" not in par_id:
            noeuds.insert(0, {"id": "moi", "nom": "Moi", "type": "moi", "cree": time.time()})
        liens = []
        for a, r, b_nom, t in bruts:
            b = par_nom.get(normaliser(b_nom))
            if not b:      # lien écrit dans Obsidian vers une note qui n'existe pas encore : on la crée
                b = cle(b_nom)
                if b not in par_id:
                    par_id[b] = {"id": b, "nom": b_nom, "type": "autre", "cree": time.time()}
                    noeuds.append(par_id[b])
                par_nom[normaliser(b_nom)] = b
            if a != b and not any(l["a"] == a and l["b"] == b for l in liens):
                liens.append({"a": a, "b": b, "relation": r[:40], "cree": float(t) if t else time.time()})
        self.donnees, self.empreinte = {"noeuds": noeuds, "liens": liens}, emp
        return self.donnees

    def _ecrire(self, d: dict):
        self.dossier.mkdir(parents=True, exist_ok=True)
        noms, pris = {}, set()
        for n in sorted(d["noeuds"], key=lambda n: (n["id"] != "moi", n.get("cree", 0))):
            base = "Moi" if n["id"] == "moi" else nom_de_fichier(n.get("nom", ""))
            stem, k = base, 2
            while normaliser(stem) in pris:
                stem, k = f"{base} ({k})", k + 1
            pris.add(normaliser(stem))
            noms[n["id"]] = (stem, str(n.get("nom", "")).strip() or stem)
        par_a = {}
        for l in d["liens"]:
            par_a.setdefault(l["a"], []).append(l)
        voulus = {}
        for n in d["noeuds"]:
            sous = DOSSIERS.get(n.get("type", "autre"), "Autres")
            chemin = (self.dossier / sous if sous else self.dossier) / f"{noms[n['id']][0]}.md"
            voulus[_cle_fichier(chemin)] = (chemin, _rendu(n, par_a.get(n["id"], []), noms))
        for cle_f, (chemin, texte) in voulus.items():
            if self.textes.get(cle_f) == texte and chemin.exists():
                continue
            chemin.parent.mkdir(parents=True, exist_ok=True)
            tmp = chemin.with_suffix(".tmp")
            tmp.write_text(texte, encoding="utf-8")
            os.replace(tmp, chemin)
            self.textes[cle_f] = texte
        for p in self._notes():           # souvenirs oubliés : leur note disparaît
            if _cle_fichier(p) not in voulus:
                try:
                    p.unlink()
                    self.textes.pop(_cle_fichier(p), None)
                except OSError:
                    pass
        for sous in DOSSIERS.values():
            if sous:
                try:
                    (self.dossier / sous).rmdir()      # seulement s'il est vide
                except OSError:
                    pass
        self.donnees, self.empreinte = d, self._empreinte(self._notes())

    def __enter__(self):
        self.verrou.acquire()
        try:
            return self._lire()
        except BaseException:
            self.verrou.release()
            raise

    def __exit__(self, *exc):
        try:
            if exc[0] is None:
                self._ecrire(self.donnees)
            else:
                self.donnees = None       # relu depuis les fichiers la prochaine fois
        finally:
            self.verrou.release()


def dossier_coffre(ctx) -> Path:
    """[memoire] coffre dans config.toml, sinon Documents\\Jarvis\\Mémoire (pendant les tests : à côté des données)."""
    import core.stockage as stockage_mod
    from core.config import DONNEES
    choisi = str(((getattr(ctx, "cfg", None) or {}).get("memoire") or {}).get("coffre", "") or "").strip()
    if choisi:
        return Path(os.path.expandvars(choisi)).expanduser()
    if Path(stockage_mod.DOSSIER) != DONNEES / "donnees":
        return Path(stockage_mod.DOSSIER).parent / "Mémoire"
    return Path(os.environ.get("USERPROFILE") or Path.home()) / "Documents" / "Jarvis" / "Mémoire"


def _racines(texte: str) -> list:
    """Mots utiles, sans pluriel : « j'aime les jeux vidéo » → [aime, jeu, video]."""
    out = []
    for m in normaliser(texte).replace("'", " ").split():
        if m in MOTS_VIDES or m in ("j", "qu", "m", "t", "s", "n", "c"):
            continue
        out.append(m[:-1] if len(m) > 3 and m[-1] in "sx" else m)
    return out


def _correspond(mot: str, mots: list) -> bool:
    return any(w == mot or (min(len(w), len(mot)) >= 4 and (w.startswith(mot) or mot.startswith(w))) for w in mots)


# Ce que l'IA a le droit de dire quand elle range (« range »).
CONSIGNE_RANGER = (
    "Tu ranges la mémoire de l'assistant d'un collégien. Voici ses souvenirs, un par ligne : "
    "« id | nom | type | ce qui y est relié ». Réponds en JSON : {\"types\": {\"id\": \"type\"}, "
    "\"fusions\": [[\"id_en_double\", \"id_a_garder\"]], \"oublier\": [\"id\"], \"renommer\": {\"id\": \"nom court\"}}. "
    "types possibles : " + ", ".join(TYPES[1:]) + ". Donne à chaque souvenir le type qui lui va vraiment. "
    "Fusionne deux souvenirs qui parlent de la même chose. Oublie seulement ce qui ne sert à rien (bouts de "
    "phrases, commandes ponctuelles, infos qui ne disent rien de durable sur lui). Renomme seulement un nom mal "
    "écrit ou trop long (5 mots au plus). Ne touche jamais à « moi ». Dans le doute, ne change rien.")
# Rangement automatique : seulement le bruit sûr (jamais « relié à rien » ni « jamais utilisé »).
RAISONS_SURES = ("date seule", "heure ou durée seule", "commande ponctuelle", "nombre seul", "trop court")


def installer(ctx, reg):
    import core.stockage as stockage_mod
    base = Coffre(dossier_coffre(ctx))
    ancien = Path(stockage_mod.DOSSIER) / "memoire.json"
    if not base._notes() and ancien.exists():     # 1re fois : l'ancienne mémoire devient un coffre Obsidian
        try:
            vieux = json.loads(ancien.read_text(encoding="utf-8"))
            with base as d:
                d["noeuds"] = vieux.get("noeuds") or d["noeuds"]
                d["liens"] = vieux.get("liens") or []
            ancien.replace(ancien.with_name("memoire.avant-obsidian.json"))
            log.info("mémoire : %d souvenirs copiés dans le coffre Obsidian %s", len(d["noeuds"]), base.dossier)
        except (OSError, ValueError):
            log.exception("mémoire : copie vers le coffre Obsidian impossible")

    def chemin_sauvegarde():
        # lu à chaque fois : les tests remplacent le dossier de données
        return stockage_mod.DOSSIER / "memoire.sauvegarde.json"

    def lire_sauvegarde():
        try:
            return json.loads(chemin_sauvegarde().read_text(encoding="utf-8"))
        except (OSError, ValueError):
            return None

    def sauvegarder_tout(d):
        """Photo complète de la mémoire avant un grand rangement : « annule le rangement » la remet telle quelle."""
        try:
            stockage_mod.DOSSIER.mkdir(parents=True, exist_ok=True)
            chemin_sauvegarde().write_text(json.dumps({"date": time.time(), "complet": True, "noeuds": d["noeuds"],
                                                       "liens": d["liens"]}, ensure_ascii=False), encoding="utf-8")
        except OSError:
            log.warning("mémoire : sauvegarde avant rangement impossible")

    def effacer(d, ids, fusions=None, sauvegarde=True) -> tuple[int, int]:
        """Supprime des souvenirs (et fusionne les doublons dans `fusions` {id: id_gardé}) après avoir
        sauvegardé la mémoire, pour pouvoir annuler. Ne touche jamais « Moi ». Renvoie (oubliés, fusionnés)."""
        fusions = fusions or {}
        ids = {i for i in ids if i != "moi" and any(n["id"] == i for n in d["noeuds"])}
        if not ids:
            return 0, 0
        avant = {"date": time.time(), "noeuds": json.loads(json.dumps(d["noeuds"])),
                 "liens": json.loads(json.dumps(d["liens"])), "ajoutes": []}
        fusionnes = 0
        for i in ids:
            vers = fusions.get(i)
            if not vers or vers in ids or not any(n["id"] == vers for n in d["noeuds"]):
                continue
            fusionnes += 1
            for l in [l for l in d["liens"] if i in (l["a"], l["b"])]:
                a, b = (vers if l["a"] == i else l["a"]), (vers if l["b"] == i else l["b"])
                if a == b or any(x["a"] == a and x["b"] == b for x in d["liens"]):
                    continue
                nouveau = {"a": a, "b": b, "relation": l["relation"], "cree": time.time()}
                d["liens"].append(nouveau)
                avant["ajoutes"].append({"a": a, "b": b, "relation": l["relation"]})
        d["noeuds"] = [n for n in d["noeuds"] if n["id"] not in ids]
        d["liens"] = [l for l in d["liens"] if l["a"] not in ids and l["b"] not in ids]
        if sauvegarde:
            try:
                stockage_mod.DOSSIER.mkdir(parents=True, exist_ok=True)
                chemin_sauvegarde().write_text(json.dumps(avant, ensure_ascii=False), encoding="utf-8")
            except OSError:
                log.warning("mémoire : sauvegarde avant nettoyage impossible")
        return len(ids) - fusionnes, fusionnes

    def suggestions_de(d):
        return [s for s in analyser(d["noeuds"], d["liens"]) if not (notes_masquees() and NOTES.search(s["nom"].lower()))]

    def nettoyer(ids=None) -> dict:
        with base as d:
            sugg = suggestions_de(d)
            fusions = {s["id"]: s["vers"] for s in sugg if s.get("vers")}
            gardes = {n["id"] for n in d["noeuds"] if n.get("garde")}
            cibles = [s["id"] for s in sugg] if ids is None else [str(i) for i in ids]
            ignores = len([i for i in cibles if i in gardes])
            oublies, fusionnes = effacer(d, [i for i in cibles if i not in gardes], fusions)
        if not oublies and not fusionnes:
            msg = "Rien à supprimer." + (" Les souvenirs gardés (épinglés) ne sont jamais supprimés." if ignores else "")
            return {"ok": True, "message": msg, "oublies": 0, "fusionnes": 0}
        morceaux = []
        if oublies:
            morceaux.append(f"{oublies} souvenir{'s' if oublies > 1 else ''} oublié{'s' if oublies > 1 else ''}")
        if fusionnes:
            morceaux.append(f"{fusionnes} doublon{'s' if fusionnes > 1 else ''} fusionné{'s' if fusionnes > 1 else ''}")
        msg = "Mémoire rangée : " + " et ".join(morceaux) + ". Tu peux annuler le nettoyage."
        if ignores:
            msg += f" ({ignores} gardé{'s' if ignores > 1 else ''} laissé{'s' if ignores > 1 else ''} de côté.)"
        return {"ok": True, "message": msg, "oublies": oublies, "fusionnes": fusionnes}

    def annuler_nettoyage() -> dict:
        sauvegarde = lire_sauvegarde()
        if not sauvegarde:
            return {"ok": False, "erreur": "Il n'y a pas de nettoyage à annuler."}
        if sauvegarde.get("complet"):          # grand rangement : on remet la photo telle quelle
            with base as d:
                d["noeuds"], d["liens"] = sauvegarde.get("noeuds") or d["noeuds"], sauvegarde.get("liens") or []
            try:
                chemin_sauvegarde().unlink()
            except OSError:
                pass
            return {"ok": True, "message": "Rangement annulé : ta mémoire est revenue comme avant."}
        with base as d:
            for a in sauvegarde.get("ajoutes", []):
                d["liens"] = [l for l in d["liens"] if not (l["a"] == a["a"] and l["b"] == a["b"]
                                                            and l["relation"] == a["relation"])]
            ids = {n["id"] for n in d["noeuds"]}
            remis = 0
            for n in sauvegarde.get("noeuds", []):
                if n.get("id") and n["id"] not in ids:
                    d["noeuds"].append(n)
                    ids.add(n["id"])
                    remis += 1
            for l in sauvegarde.get("liens", []):
                if l.get("a") in ids and l.get("b") in ids and not any(x["a"] == l["a"] and x["b"] == l["b"]
                                                                       for x in d["liens"]):
                    d["liens"].append(l)
        try:
            chemin_sauvegarde().unlink()
        except OSError:
            pass
        return {"ok": True, "message": f"Nettoyage annulé : {remis} souvenir{'s' if remis > 1 else ''} remis."}

    def modifier(p: dict) -> dict:
        ids = [str(i) for i in (p.get("ids") or ([p["id"]] if p.get("id") else []))]
        if not ids:
            return {"ok": False, "erreur": "Choisis d'abord un souvenir."}
        if "nom" in p:
            nom = str(p.get("nom", "")).strip()
            if len(ids) != 1 or ids[0] == "moi":
                return {"ok": False, "erreur": "On ne peut renommer qu'un souvenir à la fois (et pas « Moi »)."}
            if not nom or len(nom) > 60:
                return {"ok": False, "erreur": "Un souvenir doit contenir entre 1 et 60 caractères."}
            if contient_secret(nom):
                return {"ok": False, "erreur": "Les mots de passe, codes et clés secrètes ne sont pas mémorisés."}
            ancien, nouveau = ids[0], cle(nom)
            with base as d:
                n = next((n for n in d["noeuds"] if n["id"] == ancien), None)
                if not n:
                    return {"ok": False, "erreur": "Ce souvenir n'existe plus."}
                if nouveau != ancien and any(x["id"] == nouveau for x in d["noeuds"]):
                    effacer(d, [ancien], {ancien: nouveau})       # même nom qu'un autre : on les fusionne
                    return {"ok": True, "message": f"Fusionné avec « {nom} »."}
                n["nom"] = nom
                if nouveau != ancien and nouveau != "moi":
                    n["id"] = nouveau
                    for l in d["liens"]:
                        l["a"] = nouveau if l["a"] == ancien else l["a"]
                        l["b"] = nouveau if l["b"] == ancien else l["b"]
            return {"ok": True}
        with base as d:
            for n in d["noeuds"]:
                if n["id"] not in ids:
                    continue
                if "type" in p and n["id"] != "moi":
                    n["type"] = p["type"] if p["type"] in TYPES[1:] else "autre"
                if "garde" in p:
                    n["garde"] = bool(p["garde"])
        return {"ok": True}

    def etat_memoire() -> dict:
        with base as d:
            noeuds, liens = d["noeuds"], d["liens"]
            if notes_masquees():          # notes cachées dans l'île : pas de « moyenne de 12 » sur la carte non plus
                caches = {n["id"] for n in noeuds if n["id"] != "moi" and NOTES.search(n["nom"].lower())}
                noeuds = [n for n in noeuds if n["id"] not in caches]
                liens = [l for l in liens if l["a"] not in caches and l["b"] not in caches]
            nb = {}
            for l in liens:
                for k in (l["a"], l["b"]):
                    nb[k] = nb.get(k, 0) + 1
            noeuds = [dict(n, nb_liens=nb.get(n["id"], 0)) for n in noeuds]
            sugg = suggestions_de(d)
        sauvegarde = lire_sauvegarde()
        quand = time.strftime("%d/%m %H:%M", time.localtime(sauvegarde["date"])) if sauvegarde else None
        return {"ok": True, "noeuds": noeuds, "liens": liens, "suggestions": sugg, "sauvegarde": quand}

    def noeud(d, nom: str, type_: str = "autre", longueur: int = 60, fusionner: bool = False):
        if normaliser(nom) in ("moi", "je", "l'utilisateur", "utilisateur"):
            return next(n for n in d["noeuds"] if n["id"] == "moi")
        id_ = cle(nom)
        n = next((n for n in d["noeuds"] if n["id"] == id_), None)
        if not n and fusionner:        # « les jeux vidéo » existe déjà sous le nom « jeu vidéo » : on le reprend
            k = _cle_doublon(nom)
            n = next((x for x in d["noeuds"] if k and x["id"] != "moi" and _cle_doublon(x.get("nom", "")) == k), None)
        if not n:
            n = {"id": id_, "nom": nom.strip()[:longueur], "type": type_ if type_ in TYPES else "autre", "cree": time.time()}
            d["noeuds"].append(n)
        n["vu"] = time.time()
        return n

    def ajouter_faits(faits: list, max_objet: int = 80, trier: bool = False) -> int:
        """trier = souvenirs devinés par l'IA après un échange : sans le bruit, et rattachés aux souvenirs existants."""
        ajoutes = 0
        with base as d:
            for f in faits:
                s, r, o = (str(f.get(k, "")).strip() for k in ("sujet", "relation", "objet"))
                if not s or not r or not o or len(o) > max_objet or contient_secret(s, r, o):
                    continue
                if trier and (any(m.search(normaliser(o)) for _, m in BRUIT) or not _racines(o)
                              or SCOLAIRE.search(normaliser(f"{r} {o}"))):
                    continue
                a = noeud(d, s, "personne", fusionner=trier)
                b = noeud(d, o, str(f.get("type_objet", "autre")), longueur=max(60, max_objet), fusionner=trier)
                if a["id"] == b["id"]:
                    continue
                lien = next((l for l in d["liens"] if l["a"] == a["id"] and l["b"] == b["id"]), None)
                if lien:
                    lien["relation"], lien["maj"] = r[:40], time.time()
                else:
                    d["liens"].append({"a": a["id"], "b": b["id"], "relation": r[:40], "cree": time.time()})
                    ajoutes += 1
        return ajoutes

    def extraire(texte: str) -> list:
        reponse = ollama.discuter(ctx.cfg["cerveau"], [{"role": "system", "content": CONSIGNE},
                                                        {"role": "user", "content": texte}],
                                  temperature=0.0, max_mots=400, json_seulement=True)
        try:
            faits = json.loads(reponse or "{}").get("faits", [])
            return [f for f in faits if isinstance(f, dict)]
        except ValueError:
            return []

    def apres_echange(question: str, reponse: str):
        if len(question) < 12 or re.match(r"^\(?simul", reponse or ""):
            return
        n = ajouter_faits(extraire(f"L'utilisateur : {question}\nL'assistant : {reponse}")[:3], trier=True)
        if n:
            log.info("mémoire : %d nouveau(x) lien(s)", n)
            etat_auto["ajouts"] += n
            if etat_auto["ajouts"] >= 10:
                etat_auto["ajouts"] = 0
                ranger_auto()

    etat_auto = {"ajouts": 0}

    def ranger_auto() -> int:
        """Rangement tout seul : doublons et bruit sûr (dates, heures, commandes, bouts de phrases).
        Sauvegardé avant, donc « annule le nettoyage » le défait."""
        with base as d:
            sugg = [x for x in suggestions_de(d) if x.get("vers") or x["raison"] in RAISONS_SURES]
            if not sugg:
                return 0
            oublies, fusionnes = effacer(d, [x["id"] for x in sugg], {x["id"]: x["vers"] for x in sugg if x.get("vers")})
        if oublies or fusionnes:
            log.info("mémoire rangée toute seule : %d oubliés, %d fusionnés", oublies, fusionnes)
        return oublies + fusionnes

    def notes_masquees() -> bool:
        givre = getattr(ctx, "givre", None)
        e = (givre.etat() if givre else None) or {}
        return bool((e.get("pronote") or {}).get("notes_masquees"))

    def contexte(question: str = ""):
        mots = {m for m in normaliser(question).split() if len(m) > 2 and m not in MOTS_VIDES}
        with base as d:
            noms = {n["id"]: n["nom"] for n in d["noeuds"]}
            touches = {n["id"] for n in d["noeuds"] if mots & set(normaliser(n["nom"]).split())}
            for n in d["noeuds"]:
                if n["id"] in touches:            # compté pour le tri « plus utilisés » et le nettoyage
                    n["utilise"] = n.get("utilise", 0) + 1
            liens =[l for l in d["liens"] if l["a"] == "moi" or l["a"] in touches or l["b"] in touches]
        if not liens:
            return ""
        liens.sort(key=lambda l: (l["a"] in touches or l["b"] in touches, l.get("maj", l["cree"])), reverse=True)
        lignes = [f"{noms.get(l['a'], l['a'])} {l['relation']} {noms.get(l['b'], l['b'])}" for l in liens]
        # l'emploi du temps et les devoirs : seulement Pronote (sinon l'IA mélange tout)
        lignes = [x for x in lignes if not SCOLAIRE.search(normaliser(x))][:14]
        if notes_masquees():          # notes cachées dans l'île : la mémoire n'en parle pas non plus
            lignes = [x for x in lignes if not NOTES.search(x.lower())]
        if not lignes:
            return ""
        return "CE QUE TU SAIS SUR L'UTILISATEUR (ta mémoire, « Moi » = lui) : " + " ; ".join(lignes) + "."

    def adresse_onglet() -> str:
        """Adresse de l'onglet ouvert dans Chrome (par l'extension), ou "" s'il n'y en a pas."""
        pont = getattr(ctx, "pont", None)
        if pont is None or not getattr(pont, "connecte", False):
            return ""
        try:
            url = str((pont.commande({"commande": "onglet_texte"}, 3) or {}).get("url", ""))
        except Exception:  # noqa: BLE001
            return ""
        return url if url.startswith("http") else ""

    def retenir(fait: str):
        fait = fait.strip()
        if contient_secret(fait):
            return Resultat("Je ne conserve pas les mots de passe, codes ou clés secrètes.", ok=False)
        # « retiens que c'est ce lien pour aller sur mon profil » : l'adresse est celle de l'onglet ouvert
        if PARLE_D_UNE_PAGE.search(normaliser(fait)) and not ADRESSE_OU_COMMANDE.search(fait):
            url = adresse_onglet()
            if url:
                fait = f"{fait} (adresse : {url})"
        faits = extraire(f"L'utilisateur : Souviens-toi de ça : {fait}")
        entier = ADRESSE_OU_COMMANDE.search(fait)
        if entier:
            # une adresse ou une commande se perd dans l'extraction : on garde aussi la phrase entière
            ajouter_faits([{"sujet": "Moi", "relation": "a retenu", "objet": fait[:200], "type_objet": "autre"}],
                          max_objet=200)
        if not faits and not entier:
            faits = [{"sujet": "Moi", "relation": "a dit", "objet": fait[:80], "type_objet": "autre"}]
        ajouter_faits(faits)
        return Resultat("C'est noté, je m'en souviendrai.")

    def ce_que_tu_sais(sujet: str = ""):
        with base as d:
            noms = {n["id"]: n["nom"] for n in d["noeuds"]}
            cible = cle(sujet) if sujet and normaliser(sujet) not in ("moi", "toi") else "moi"
            liens = [l for l in d["liens"] if cible in (l["a"], l["b"])]
        if not liens:
            return Resultat("Je ne sais encore rien là-dessus." if sujet else "Je ne sais encore presque rien sur toi.")
        phrases = [f"{noms.get(l['a'], '?')} {l['relation']} {noms.get(l['b'], '?')}".replace("Moi ", "tu ", 1)
                   for l in liens]
        if notes_masquees():
            phrases = [p for p in phrases if not NOTES.search(p.lower())]
        if not phrases:
            return Resultat("Je ne sais encore presque rien sur toi.")
        return Resultat("Je sais que " + ", ".join(phrases[-8:]) + ".")

    def phrase(noms, l) -> str:
        return f"{noms.get(l['a'], l['a'])} {l['relation']} {noms.get(l['b'], l['b'])}".replace("Moi ", "tu ", 1)

    def trouver(d, sujet: str) -> tuple[list, list]:
        """Souvenirs (nœuds) et faits (liens) dont parle « sujet » : nom exact, sinon tous les mots (approchés,
        « foot » = « football »), sinon les meilleurs à moitié. Jamais « Moi » lui-même."""
        noms = {n["id"]: n["nom"] for n in d["noeuds"]}
        exact = [n for n in d["noeuds"] if n["id"] == cle(sujet) and n["id"] != "moi"]
        if exact:
            return exact, []
        mots = [m for m in _racines(sujet) if m not in ("moi", "tu", "souvenir", "memoire", "oublie", "info")]
        if not mots:
            return [], []

        def note(texte):
            ici = _racines(texte)
            return sum(1 for m in mots if _correspond(m, ici)) / len(mots)
        n_notes = [(note(n["nom"]), n) for n in d["noeuds"] if n["id"] != "moi"]
        l_notes = [(note(phrase(noms, l)), l) for l in d["liens"]]
        meilleur = max([x for x, _ in n_notes] + [x for x, _ in l_notes] + [0])
        seuil = 1.0 if meilleur >= 1.0 else meilleur
        if seuil < 0.5:
            return [], []
        noeuds = [n for x, n in n_notes if x >= seuil]
        ids = {n["id"] for n in noeuds}
        liens = [l for x, l in l_notes if x >= seuil and l["a"] not in ids and l["b"] not in ids]
        return noeuds, liens

    def trouver_par_ia(d, sujet: str) -> tuple[list, list]:
        """Rien trouvé avec les mots : l'IA lit la liste des souvenirs et dit lesquels correspondent."""
        noms = {n["id"]: n["nom"] for n in d["noeuds"]}
        liens = d["liens"][-150:]
        if not liens:
            return [], []
        lignes = "\n".join(f"{i} | {phrase(noms, l)}" for i, l in enumerate(liens))
        r = ollama.discuter(ctx.cfg["cerveau"], [
            {"role": "system", "content": "Voici la mémoire d'un assistant, un souvenir par ligne « numéro | souvenir ». "
             "L'utilisateur veut en oublier. Réponds en JSON : {\"numeros\": [..]} avec les numéros des souvenirs qui "
             "correspondent EXACTEMENT à ce qu'il veut oublier, ou {\"numeros\": []} si aucun."},
            {"role": "user", "content": f"{lignes}\n\nÀ oublier : {sujet}"}], temperature=0.0, max_mots=80,
            json_seulement=True)
        try:
            nums = [int(x) for x in json.loads(r or "{}").get("numeros", [])]
        except (ValueError, TypeError, AttributeError):
            return [], []
        return [], [liens[i] for i in nums if 0 <= i < len(liens)][:6]

    def oublier_souvenir(sujet: str):
        """Cherche les souvenirs et les faits dont parle « sujet », demande un « oui », puis les efface vraiment
        (la note disparaît du coffre, et un souvenir qui n'est plus relié à rien aussi)."""
        sujet = re.sub(r"(?i)\s*(?:de|dans) (?:ta|la|ma) m[ée]moire\s*$", "", str(sujet)).strip()
        sujet = re.sub(r"(?i)^(?:que |qu'|le fait que |l'info sur |tout ce que tu sais sur |tout sur )", "", sujet).strip()
        with base as d:
            noeuds, liens = trouver(d, sujet)
            if not noeuds and not liens and not getattr(ctx, "simulation", False):
                noeuds, liens = trouver_par_ia(d, sujet)
            noms = {n["id"]: n["nom"] for n in d["noeuds"]}
            relies = {k for l in d["liens"] for k in (l["a"], l["b"])}
        if not noeuds and not liens:
            return Resultat(f"Je n'avais rien sur « {sujet} ».")
        if len(noeuds) + len(liens) > 8:
            return Resultat(f"Il y a {len(noeuds) + len(liens)} souvenirs qui parlent de « {sujet} ». Sois plus précis, "
                            "ou trie-les dans la section Mémoire de la grande île.")
        ids = [n["id"] for n in noeuds]
        paires = [(l["a"], l["b"]) for l in liens]
        textes = ([f"« {n['nom']} » (et ce qui y est relié)" if n["id"] in relies else f"« {n['nom']} »" for n in noeuds]
                  + [f"« {phrase(noms, l)} »" for l in liens])

        def oui():
            with base as d:
                avant = len(d["noeuds"]) + len(d["liens"])
                sauvegarder_tout(d)
                d["liens"] = [l for l in d["liens"] if (l["a"], l["b"]) not in paires]
                # un souvenir qui n'est plus relié à rien (et que tu n'as pas écrit toi-même) part aussi
                encore = {k for l in d["liens"] for k in (l["a"], l["b"])}
                orphelins = [k for a, b in paires for k in (a, b) if k not in encore and k != "moi"
                             and not any(n["id"] == k and (n.get("garde") or n.get("texte")) for n in d["noeuds"])]
                effacer(d, ids + orphelins, sauvegarde=False)
                apres = len(d["noeuds"]) + len(d["liens"])
            if apres == avant:
                return Resultat("C'était déjà oublié.")
            return Resultat("C'est oublié. Si tu changes d'avis, dis « annule le nettoyage ».")
        question = (f"Tu veux que j'oublie {textes[0]} ?" if len(textes) == 1
                    else f"J'ai trouvé {len(textes)} souvenirs : {', '.join(textes)}. Je les oublie tous ?")
        return Resultat(question, confirmer=oui, annuler="D'accord, je garde tout.")

    def plan_de_l_ia(d) -> dict:
        """L'IA lit les souvenirs (par paquets de 100) et dit comment les classer, fusionner, renommer, oublier."""
        plan = {"types": {}, "fusions": [], "oublier": [], "renommer": {}}
        if getattr(ctx, "simulation", False):
            return plan
        noms = {n["id"]: n["nom"] for n in d["noeuds"]}
        voisins = {}
        for l in d["liens"]:
            voisins.setdefault(l["b"], []).append(f"{noms.get(l['a'], l['a'])} {l['relation']}")
            voisins.setdefault(l["a"], []).append(f"{l['relation']} {noms.get(l['b'], l['b'])}")
        noeuds = [n for n in d["noeuds"] if n["id"] != "moi" and not n.get("garde")]
        for i in range(0, len(noeuds), 100):
            lignes = "\n".join(f"{n['id']} | {n['nom']} | {n.get('type', 'autre')} | "
                               f"{'; '.join(voisins.get(n['id'], [])[:3])}" for n in noeuds[i:i + 100])
            r = ollama.discuter(ctx.cfg["cerveau"], [{"role": "system", "content": CONSIGNE_RANGER},
                                                      {"role": "user", "content": lignes}],
                                temperature=0.0, max_mots=900, json_seulement=True)
            try:
                p = json.loads(r or "{}")
            except ValueError:
                continue
            if not isinstance(p, dict):
                continue
            for k in ("types", "renommer"):
                if isinstance(p.get(k), dict):
                    plan[k].update({str(a): str(b) for a, b in p[k].items()})
            for k in ("fusions", "oublier"):
                if isinstance(p.get(k), list):
                    plan[k] += p[k]
        return plan

    def appliquer_plan(d, plan) -> dict:
        ids = {n["id"] for n in d["noeuds"]}
        gardes = {n["id"] for n in d["noeuds"] if n.get("garde") or n.get("texte")}
        stats = {"classes": 0, "renommes": 0}
        for n in d["noeuds"]:
            if n["id"] == "moi":
                continue
            t = plan["types"].get(n["id"])
            if t in TYPES[1:] and t != n.get("type"):
                n["type"], stats["classes"] = t, stats["classes"] + 1
            nom = str(plan["renommer"].get(n["id"], "")).strip()
            if nom and nom != n["nom"] and len(nom) <= 60 and not contient_secret(nom) and n["id"] not in gardes:
                n["nom"], stats["renommes"] = nom, stats["renommes"] + 1
        fusions = {}
        for f in plan["fusions"]:
            if (isinstance(f, list) and len(f) == 2 and f[0] in ids and f[1] in ids and f[0] != f[1]
                    and "moi" not in f and f[0] not in gardes):
                fusions[str(f[0])] = str(f[1])
        oublier = [i for i in plan["oublier"] if isinstance(i, str) and i in ids and i != "moi" and i not in gardes]
        stats["oublies"], stats["fusionnes"] = effacer(d, list(fusions) + oublier, fusions, sauvegarde=False)
        return stats

    def ranger_memoire():
        """« Range » : tout est réorganisé (bruit, doublons, puis l'IA classe, fusionne et renomme).
        Une photo est prise avant : « annule le rangement » remet tout comme avant."""
        with base as d:
            sauvegarder_tout(d)
            sugg = [x for x in suggestions_de(d) if x.get("vers") or x["raison"] in RAISONS_SURES]
            o1, f1 = effacer(d, [x["id"] for x in sugg], {x["id"]: x["vers"] for x in sugg if x.get("vers")},
                             sauvegarde=False)
            copie = json.loads(json.dumps(d))
        plan = plan_de_l_ia(copie)          # sans bloquer la mémoire pendant que l'IA réfléchit
        with base as d:
            st = appliquer_plan(d, plan)
            total = len(d["noeuds"]) - 1
        oublies, fusionnes = o1 + st["oublies"], f1 + st["fusionnes"]
        morceaux = [f"{x} {mot}" for x, mot in ((oublies, "oubliés"), (fusionnes, "doublons fusionnés"),
                                               (st["classes"], "reclassés"), (st["renommes"], "renommés")) if x]
        if not morceaux:
            return Resultat(f"Ta mémoire est déjà bien rangée ({total} souvenirs, chacun dans son dossier).")
        return Resultat(f"Mémoire rangée : {', '.join(morceaux)}. Il reste {total} souvenirs, chacun dans son dossier "
                        "du coffre Obsidian. Si ça ne te plaît pas, dis « annule le rangement ».")

    def annuler_rangement():
        r = annuler_nettoyage()
        return Resultat(r.get("message") or r.get("erreur", ""), ok=r["ok"])

    def ajouter_note(nom: str, type_: str = "autre"):
        nom = str(nom).strip()
        if not nom or len(nom) > 60:
            return {"ok": False, "erreur": "Une note doit contenir entre 1 et 60 caractères."}
        if contient_secret(nom):
            return {"ok": False, "erreur": "Les mots de passe, codes et clés secrètes ne sont pas mémorisés."}
        with base as d:
            if any(n["id"] == cle(nom) for n in d["noeuds"]):
                return {"ok": False, "erreur": "Cette note existe déjà."}
            note = noeud(d, nom, type_)
            d["liens"].append({"a": "moi", "b": note["id"], "relation": "a noté", "cree": time.time()})
        return {"ok": True}

    def ajouter_lien(a: str, b: str, relation: str):
        a, b, relation = str(a).strip(), str(b).strip(), str(relation).strip()
        if not a or not b or a == b:
            return {"ok": False, "erreur": "Choisis deux notes différentes à relier."}
        if not relation or len(relation) > 40:
            return {"ok": False, "erreur": "La relation doit contenir entre 1 et 40 caractères."}
        with base as d:
            ids = {n["id"] for n in d["noeuds"]}
            if a not in ids or b not in ids:
                return {"ok": False, "erreur": "Une des notes choisies n'existe plus."}
            lien = next((l for l in d["liens"] if l["a"] == a and l["b"] == b), None)
            if lien:
                lien["relation"], lien["maj"] = relation, time.time()
            else:
                d["liens"].append({"a": a, "b": b, "relation": relation, "cree": time.time()})
        return {"ok": True}

    def api(methode, p):
        if methode not in ("GET", "POST"):
            return {"ok": False, "erreur": "Méthode non prise en charge."}
        if methode == "POST":
            a = p.get("action")
            if a == "ajouter":
                texte = str(p.get("texte", "")).strip()
                if not texte:
                    return {"ok": False, "erreur": "Écris d’abord le contenu du souvenir."}
                resultat = retenir(texte)
                if not resultat.ok:
                    return {"ok": False, "erreur": resultat.dire}
            elif a == "ajouter_note":
                resultat = ajouter_note(str(p.get("nom", "")), str(p.get("type", "autre")))
                if not resultat["ok"]:
                    return resultat
            elif a == "lier":
                resultat = ajouter_lien(str(p.get("a", "")), str(p.get("b", "")), str(p.get("relation", "")))
                if not resultat["ok"]:
                    return resultat
            elif a == "supprimer":
                id_ = str(p.get("id", ""))
                if not id_ or id_ == "moi":
                    return {"ok": False, "erreur": "Ce souvenir ne peut pas être supprimé."}
                with base as d:
                    if not any(n["id"] == id_ for n in d["noeuds"]):
                        return {"ok": False, "erreur": "Ce souvenir n'existe plus."}
                    effacer(d, [id_])
            elif a == "nettoyer":          # ids choisis dans la liste, ou toutes les suggestions
                ids = p.get("ids")
                if ids is not None and not isinstance(ids, list):
                    return {"ok": False, "erreur": "Liste de souvenirs invalide."}
                message = nettoyer(ids)["message"]
                return dict(etat_memoire(), message=message)
            elif a == "annuler_nettoyage":
                r = annuler_nettoyage()
                if not r["ok"]:
                    return r
                return dict(etat_memoire(), message=r["message"])
            elif a == "modifier":
                r = modifier(p)
                if not r["ok"]:
                    return r
                if r.get("message"):
                    return dict(etat_memoire(), message=r["message"])
            elif a == "supprimer_lien":
                source, cible = str(p.get("a", "")), str(p.get("b", ""))
                if not source or not cible:
                    return {"ok": False, "erreur": "Le lien à supprimer est incomplet."}
                with base as d:
                    avant = len(d["liens"])
                    d["liens"] = [l for l in d["liens"] if not (l["a"] == source and l["b"] == cible)]
                    if len(d["liens"]) == avant:
                        return {"ok": False, "erreur": "Ce lien n'existe plus."}
            else:
                return {"ok": False, "erreur": "Action mémoire inconnue."}
        return etat_memoire()

    reg.api("memoire", api)
    reg.contexte(contexte)
    if not getattr(ctx, "simulation", False):
        reg.apres_echange(apres_echange)
    reg.outil("retenir", "Retient durablement une information sur l'utilisateur (« souviens-toi que… », « retiens ça »).",
              {"fait": "ce qu'il faut retenir, en une phrase compréhensible seule : remplace « ça », « cette "
                       "commande »… par ce dont il s'agit VRAIMENT dans la conversation (la commande exacte, le nom). "
                       "Pour la page ouverte dans Chrome, écris « cette page » : son adresse est ajoutée toute "
                       "seule"}, ["fait"], retenir)
    reg.outil("ce_que_tu_sais", "Dit ce que Jarvis sait (sa mémoire) sur l'utilisateur ou sur un sujet.",
              {"sujet": "personne ou sujet (vide = l'utilisateur)"}, [], ce_que_tu_sais)
    reg.outil("oublier_souvenir", "Oublie un souvenir précis de la mémoire (Jarvis demande confirmation).",
              {"sujet": "le souvenir à oublier"}, ["sujet"], oublier_souvenir)
    reg.outil("ranger_memoire", "Range / trie / réorganise toute la mémoire (« range ») : enlève le bruit et les "
              "doublons, puis classe chaque souvenir dans le bon dossier, fusionne et renomme. On peut annuler.", {}, [],
              ranger_memoire)
    reg.outil("annuler_nettoyage_memoire", "Annule le dernier nettoyage ou oubli de la mémoire (remet les souvenirs).",
              {}, [], annuler_rangement)
    reg.regle(r"^(?:range|ranges|trie|tries|nettoie|nettoies|organise|reorganise|classe|fais le menage dans|"
              r"fait le menage dans|fais du tri dans|fait du tri dans|fais le tri dans|fait le tri dans)(?: un peu)? "
              r"(?:ma|ta|la) memoire$|^(?:range|ranges|range toi|range tout ca|range ta tete|range tes souvenirs|"
              r"range ton cerveau)$", lambda m, t: [("ranger_memoire", {})], priorite=21)
    reg.regle(r"^(?:annule|defais|annuler) (?:le |ce |mon |ton )?(?:dernier )?(?:nettoyage|rangement|menage|tri)"
              r"(?: de (?:ma|ta|la) memoire)?$|^(?:restaure|remets) (?:ma|ta|la) memoire$",
              lambda m, t: [("annuler_nettoyage_memoire", {})], priorite=21)
    reg.regle(r"^oublie tout (?:ce que tu sais )?(?:sur|de|a propos de) (?P<s>.+)$",
              lambda m, t: [("oublier_souvenir", {"sujet": m["s"]})], priorite=71)
    reg.regle(r"^(?:oublie|efface de ta memoire|supprime de ta memoire|retire de ta memoire) "
              r"(?!pas\b|ca\b|cela\b|tout\b|ce que\b|ta derniere\b|la derniere\b)(?P<s>.+?)(?: de ta memoire)?$",
              lambda m, t: [("oublier_souvenir", {"sujet": m["s"]})], priorite=70)
    def retenir_direct(m, t):
        # « retiens ÇA », « c'est CE lien »… : seule l'IA, qui voit la conversation, sait de quoi il parle
        if RENVOI.search(m["f"]):
            return []
        return [("retenir", {"fait": re.sub(r"(?i)^\W*(souviens[- ]toi|rappelle[- ]toi|retiens|n'oublie pas)"
                                            r"(?: bien)?\s*(?:dans ta m[ée]moire\s*)?(?:que |qu'|de |d')?", "", t).strip()})]

    reg.regle(r"^(?:souviens toi|rappelle toi|retiens|n'oublie pas|n oublie pas)(?: bien)?"
              r"(?: dans ta memoire)? (?:que |qu'|de |d')?(?P<f>.+)$", retenir_direct, priorite=21)
    reg.regle(r"^(?:qu'est ce que|qu est ce que|que) tu sais (?:sur|de) (?P<s>.+?)$",
              lambda m, t: [("ce_que_tu_sais", {"sujet": m["s"]})], priorite=21)
