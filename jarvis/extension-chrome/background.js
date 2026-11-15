// Jarvis — pont Chrome : relie l'extension à Jarvis (ws://127.0.0.1:47801/extension).
// Jarvis envoie des commandes ({commande, id}) ; on répond {type: "reponse", id, ok, ...}.
// Les onglets (Discord, vidéos) nous envoient des événements qu'on transmet à Jarvis.

const ADRESSE = "ws://127.0.0.1:47801/extension";
let ws = null;

function connecter() {
  if (ws && (ws.readyState === WebSocket.CONNECTING || ws.readyState === WebSocket.OPEN)) return;
  try {
    ws = new WebSocket(ADRESSE);
  } catch (e) {
    ws = null;
    return;
  }
  ws.onopen = () => {
    envoyer({ type: "bonjour", version: chrome.runtime.getManifest().version });
    chrome.action.setBadgeText({ text: "" });
    chrome.action.setTitle({ title: "Jarvis : connecté" });
  };
  ws.onclose = () => {
    ws = null;
    chrome.action.setBadgeText({ text: "off" });
    chrome.action.setBadgeBackgroundColor({ color: "#888" });
    chrome.action.setTitle({ title: "Jarvis : pas lancé" });
  };
  ws.onerror = () => {};
  ws.onmessage = async (ev) => {
    let m;
    try { m = JSON.parse(ev.data); } catch (e) { return; }
    if (m.type === "ping") return envoyer({ type: "pong" });
    if (!m.commande) return;
    let rep;
    try {
      rep = await traiter(m);
    } catch (e) {
      rep = { ok: false, erreur: String((e && e.message) || e) };
    }
    envoyer({ type: "reponse", id: m.id, ...rep });
  };
}

function envoyer(objet) {
  if (ws && ws.readyState === WebSocket.OPEN) ws.send(JSON.stringify(objet));
}

// Réveil régulier : se reconnecte si Jarvis a redémarré.
chrome.alarms.create("jarvis", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener(connecter);
chrome.runtime.onStartup.addListener(connecter);
chrome.runtime.onInstalled.addListener(connecter);
connecter();

// Événements venant des onglets (nouveau message Discord…)
chrome.runtime.onMessage.addListener((msg) => {
  connecter();
  if (msg && msg.type === "discord_message") envoyer(msg);
});

// ── commandes de Jarvis ─────────────────────────────────────
async function traiter(m) {
  if (m.commande === "video") return await commandeVideo(m);
  if (m.commande === "discord_envoyer") return await commandeDiscord(m);
  if (m.commande === "onglets") {
    const onglets = await chrome.tabs.query({});
    return { ok: true, onglets: onglets.map((o) => ({ titre: o.title, url: o.url, son: o.audible })) };
  }
  return { ok: false, erreur: "commande inconnue : " + m.commande };
}

async function commandeVideo(m) {
  // L'onglet qui fait du bruit d'abord, puis l'onglet affiché, puis les autres.
  const bruyants = await chrome.tabs.query({ audible: true });
  const actifs = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  const tous = await chrome.tabs.query({});
  const vus = new Set();
  const ordre = [...bruyants, ...actifs, ...tous].filter((o) => !vus.has(o.id) && vus.add(o.id) && /^https?:/.test(o.url || ""));
  for (const onglet of ordre.slice(0, 15)) {
    try {
      const rep = await chrome.tabs.sendMessage(onglet.id, { type: "video", action: m.action, valeur: m.valeur });
      if (rep && rep.ok) return { ...rep, onglet: onglet.title };
    } catch (e) {
      // pas de vidéo dans cet onglet (ou page sans l'extension) : onglet suivant
    }
  }
  return { ok: false, erreur: "aucune vidéo trouvée dans Chrome" };
}

async function commandeDiscord(m) {
  const onglets = await chrome.tabs.query({ url: "https://discord.com/*" });
  if (!onglets.length) return { ok: false, erreur: "aucun onglet Discord ouvert dans Chrome" };
  try {
    return await chrome.tabs.sendMessage(onglets[0].id, { type: "discord_envoyer", destinataire: m.destinataire, texte: m.texte });
  } catch (e) {
    return { ok: false, erreur: "recharge l'onglet Discord (F5) pour activer l'extension" };
  }
}
