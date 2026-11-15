// Tourne dans chaque page : contrôle la vidéo de la page quand Jarvis le demande.
chrome.runtime.onMessage.addListener((m, _expediteur, repondre) => {
  if (!m || m.type !== "video") return;
  const v = choisirVideo();
  if (!v) return; // pas de vidéo ici : un autre onglet répondra
  const valeur = Number(m.valeur);
  switch (m.action) {
    case "pause": v.pause(); break;
    case "lecture": v.play().catch(() => {}); break;
    case "basculer": v.paused ? v.play().catch(() => {}) : v.pause(); break;
    case "avancer": v.currentTime = Math.min(isFinite(v.duration) ? v.duration - 0.5 : Infinity, v.currentTime + (valeur || 10)); break;
    case "reculer": v.currentTime = Math.max(0, v.currentTime - (valeur || 10)); break;
    case "aller": v.currentTime = Math.max(0, valeur || 0); break;
    case "vitesse": v.playbackRate = Math.min(4, Math.max(0.25, valeur || 1)); break;
    case "muet": v.muted = !v.muted; break;
    case "suivant": {
      const bouton = document.querySelector(".ytp-next-button");
      if (!bouton) { repondre({ ok: false, erreur: "pas de vidéo suivante ici" }); return; }
      bouton.click();
      break;
    }
    case "infos": break;
    default: repondre({ ok: false, erreur: "action inconnue" }); return;
  }
  repondre({ ok: true, titre: document.title, temps: Math.round(v.currentTime), duree: Math.round(v.duration || 0),
             pause: v.paused, vitesse: v.playbackRate });
});

function choisirVideo() {
  const videos = [...document.querySelectorAll("video")].filter((v) => v.readyState > 0);
  if (!videos.length) return null;
  const enCours = videos.find((v) => !v.paused);
  if (enCours) return enCours;
  return videos.sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
}
