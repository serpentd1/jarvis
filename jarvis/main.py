"""Jarvis — assistant vocal 100 % local.

    Jarvis.exe                   sans fenêtre, icône près de l'horloge (= main.py --service)
    python main.py               pareil, avec une console qui montre ce qui se passe
    python main.py --clavier     appuie sur Entrée pour parler (en plus de « Hey Jarvis »)
    python main.py --texte       tu écris, Jarvis agit vraiment et répond à voix haute
    python main.py --test        vérifie tout sans micro (les actions sont simulées)
"""
import argparse
import ctypes
import logging
import os
import sys
import threading
import time
import types
from collections import deque

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")

from core import config, journal  # noqa: E402

log = logging.getLogger("jarvis")
CODE_REDEMARRER = 3
_mutex = None
_plantages = None


def instance_unique() -> bool:
    global _mutex
    _mutex = ctypes.windll.kernel32.CreateMutexW(None, False, "Local\\Jarvis-moteur")
    return ctypes.windll.kernel32.GetLastError() != 183      # 183 = existe déjà


class Jarvis:
    def __init__(self, args):
        from core.etat import Etat
        from core.givre import Givre
        from core.pont import Pont
        self.args = args
        self.cfg = config.charger()
        self.etat = Etat()
        self.givre = Givre(self.cfg["givre"]["url"])
        self.pont = Pont()
        self.cerveau = self.reg = None
        self.voix = self.stt = self.micro = self.mot = self.lecteur = None
        self.coupure, self.dit_avant_coupure = None, ""
        # Quand tu lui parles par-dessus, il finit d'écouter ta phrase (en continuant de parler), puis la garde ici
        # déjà transcrite ; `dit_au_debut` = ce qu'il avait dit quand tu as commencé à parler.
        self.phrase_coupure, self.dit_au_debut = None, ""
        self.question_a_completer = None          # ta phrase, quand tu l'as coupé avant qu'il dise grand-chose
        self._verrou_stt = threading.Lock()
        self.ctx = types.SimpleNamespace(cfg=self.cfg, givre=self.givre, pont=self.pont, etat=self.etat,
                                         simulation=bool(args.test), appels=[])

    # ── chargement ───────────────────────────────────────────
    def charger_cerveau(self):
        import modules
        from core.cerveau import Cerveau
        self.reg = modules.charger(self.ctx)
        self.cerveau = Cerveau(self.cfg["cerveau"], self.cfg["general"]["nom"], self.reg)
        self.ctx.cerveau = self.cerveau
        self.cerveau.prechauffer()
        for tache in self.reg.taches:
            threading.Thread(target=self._tache, args=(tache,), daemon=True, name=tache.__name__).start()

    @staticmethod
    def _tache(fonction):
        try:
            fonction()
        except Exception:  # noqa: BLE001
            log.exception("tâche de fond en erreur")

    def charger_voix(self, avec_micro=True, avec_stt=True):
        from core.tts import Voix
        self.voix = Voix(self.cfg["voix"])
        self.ctx.changer_voix = self.changer_voix
        self.ctx.voix_actuelle = lambda: self.voix.nom if self.voix else None
        if avec_stt:
            from core.stt import Transcripteur
            t = time.time()
            self.stt = Transcripteur(self.cfg["transcription"])
            log.info("Transcription %s prête sur %s (%.0f s)", self.stt.nom, self.stt.appareil, time.time() - t)
            self.ctx.changer_transcription = self.changer_transcription
            self.ctx.transcription_actuelle = lambda: self.stt.nom
        if avec_micro:
            from core.audio import Micro
            from core.sortie import AntiEcho, Lecteur
            from core.wakeword import MotActivation
            anti_echo = None
            if not self.cfg.get("conversation", {}).get("mode_casque", False):
                try:
                    anti_echo = AntiEcho()
                except Exception as e:  # noqa: BLE001
                    log.warning("Anti-écho indisponible (%s) : mets un casque, ou installe « livekit »", e)
            self.lecteur = Lecteur(anti_echo)
            self.micro = Micro(self.cfg["micro"]["appareil"], anti_echo)
            if anti_echo is not None:
                anti_echo.regler_delai(self.micro.flux.latency + self.lecteur.flux.latency)
            self.mot = MotActivation(float(self.cfg["micro"]["seuil_activation"]))

    def changer_voix(self, nom: str):
        """Change la voix (la télécharge si besoin) et la garde dans config.toml."""
        self.voix.changer(nom)
        self.cfg["voix"]["voix"] = nom
        try:
            config.ecrire_valeur("voix", "voix", nom)
        except Exception:  # noqa: BLE001
            log.exception("config.toml non modifié")

    def changer_transcription(self, modele: str, annoncer: bool = False):
        """Passe de Parakeet à Whisper (ou l'inverse) sans redémarrer, et le garde dans config.toml.
        L'ancien modèle continue d'écouter pendant que le nouveau se charge."""
        from core import stt
        with self._verrou_stt:
            if modele == self.stt.nom:
                return
            try:
                t = time.time()
                nouveau = stt.Transcripteur({**self.cfg["transcription"], "modele": modele})
            except Exception:  # noqa: BLE001
                log.exception("changement de transcription")
                if annoncer:
                    self.etat.annoncer(f"Je n'ai pas réussi à passer sur {stt.titre(modele)}.")
                return
            self.stt = nouveau
            self.cfg["transcription"]["modele"] = nouveau.nom
            log.info("Transcription %s prête sur %s (%.0f s)", nouveau.nom, nouveau.appareil, time.time() - t)
            try:
                config.ecrire_valeur("transcription", "modele", nouveau.nom)
            except Exception:  # noqa: BLE001
                log.exception("config.toml non modifié")
            if annoncer:
                self.etat.annoncer(f"C'est fait, je t'écoute maintenant avec {stt.titre(nouveau.nom)}.")

    def _changer_voix_et_dire(self, nom: str):
        if not self.voix or nom == self.voix.nom:
            return
        try:
            self.changer_voix(nom)
            self.etat.annoncer("Voici ma nouvelle voix. Elle te plaît ?")
        except Exception:  # noqa: BLE001
            log.exception("changement de voix")
            self.etat.annoncer("Je n'ai pas réussi à changer de voix.")

    # ── mode normal (Jarvis.exe) ─────────────────────────────
    def service(self) -> int:
        from core.serveur import Serveur
        serveur = Serveur(int(self.cfg["serveur"]["port"]), self.etat, self.pont, lambda: self.cerveau,
                          lambda: self.cfg["annonces"]["lire_reponses_ile"],
                          voix_actuelle=lambda: self.voix.nom if self.voix else None,
                          changer_voix=self._changer_voix_et_dire)
        if not serveur.demarrer():
            return 0
        self.etat.abonner(lambda e, t, q, r: self.givre.etat_jarvis(e, t, q, r, self.etat.duree, self.etat.toi))
        g = self.cfg["general"]
        # Une seule appli, une seule icône : quand la barre (île + widgets) est là, c'est SON icône près de
        # l'horloge qui sert pour tout. L'icône « J » de Python ne sert plus que si la barre manque.
        avec_barre = bool(g["lancer_givre"] and self.givre.trouver_exe(g["givre_exe"]))
        icone = None
        if not avec_barre:
            try:
                from core.icone import Icone
                icone = Icone(self.etat, self.givre, g,
                              voix_actuelle=lambda: self.voix.nom if self.voix else None,
                              changer_voix=lambda nom: threading.Thread(target=self._changer_voix_et_dire,
                                                                        args=(nom,), daemon=True).start())
                icone.demarrer()
            except Exception:  # noqa: BLE001
                log.exception("icône indisponible")
        if g["lancer_givre"]:
            threading.Thread(target=self.givre.lancer, args=(g["givre_exe"], g["givre_arguments"]), daemon=True).start()
        self.charger_cerveau()
        try:
            self.charger_voix()
        except Exception:  # noqa: BLE001
            log.exception("Voix ou micro indisponible : Jarvis répondra seulement au chat de l'île.")
            self.givre.notif("Jarvis", "Micro ou voix indisponible : regarde le journal de Jarvis.", "jarvis")
        if self.args.clavier:
            threading.Thread(target=self._entree_clavier, daemon=True).start()
        if self.args.lanceur:
            threading.Thread(target=self._surveiller_lanceur, args=(self.args.lanceur,), daemon=True).start()
        from core import mise_a_jour
        mise_a_jour.demarrer(self.cfg, self.etat)
        self.etat.changer("repos", "Prêt")
        if self.voix and g["phrase_demarrage"]:
            self.etat.annoncer(g["phrase_demarrage"])
        log.info("Jarvis est prêt. Dis « Hey Jarvis »…" + (" (ou appuie sur Entrée)" if self.args.clavier else ""))
        try:
            self.boucle()
        finally:
            if icone:
                icone.arreter()
            if self.micro:
                self.micro.arreter()
            if avec_barre and not self.etat.redemarrer:
                self.givre.quitter()          # « Quitter Jarvis » : l'île et les widgets partent aussi
        return CODE_REDEMARRER if self.etat.redemarrer else 0

    def _surveiller_lanceur(self, pid: int):
        """Si Jarvis.exe disparaît (tué dans le gestionnaire des tâches), le moteur s'arrête aussi."""
        import psutil
        while not self.etat.quitter.wait(2):
            if not psutil.pid_exists(pid):
                log.info("Jarvis.exe a disparu : j'arrête le moteur.")
                self.etat.quitter.set()

    def _entree_clavier(self):
        while not self.etat.quitter.is_set():
            try:
                input()
            except EOFError:
                return
            self.etat.ecouter_maintenant.set()

    def boucle(self):
        from core.audio import BLOC, FREQ

        micro_ouvert = False
        avant = deque(maxlen=int(2.0 * FREQ / BLOC))
        while not self.etat.quitter.is_set():
            if self.micro and self.etat.micro != micro_ouvert:
                (self.micro.demarrer if self.etat.micro else self.micro.pause)()
                micro_ouvert = self.etat.micro
                self.mot.reinitialiser()
                avant.clear()
                log.info("Micro %s", "allumé" if micro_ouvert else "coupé")
                self.etat.changer(self.etat.etat)
            if self.voix and not self.etat.annonces.empty():
                phrase = self.etat.annonces.get_nowait()
                self.etat.changer("parle", phrase)
                interrompu = self.parler(phrase)
                self.etat.changer("repos")
                if interrompu and self.coupure:     # annonce coupée par ta voix
                    self._noter_coupure(phrase)
                    self.ecouter("coupe", self.coupure)
                elif interrompu:
                    self.ecouter()
                elif micro_ouvert and self.etat.annonces.empty() and float(self.cfg["micro"]["conversation"]) > 0:
                    self.ecouter("annonce")           # « réponds-lui que… » sans redire « Hey Jarvis »
                continue
            if self.etat.ecouter_maintenant.is_set():
                self.etat.ecouter_maintenant.clear()
                self.ecouter()
                continue
            if not micro_ouvert:
                time.sleep(0.2)
                continue
            bloc = self.micro.bloc(0.25)
            if bloc is not None:
                avant.append(bloc)
            if bloc is not None and self.mot.entendu(bloc):
                log.info("« Hey Jarvis » entendu")
                self.mot.reinitialiser()
                self.ecouter(avant=list(avant))
                avant.clear()

    def ecouter(self, mode: str = "normal", avant=None):
        """mode : "normal" (après « Hey Jarvis »), "question" (Jarvis attend oui/non),
        "conversation" (suite de la discussion, sans redire « Hey Jarvis »)."""
        if not (self.micro and self.stt and self.cerveau):
            return
        while mode and not self.etat.quitter.is_set():
            mode = self._un_echange(mode, avant)
            avant = self.coupure if mode == "coupe" else None     # le début de ta phrase, quand tu l'as coupé

    def _un_echange(self, mode: str, avant=None):
        """Écoute une phrase, répond, et dit comment continuer (ou None pour revenir à « Hey Jarvis »)."""
        from core.audio import FREQ, ding
        from core.cerveau import STOP, commande_autour_du_mot, enlever_mot_activation
        from modules import normaliser
        m = self.cfg["micro"]
        conversation = float(m["conversation"])
        continuer = bool(m["conversation_continue"])
        discussion = mode in ("conversation", "annonce", "coupe")   # pas de « ding », et il faut parler assez fort
        if mode == "conversation":
            attente = float(m["fin_conversation"]) if continuer else conversation
        elif mode == "coupe":                       # tu viens de lui couper la parole : tu parles déjà
            attente = 2.0
        else:
            attente = conversation if mode == "annonce" else float(m["attente_debut"])
        deja_entendu, self.phrase_coupure = (self.phrase_coupure if mode == "coupe" else None), None
        if deja_entendu:
            # tu lui as parlé par-dessus : il a déjà écouté (et transcrit) ta phrase en entier en finissant la sienne
            audio, fin_phrase = None, time.perf_counter()
        else:
            deja_ouvert = self.micro.flux.active
            self.micro.demarrer()
            self.voix.prechauffer()                     # la voix en ligne se connecte pendant que tu parles
            try:
                self.etat.changer("ecoute", "On discute… (dis « stop » pour finir)"
                                  if mode == "conversation" and continuer else "À toi, je t'écoute…")
                if m["ding"] and not discussion:
                    ding(aigu=mode == "question")
                audio = self.micro.enregistrer_commande(
                    float(m["silence_fin"]), float(m["duree_max"]), attente,
                    ignorer_debut=0.0 if discussion else 0.3, voix_min=0.35 if discussion else 0.24,
                    volume_min=float(m["volume_conversation"]) if discussion else 150.0, prefixe=avant,
                    deja_dit_min=0.2 if mode == "coupe" else 1.3,
                    sur_voix=lambda: self.etat.changer("parole", "Je t'entends…"))
                fin_phrase = time.perf_counter()
            finally:
                if not deja_ouvert:
                    self.micro.pause()
            if not len(audio):
                self.etat.changer("repos")
                # après l'avoir coupé, il continue d'écouter au lieu de se mettre en pause
                return "conversation" if mode in ("conversation", "coupe") and (continuer or conversation > 0) else None
        self.etat.changer("reflechit")
        texte = deja_entendu or self.stt.transcrire(audio)
        mesure = {"fin": fin_phrase, "texte": time.perf_counter()}
        if getattr(self.micro, "perdus", 0):
            log.warning("Le micro a perdu du son %d fois (ordinateur trop occupé) : la phrase peut être mal comprise",
                        self.micro.perdus)
            self.micro.perdus = 0
        if audio is not None and self.cfg.get("conversation", {}).get("enregistrer_derniere_phrase", False):
            try:
                from core.stockage import DOSSIER
                self.micro.sauver_derniers(len(audio) / FREQ + 1.0, DOSSIER)
                (DOSSIER / "derniere-phrase.txt").write_text(texte, encoding="utf-8")
            except Exception:  # noqa: BLE001
                log.exception("enregistrement de la dernière phrase")
        if avant:
            texte = commande_autour_du_mot(texte)
        if not texte.strip():
            log.info("(rien compris)")
            self.etat.changer("repos")
            # un bruit ne coupe pas la discussion
            return "conversation" if mode in ("conversation", "coupe") and (continuer or conversation > 0) else None
        a_completer, self.question_a_completer = self.question_a_completer, None
        log.info("Toi    : %s", texte)
        if mode == "coupe" and STOP.match(normaliser(texte)):
            # « stop », « tais-toi » juste après l'avoir coupé : il se tait, sans nouvelle réponse
            log.info("Jarvis se tait")
            self.etat.changer("repos")
            return "conversation" if any(m_ in normaliser(texte) for m_ in ("tais", "chut")) else None
        if discussion and STOP.match(normaliser(texte)):
            log.info("Fin de la conversation")
            self.parler(self.cerveau.fin_conversation() or "D'accord, à plus tard !")
            self.etat.changer("repos")
            return None
        if mode == "coupe" and a_completer:
            # tu l'as coupé alors qu'il avait à peine commencé : tu n'avais pas fini ta phrase, on la recolle
            texte = f"{a_completer} {texte}"
            log.info("Ta phrase en entier : %s", texte)
        if self._reglage_micro_demande(texte):
            self.regler_micro()
            self.etat.changer("repos")
            return None
        self.etat.changer("reflechit", texte, toi=enlever_mot_activation(texte) or texte)
        # Le cerveau réfléchit dans un fil à part : Jarvis commence à parler dès la première phrase prête.
        import queue
        from core.cerveau import Reponse
        phrases, resultat, recues = queue.Queue(), {}, []

        def sur_phrase(p):
            mesure.setdefault("cerveau", time.perf_counter())
            recues.append(p)
            phrases.put(p)

        def penser():
            try:
                rep = self.cerveau.repondre(texte, "voix", sur_phrase=sur_phrase)
            except Exception:  # noqa: BLE001
                log.exception("cerveau")
                rep = Reponse("Oups, j'ai eu un bug. Tu peux répéter ?")
            resultat["rep"] = rep
            log.info("Jarvis : %s", rep.texte)
            reste = rep.texte if rep.reste is None else rep.reste
            if reste:
                sur_phrase(reste)
            phrases.put(None)
            # l'échange s'ajoute au fil du chat de l'île (sans changer l'état affiché)
            self.givre.etat_jarvis("", "", enlever_mot_activation(texte) or texte, rep.texte)

        fil = threading.Thread(target=penser, daemon=True, name="cerveau")
        fil.start()
        interrompu = self.parler(iter(phrases.get, None), mesure)
        if interrompu:
            self.cerveau.couper()                  # l'IA s'arrête d'écrire, la suite n'est pas dite
        fil.join(3 if interrompu else 120)
        self._journal_latence(mesure)
        rep = resultat.get("rep") or Reponse("")
        self.etat.changer("repos")
        if interrompu:
            # il saura où il en était : ce qu'il a VRAIMENT dit, et ce qu'il n'a pas eu le temps de dire
            self._noter_coupure(" ".join(recues) or rep.texte)
            if self.coupure:                       # coupé par ta voix : il t'écoute tout de suite
                if len(self.dit_au_debut) < 40:
                    self.question_a_completer = texte  # il n'avait presque rien dit : tu continuais sans doute
                    self.cerveau.coupe, self.cerveau.pas_dit = "…", ""   # sa réponse à ta demi-phrase est oubliée
                return "coupe"
            return "normal"
        if rep.suivi:
            return "question"
        return "conversation" if continuer or conversation > 0 else None

    def _noter_coupure(self, complet: str):
        """Dit au cerveau où il a été coupé : ce qu'il a dit, et ce qu'il lui restait à dire (pour un mini-résumé
        après sa réponse). S'il avait déjà tout dit, ce n'est pas vraiment une coupure : rien à résumer."""
        dit = (self.dit_avant_coupure or "").strip()
        mots = (complet or "").split()
        pas_dit = " ".join(mots[len(dit.split()):]).strip() if dit else (complet or "").strip()
        # il avait déjà tout dit quand ta phrase s'est finie : l'historique est juste, pas de note de coupure
        self.cerveau.coupe = None if dit and not pas_dit else (dit or "…")
        self.cerveau.pas_dit = pas_dit
        log.info("Coupé après « %s »%s", dit[-80:], f" ; pas dit : « {pas_dit[:120]} »" if pas_dit else " (il avait fini)")

    @staticmethod
    def _reglage_micro_demande(texte: str) -> bool:
        import re
        from core.cerveau import enlever_mot_activation
        from modules import normaliser
        return bool(re.match(r"^(?:regle|calibre|ajuste|teste)(?: moi)? (?:mon |le |ton )?micro(?:phone)?$",
                             normaliser(enlever_mot_activation(texte))))

    def regler_micro(self):
        """« Règle mon micro » : mesure le bruit de la pièce puis ta voix, et choisit `volume_conversation`
        (le niveau qu'il faut dépasser pendant la conversation continue). Écrit dans config.toml."""
        import numpy as np
        from core.audio import niveau
        from core.config import ecrire_valeur
        m = self.cfg["micro"]
        deja_ouvert = self.micro.flux.active
        self.micro.demarrer()
        try:
            self.parler("D'accord. Ne dis rien pendant deux secondes.")
            fond, fin = [], time.time() + 2.0
            while time.time() < fin:
                bloc = self.micro.bloc(0.5)
                if bloc is not None:
                    fond.append(niveau(bloc))
            self.parler("Maintenant, dis une phrase normalement, comme si tu me parlais.")
            audio = self.micro.enregistrer_commande(float(m["silence_fin"]), 8, 6, ignorer_debut=0.0)
        finally:
            if not deja_ouvert:
                self.micro.pause()
        if not len(audio) or not fond:
            self.parler("Je ne t'ai pas entendu. Redis « règle mon micro » pour recommencer.")
            return
        brut, taille = audio * 32768.0, 1280
        niveaux = [niveau(brut[i:i + taille]) for i in range(0, len(brut) - taille, taille)] or [0.0]
        voix, bruit = float(np.percentile(niveaux, 75)), float(np.median(fond))
        volume = int(max(bruit * 2.5, voix * 0.4, 120))
        log.info("Réglage du micro : bruit %.0f, voix %.0f -> volume_conversation %d", bruit, voix, volume)
        if volume >= voix * 0.8:
            self.parler("Il y a trop de bruit autour de toi pour bien régler. Réessaie dans un endroit plus calme.")
            return
        ecrire_valeur("micro", "volume_conversation", volume)
        m["volume_conversation"] = volume
        self.parler("C'est réglé ! Pendant nos discussions, je réagirai à ta voix, mais pas aux petits bruits.")

    @staticmethod
    def decouper_phrases(texte: str, min_car: int = 40) -> list:
        """Coupe la réponse en phrases (les très courtes sont regroupées) pour commencer à parler plus tôt."""
        import re
        morceaux = []
        for phrase in re.split(r"(?<=[.!?…])\s+", texte.strip()):
            if morceaux and len(morceaux[-1]) < min_car:
                morceaux[-1] += " " + phrase
            elif phrase:
                morceaux.append(phrase)
        return morceaux

    def _journal_latence(self, mesure: dict):
        """Temps de chaque étape, de la fin de ta phrase au début de sa voix (objectif : moins de 1,5 s)."""
        if not self.cfg.get("conversation", {}).get("journal_latence", True) or "voix" not in mesure:
            return
        etapes, avant = [], mesure["fin"]
        for cle, nom in (("texte", "transcription"), ("cerveau", "1re phrase de l'IA"), ("son", "voix prête"),
                         ("voix", "haut-parleur")):
            if cle in mesure:
                etapes.append(f"{nom} {(mesure[cle] - avant) * 1000:.0f} ms")
                avant = mesure[cle]
        total = (mesure["voix"] - mesure["fin"]) * 1000
        log.info("Latence : %s → total %.0f ms%s", " · ".join(etapes), total, "" if total < 1500 else " (trop lent)")

    def parler(self, texte, mesure: dict | None = None) -> bool:
        """Dit la réponse phrase par phrase, dès que chaque morceau de voix est prêt (Jarvis parle pendant que
        l'IA écrit encore). `texte` : la réponse entière, ou des phrases qui arrivent au fur et à mesure.
        Le micro reste ouvert : « Hey Jarvis » le coupe net. Ta voix (0,3 s) ne le coupe PAS net : il finit
        d'écouter ta phrase en continuant de parler, puis se tait ; ta phrase transcrite est dans
        `self.phrase_coupure`. Renvoie True s'il a été coupé ; ce qu'il a vraiment dit est dans
        `self.dit_avant_coupure`."""
        from collections import deque

        import numpy as np
        from core.audio import BLOC, FREQ, niveau
        from core.sortie import Lecteur
        if self.lecteur is None:
            self.lecteur = Lecteur()
        lecteur, voix = self.lecteur, self.voix
        en_direct = not isinstance(texte, str)
        blocs = texte if en_direct else [texte]
        arret, fini = threading.Event(), threading.Event()
        lecteur.reinitialiser()

        def preparer():
            try:
                for bloc in blocs:
                    for morceau in self.decouper_phrases(bloc) or [bloc]:
                        if arret.is_set():
                            return
                        i = lecteur.nouvelle_phrase(morceau)
                        for audio, freq in voix.flux(morceau):
                            if arret.is_set():
                                return
                            if mesure is not None:
                                mesure.setdefault("son", time.perf_counter())
                            lecteur.ajouter(i, audio, freq)
                        lecteur.fin_phrase(i)
            except Exception:  # noqa: BLE001
                log.exception("synthèse de la voix")
            finally:
                fini.set()

        threading.Thread(target=preparer, daemon=True, name="voix").start()
        m, conv = self.cfg["micro"], self.cfg.get("conversation", {})
        ecoute = bool(self.micro and self.micro.flux.active and m["interrompre"])
        couper = ecoute and bool(m.get("couper_en_parlant", True))
        duree_min = float(conv.get("couper_apres", 0.3))
        seuil_pendant = float(conv.get("seuil_voix_pendant", 0.85))   # plus exigeant quand il parle (écho)
        volume = float(m["volume_conversation"])
        fin_silence = max(float(m["silence_fin"]), 0.7)               # fin de ta phrase, quand tu parles par-dessus
        duree_max = float(m["duree_max"])
        recents, duree_bloc = deque(maxlen=int(2.5 * FREQ / BLOC)), BLOC / FREQ
        self.coupure, interrompu, affichee, voix_entendue = None, False, None, 0.0
        self.phrase_coupure, self.dit_au_debut = None, ""
        # Tu lui parles par-dessus : il NE s'arrête PAS net. Il finit d'écouter ta phrase en continuant de parler,
        # la transcrit, et seulement alors il se tait (s'il n'y avait que du bruit, il continue comme si de rien).
        par_dessus, ta_phrase, silence, parole = False, [], 0.0, 0.0
        if couper:
            self.micro.vad.reinitialiser()
        try:
            while not self.etat.quitter.is_set():
                parle_encore = not (fini.is_set() and not lecteur.occupe())
                if not parle_encore and not par_dessus:
                    break
                en_cours = lecteur.phrase_en_cours() if parle_encore else None
                if en_cours and en_cours[0] != affichee:          # l'île montre la phrase qu'il dit
                    affichee = en_cours[0]
                    self.etat.changer("parle_ecoute" if par_dessus else "parle", en_cours[1], duree=en_cours[2])
                if not ecoute:
                    time.sleep(0.03)
                    continue
                bloc = self.micro.bloc(0.1)
                if bloc is None:
                    continue
                if not par_dessus and self.mot.score(bloc) >= max(self.mot.seuil, 0.7):
                    interrompu = True
                    log.info("Coupé par « Hey Jarvis »")
                    break
                if not couper:
                    continue
                recents.append(bloc)
                p, n = self.micro.vad.proba(bloc), niveau(bloc)
                joue = lecteur.son_recent(0.3)
                if not par_dessus:
                    est_voix = p >= (seuil_pendant if joue else 0.6) and n >= (volume * 1.2 if joue else volume)
                    voix_entendue = voix_entendue + duree_bloc if est_voix else max(0.0, voix_entendue - duree_bloc)
                    if voix_entendue >= duree_min - 1e-6:
                        par_dessus, ta_phrase, silence, parole = True, list(recents), 0.0, voix_entendue
                        self.dit_au_debut = lecteur.dit()
                        log.info("Tu me parles par-dessus : je finis ma phrase et je t'écoute")
                        self.etat.changer("parle_ecoute" if parle_encore else "parole")
                    continue
                # ta phrase continue : un peu moins exigeant qu'au début, mais plus que sans son (l'écho)
                ta_phrase.append(bloc)
                parole += duree_bloc
                fort = p >= (0.5 if joue else 0.35) and n >= (volume if joue else volume * 0.5)
                silence = 0.0 if fort else silence + duree_bloc
                if not parle_encore and self.etat.etat == "parle_ecoute":
                    self.etat.changer("parole")                  # il a fini de parler, il t'écoute encore
                if silence < fin_silence and parole < duree_max:
                    continue
                texte = ""
                try:
                    texte = self.stt.transcrire(np.concatenate(ta_phrase).astype(np.float32) / 32768.0).strip()
                except Exception:  # noqa: BLE001
                    log.exception("transcription pendant qu'il parle")
                if texte:
                    interrompu, self.coupure, self.phrase_coupure = True, ta_phrase, texte
                    log.info("Ta phrase, entendue pendant que je parlais : %s", texte)
                    break
                # rien de compris (un bruit) : il continue comme si de rien n'était
                log.info("Bruit pendant qu'il parlait : il continue")
                par_dessus, voix_entendue = False, 0.0
                recents.clear()
                if parle_encore:
                    self.etat.changer("parle")                   # l'île repasse en « Jarvis parle »
        finally:
            arret.set()
            if interrompu:
                lecteur.vider()                  # stop immédiat du son
            if mesure is not None and lecteur.premier_son is not None:
                mesure["voix"] = lecteur.premier_son
            self.dit_avant_coupure = lecteur.dit()
        if self.micro:
            self.micro.vider()
        if self.mot:
            self.mot.reinitialiser()
        return interrompu

    # ── mode texte ───────────────────────────────────────────
    def texte(self) -> int:
        self.charger_cerveau()
        self.charger_voix(avec_micro=False, avec_stt=False)
        print("\nÉcris ce que tu veux dire à Jarvis (Entrée vide pour quitter). Les actions sont réelles.")
        while (texte := input("> ").strip()):
            rep = self.cerveau.repondre(texte, "texte")
            print(f"Jarvis : {rep.texte}")
            self.voix.dire(rep.texte)
        return 0

    # ── mode test ────────────────────────────────────────────
    def test(self) -> int:
        import numpy as np
        from core.audio import BLOC
        from core.wakeword import MotActivation

        self.charger_cerveau()
        self.charger_voix(avec_micro=False)
        index = getattr(self.ctx, "index_applis", None)
        if index:
            index.pret.wait(20)
        print(f"\nGivre : {'connecté' if self.givre.disponible() else 'pas lancé'} ; "
              f"modèle : {self.cfg['cerveau']['modele']} ; outils : {len(self.reg.outils)}")

        mot = MotActivation(float(self.cfg["micro"]["seuil_activation"]))
        son = self.voix.audio_16k("Hey Jarvis!")
        pcm = (np.concatenate([np.zeros(16000, np.float32), son, np.zeros(16000, np.float32)]) * 32767).astype(np.int16)
        meilleur = max(mot.score(pcm[i:i + BLOC]) for i in range(0, len(pcm) - BLOC, BLOC))
        print(f"Mot d'activation (voix de synthèse) : score {meilleur:.2f}")

        t = time.time()
        texte = self.stt.transcrire(self.voix.audio_16k("Hey Jarvis, lance-moi Discord."))
        print(f"Transcription ({time.time() - t:.2f} s) : « {texte} »")

        cas = [
            ("Hey Jarvis, lance-moi Discord", ["lancer_application"]),
            ("Apprends ma leçon de maths", ["apprendre_lecon"]),
            ("Fais-moi réciter ma leçon de maths", ["reciter_lecon"]),
            ("Mets un minuteur de 5 minutes pour les pâtes", ["minuteur"]),
            ("Mets la vidéo en pause", ["media"]),
            ("Baisse le son", ["volume"]),
            ("Ouvre mes mails", ["ouvrir_givre"]),
            ("Cherche des tutos Blender sur YouTube", ["recherche_web"]),
            ("Réveille-moi à 7h30", ["alarme"]),
            ("Lance le chrono", ["chrono"]),
            ("Quelle heure est-il ?", ["dire_heure"]),
            ("Ouvre YouTube", ["ouvrir_site"]),
            ("Réponds à Tom que j'arrive dans cinq minutes", ["repondre_discord"]),
            ("Ouvre Chrome et mets un minuteur de 10 minutes", [("lancer_application", "ouvrir_site"), "minuteur"]),
            ("Mets la musique un peu plus fort", ["volume"]),
            ("C'est quoi la chanson qui passe ?", ["media"]),
            ("Change de voix, prends une voix d'homme", ["changer_voix"]),
            ("Qu'est-ce qu'il y a sur mon écran ?", ["regarder_ecran"]),
            ("Regarde cette vidéo YouTube, je n'ai pas compris", ["regarder_ecran"]),
            ("Clique sur le bouton S'abonner", ["cliquer_sur"]),
            ("Écris bonjour tout le monde", ["taper_texte"]),
            ("Appuie sur entrée", ["appuyer_touches"]),
            ("Fais contrôle C", ["appuyer_touches"]),
            ("Descends un peu", ["defiler"]),
            ("Envoie un message à Claude qui dit salut ça va", ["repondre_discord"]),
            ("Envoie salut à Matt", ["repondre_discord"]),
            ("Envoie un mail à mon prof", []),
            ("Ajoute payer les factures à mes tâches", ["ajouter_tache"]),
            ("Rappelle-moi dans 20 minutes de sortir le chien", ["ajouter_rappel"]),
            ("J'ai fait mes 20 pompes", ["cocher_habitude"]),
            ("Lance la routine du matin", ["lancer_routine"]),
            ("Souviens-toi que mon meilleur ami s'appelle Tom", ["retenir"]),
            ("Qu'est-ce que tu sais sur moi ?", ["ce_que_tu_sais"]),
            ("Ouvre l'application Jarvis", ["ouvrir_application_jarvis"]),
            ("Montre mes tâches", ["ouvrir_application_jarvis"]),
            ("Es-tu connecté à Pronote ?", ["etat_pronote"]),
            ("Quelle note j'ai en maths ?", []),
            ("Comment va mon PC ?", ["etat_pc"]),
            ("C'est quoi la batterie de ma manette ?", ["batteries"]),
            ("Dis-moi ce que Claude fait", ["activite_claude"]),
            ("Fais sonner mon téléphone", ["faire_sonner_telephone"]),
            ("Où est mon portable ?", ["faire_sonner_telephone"]),
            ("Qui m'a appelé ?", []),
            ("J'ai reçu des SMS ?", []),
            ("Envoie un message à Claude qui dit qu'il a fait ses devoirs", ["repondre_discord"]),
            ("Mets la playlist Electro Chill", ["jouer_deezer"]),
            ("Joue la chanson Bande organisée sur Deezer", ["jouer_deezer"]),
            ("Résume-moi le mail d'AliExpress", ["resumer_mail"]),
            ("À propos du mail n°2 (de Emergent, « Pet Focus Quest ») : fais-moi un résumé", ["resumer_mail"]),
            ("Raconte-moi une blague", []),
        ]
        reussis = 0
        for question, attendu in cas:
            self.ctx.appels.clear()
            t = time.time()
            rep = self.cerveau.repondre(question, "test")
            outils = [n for n, _ in self.ctx.appels]
            # un tuple = « l'un ou l'autre » (ouvrir_site relance l'appli si c'en est une)
            ok = all((any(x in outils for x in a) if isinstance(a, tuple) else a in outils) for a in attendu) \
                and (bool(attendu) or not outils)
            reussis += ok
            print(f"{'OK ' if ok else 'RATÉ'} {time.time() - t:4.1f} s | {question}\n"
                  f"       -> {self.ctx.appels or 'aucune action'} | « {rep.texte[:110]} »")
        t = time.time()
        audio, freq = self.voix.synthetiser("Test terminé.")
        print(f"Voix : {len(audio) / freq:.1f} s d'audio en {time.time() - t:.2f} s")
        print(f"\n{reussis}/{len(cas)} commandes comprises.")
        return 0 if reussis >= len(cas) - 1 else 1


