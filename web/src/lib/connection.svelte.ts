// Whether the server is reachable. Offline, the service worker answers reads from its
// cache (marked with `X-Streamline-Offline`), so the app shows the last data read-only.
// In the desktop app, `pending` counts changes kept on the computer until the server is back (9.5).
export const connection = $state({ offline: false, pending: 0 })
