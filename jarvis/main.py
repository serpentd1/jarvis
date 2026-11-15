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

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")

from core import config, journal  # noqa: E402

log = logging.getLogger("jarvis")
CODE_REDEMARRER = 3
_mutex = None


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
        self.voix = self.stt = self.micro = self.mot = None
        self.ctx = types.SimpleNamespace(cfg=self.cfg, givre=self.givre, pont=self.pont, etat=self.etat,
                                         simulation=bool(args.test), appels=[])

    # ── chargement ───────────────────────────────────────────
    def charger_cerveau(self):
        import modules
        from core.cerveau import Cerveau
        self.reg = modules.charger(self.ctx)
        self.cerveau = Cerveau(self.cfg["cerveau"], self.cfg["general"]["nom"], self.reg)
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
            log.info("Transcription prête sur %s (%.0f s)", self.stt.appareil, time.time() - t)
        if avec_micro:
            from core.audio import Micro
            from core.wakeword import MotActivation
            self.micro = Micro(self.cfg["micro"]["appareil"])
            self.mot = MotActivation(float(self.cfg["micro"]["seuil_activation"]))

    def changer_voix(self, nom: str):
        """Change la voix (la télécharge si besoin) et la garde dans config.toml."""
        self.voix.changer(nom)
        self.cfg["voix"]["voix"] = nom
        try:
            config.ecrire_valeur("voix", "voix", nom)
        except Exception:  # noqa: BLE001
            log.exception("config.toml non modifié")

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
        self.etat.abonner(lambda e, t, q, r: self.givre.etat_jarvis(e, t, q, r))
        icone = None
        try:
            from core.icone import Icone
            icone = Icone(self.etat, self.givre, self.cfg["general"],
                          voix_actuelle=lambda: self.voix.nom if self.voix else None,
                          changer_voix=lambda nom: threading.Thread(target=self._changer_voix_et_dire, args=(nom,),
                                                                    daemon=True).start())
            icone.demarrer()
        except Exception:  # noqa: BLE001
            log.exception("icône indisponible")
        g = self.cfg["general"]
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
        micro_ouvert = False
        while not self.etat.quitter.is_set():
            if self.micro and self.etat.micro != micro_ouvert:
                (self.micro.demarrer if self.etat.micro else self.micro.pause)()
                micro_ouvert = self.etat.micro
                self.mot.reinitialiser()
                log.info("Micro %s", "allumé" if micro_ouvert else "coupé")
                self.etat.changer(self.etat.etat)
            if self.voix and not self.etat.annonces.empty():
                phrase = self.etat.annonces.get_nowait()
                self.etat.changer("parle", phrase)
                interrompu = self.parler(phrase)
                self.etat.changer("repos")
                if interrompu:
                    self.ecouter()
                elif micro_ouvert and self.etat.annonces.empty() and float(self.cfg["micro"]["conversation"]) > 0:
                    self.ecouter("conversation")      # « réponds-lui que… » sans redire « Hey Jarvis »
                continue
            if self.etat.ecouter_maintenant.is_set():
                self.etat.ecouter_maintenant.clear()
                self.ecouter()
                continue
            if not micro_ouvert:
                time.sleep(0.2)
                continue
            bloc = self.micro.bloc(0.25)
            if bloc is not None and self.mot.entendu(bloc):
                log.info("« Hey Jarvis » entendu")
                self.mot.reinitialiser()
                self.ecouter()

    def ecouter(self, mode: str = "normal"):
        """mode : "normal" (après « Hey Jarvis »), "question" (Jarvis attend oui/non),
        "conversation" (suite de la discussion, sans redire « Hey Jarvis »)."""
        if not (self.micro and self.stt and self.cerveau):
            return
        while mode and not self.etat.quitter.is_set():
            mode = self._un_echange(mode)

    def _un_echange(self, mode: str):
        """Écoute une phrase, répond, et dit comment continuer (ou None pour revenir à « Hey Jarvis »)."""
        from core.audio import ding
        from core.cerveau import enlever_mot_activation
        m = self.cfg["micro"]
        conversation = float(m["conversation"])
        deja_ouvert = self.micro.flux.active
        self.micro.demarrer()
        try:
            self.etat.changer("ecoute", "Je t'écoute…")
            if m["ding"] and mode != "conversation":
                ding(aigu=mode == "question")
            audio = self.micro.enregistrer_commande(
                float(m["silence_fin"]), float(m["duree_max"]),
                conversation if mode == "conversation" else float(m["attente_debut"]),
                ignorer_debut=0.0 if mode == "conversation" else 0.3)
        finally:
            if not deja_ouvert:
                self.micro.pause()
        if not len(audio):
            self.etat.changer("repos")
            return None
        self.etat.changer("reflechit")
        texte = self.stt.transcrire(audio)
        if not texte.strip():
            log.info("(rien compris)")
            self.etat.changer("repos")
            return None
        log.info("Toi    : %s", texte)
        self.etat.changer("reflechit", texte)
        rep = self.cerveau.repondre(texte, "voix")
        log.info("Jarvis : %s", rep.texte)
        self.etat.changer("parle", rep.texte, question=enlever_mot_activation(texte) or texte, reponse=rep.texte)
        interrompu = self.parler(rep.texte)
        self.etat.changer("repos")
        if interrompu:
            return "normal"
        if rep.suivi:
            return "question"
        return "conversation" if conversation > 0 else None

    def parler(self, texte: str) -> bool:
        """Dit la phrase. Renvoie True si on l'a coupé en disant « Hey Jarvis »."""
        import sounddevice as sd
        audio, freq = self.voix.synthetiser(texte)
        sd.play(audio, freq)
        fin = time.time() + len(audio) / freq + 0.1
        ecoute = bool(self.micro and self.micro.flux.active and self.cfg["micro"]["interrompre"])
        interrompu = False
        while time.time() < fin and not self.etat.quitter.is_set():
            if ecoute:
                bloc = self.micro.bloc(0.1)
                if bloc is not None and self.mot.score(bloc) >= max(self.mot.seuil, 0.7):
                    interrompu = True
                    log.info("Coupé par « Hey Jarvis »")
                    break
            else:
                time.sleep(0.05)
        sd.stop()
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
