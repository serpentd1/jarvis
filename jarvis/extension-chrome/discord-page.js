// Tourne DANS la page Discord (avant elle) : quand Discord affiche une notification
// (nouveau message), on en garde une copie pour Jarvis. Rien n'est envoyé ailleurs que sur ton PC.
(() => {
  const Vraie = window.Notification;
  if (!Vraie || Vraie.__jarvis) return;

  function Notif(titre, options) {
    try {
      window.postMessage({
        __jarvis: "discord_notif",
        titre: String(titre || ""),
        texte: String((options && options.body) || ""),
      }, location.origin);
    } catch (e) { /* rien */ }
    if (Vraie.permission === "granted") return new Vraie(titre, options);
    // Chrome n'a pas le droit d'afficher les notifications : on fait semblant, sans rien afficher.
    return { close() {}, addEventListener() {}, removeEventListener() {}, onclick: null, onclose: null, onshow: null, onerror: null };
  }
  Notif.prototype = Vraie.prototype;
  Notif.requestPermission = (...a) => Vraie.requestPermission(...a);
  // Discord ne crée des notifications que s'il croit en avoir le droit.
  Object.defineProperty(Notif, "permission", { get: () => "granted" });
  Notif.__jarvis = true;
  window.Notification = Notif;
})();