def main():
    p = argparse.ArgumentParser(description="Jarvis, assistant vocal local")
    p.add_argument("--service", action="store_true", help="sans console (utilisé par Jarvis.exe)")
    p.add_argument("--clavier", action="store_true", help="Entrée pour parler")
    p.add_argument("--texte", action="store_true", help="écrire au lieu de parler")
    p.add_argument("--test", action="store_true", help="test complet sans micro, actions simulées")
    p.add_argument("--lanceur", type=int, default=0, help=argparse.SUPPRESS)   # PID de Jarvis.exe
    args = p.parse_args()
    journal.configurer(sans_console=args.service)
    # un plantage « dur » (dans une bibliothèque en C) ne laisse rien dans jarvis.log : on le note ici
    import faulthandler
    global _plantages
    try:
        _plantages = open(journal.FICHIER.with_name("plantage.log"), "a", encoding="utf-8")
        faulthandler.enable(_plantages)
    except OSError:
        pass
    code = 0
    try:
        if args.test:
            code = Jarvis(args).test()
        elif args.texte:
            code = Jarvis(args).texte()
        elif not instance_unique():
            log.info("Jarvis tourne déjà.")
        else:
            code = Jarvis(args).service()
    except KeyboardInterrupt:
        log.info("À plus !")
    except Exception:  # noqa: BLE001
        log.exception("Jarvis s'est arrêté sur une erreur")
        code = 1
    logging.shutdown()
    os._exit(code)


if __name__ == "__main__":
    main()
