// Tourne dans l'onglet Discord : transmet les nouveaux messages à Jarvis,
// et écrit / envoie un message quand Jarvis le demande (après ton « oui »).

const pause = (ms) => new Promise((r) => setTimeout(r, ms));
const normaliser = (s) => (s || "").normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/\s+/g, " ").trim();

// 1) Nouveaux messages : discord-page.js nous passe chaque notification de Discord.
window.addEventListener("message", (e) => {
  if (e.source !== window || !e.data || e.data.__jarvis !== "discord_notif") return;
  const titre = e.data.titre.trim();
  // MP : « Tom » ; groupe : « Tom (Les potes) » ; serveur : « Tom (#général, Mon serveur) »
  const m = titre.match(/^(.*?)\s*\(([^()]*)\)\s*$/);
  const auteur = m ? m[1] : titre;
  const salon = m ? m[2] : "";
  chrome.runtime.sendMessage({
    type: "discord_message", auteur, texte: e.data.texte, salon, mp: !salon.startsWith("#"),
  }).catch(() => {});
});

// 2) Envoyer un message demandé par Jarvis.
chrome.runtime.onMessage.addListener((m, _expediteur, repondre) => {
  if (!m || (m.type !== "discord_envoyer" && m.type !== "discord_appeler")) return;
  const action = m.type === "discord_appeler" ? appeler(m.destinataire) : envoyer(m.destinataire, m.texte);
  action.then(repondre, (e) => repondre({ ok: false, erreur: String((e && e.message) || e) }));
  return true; // réponse asynchrone
});

// 3) Appel vocal demandé par Jarvis (« appelle Tom sur Discord », après ton « oui ») :
//    on ouvre la conversation privée puis on clique sur le bouton d'appel vocal de Discord.
function boutonAppel() {
  return [...document.querySelectorAll('[role="button"][aria-label], button[aria-label]')].find((b) => {
    const nom = normaliser(b.getAttribute("aria-label") || "");
    return (nom.includes("appel vocal") || nom.includes("voice call")) && !nom.includes("video");
  });
}

async function appeler(destinataire) {
  if (!destinataire) return { ok: false, erreur: "il manque le nom de la personne" };
  const lien = await trouverConversation(destinataire);
  if (!lien) return { ok: false, erreur: `je ne trouve pas la conversation avec ${destinataire} dans tes messages privés` };
  const adresse = lien.getAttribute("href");
  lien.click();
  const arrive = await attendre(() => location.pathname === adresse, 4000);
  if (!arrive) return { ok: false, erreur: "Discord n'a pas ouvert la conversation" };
  await pause(400);
  const bouton = await attendre(boutonAppel, 5000);
  if (!bouton) return { ok: false, erreur: "je ne trouve pas le bouton d'appel de Discord" };
  bouton.click();
  return { ok: true };
}

async function attendre(trouver, delai) {
  const fin = Date.now() + delai;
  while (Date.now() < fin) {
    const x = trouver();
    if (x) return x;
    await pause(150);
  }
  return null;
}

function liensConversations() {
  return [...document.querySelectorAll('a[href^="/channels/@me/"]')];
}

function nomDuLien(a) {
  return normaliser((a.getAttribute("aria-label") || "") + " " + (a.textContent || ""));
}

async function trouverConversation(nom) {
  const cible = normaliser(nom);
  const chercher = () => liensConversations().find((a) => nomDuLien(a).includes(cible));
  let lien = chercher();
  if (!lien) {
    // Pas visible : on passe sur la liste des messages privés (accueil de Discord).
    const accueil = document.querySelector('a[href="/channels/@me"]') ||
      document.querySelector('[data-list-item-id="guildsnav___home"]');
    if (accueil) {
      accueil.click();
      lien = await attendre(chercher, 4000);
    }
  }
  return lien;
}

function zoneDeSaisie() {
  return document.querySelector('[role="textbox"][data-slate-editor="true"]') ||
    document.querySelector('div[role="textbox"][contenteditable="true"]');
}

async function envoyer(destinataire, texte) {
  if (!texte) return { ok: false, erreur: "message vide" };
  if (destinataire) {
    const lien = await trouverConversation(destinataire);
    if (!lien) return { ok: false, erreur: `je ne trouve pas la conversation avec ${destinataire} dans tes messages privés` };
    const adresse = lien.getAttribute("href");
    lien.click();
    const arrive = await attendre(() => location.pathname === adresse, 4000);
    if (!arrive) return { ok: false, erreur: "Discord n'a pas ouvert la conversation" };
  }
  const zone = await attendre(zoneDeSaisie, 5000);
  if (!zone) return { ok: false, erreur: "zone de saisie de Discord introuvable" };
  await pause(300);
  zone.focus();
  const debut = texte.slice(0, 12);
  document.execCommand("insertText", false, texte);
  await pause(200);
  if (!zone.textContent.includes(debut)) {
    // Autre méthode : « coller » le texte, comme un Ctrl+V.
    const donnees = new DataTransfer();
    donnees.setData("text/plain", texte);
    zone.dispatchEvent(new ClipboardEvent("paste", { clipboardData: donnees, bubbles: true, cancelable: true }));
    await pause(200);
  }
  if (!zone.textContent.includes(debut)) return { ok: false, erreur: "je n'arrive pas à écrire dans Discord" };
  zone.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true, cancelable: true }));
  const vide = await attendre(() => !zoneDeSaisie() || !zoneDeSaisie().textContent.includes(debut), 3000);
  return vide ? { ok: true } : { ok: false, erreur: "le message est écrit mais pas parti : appuie sur Entrée dans Discord" };
}
