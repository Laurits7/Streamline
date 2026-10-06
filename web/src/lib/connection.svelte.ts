// Whether the server is reachable. Offline, the service worker answers reads from its
// cache (marked with `X-Streamline-Offline`), so the app shows the last data read-only.
export const connection = $state({ offline: false })
