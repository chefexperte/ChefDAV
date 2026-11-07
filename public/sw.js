// ChefDAV service worker: schedules reminder notifications for upcoming
// events. The page sends {type:"schedule-reminders", reminders:[...]}; the
// worker shows a notification at the requested time while it stays alive.
// (True delivery while the browser is fully closed would need Web Push and a
// push server; this is the client-only best effort.)

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));

let timers = [];

self.addEventListener("message", (event) => {
  const data = event.data || {};
  if (data.type !== "schedule-reminders") return;
  for (const t of timers) clearTimeout(t);
  timers = [];
  for (const reminder of data.reminders || []) {
    const delay = new Date(reminder.when).getTime() - Date.now();
    if (delay > 0 && delay < 24 * 60 * 60 * 1000) {
      timers.push(
        setTimeout(() => {
          self.registration.showNotification(reminder.title, {
            body: reminder.body,
            tag: reminder.tag,
            icon: "/logo.svg",
            badge: "/logo.svg",
          });
        }, delay)
      );
    }
  }
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil(self.clients.openWindow("/"));
});
