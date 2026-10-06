<script lang="ts">
  // Sets the current place from GPS when the user enabled it on this device and the
  // browser allows it (secure contexts only, D-45). Otherwise places are picked by hand.
  import { placeAt } from '../places'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'

  $effect(() => {
    if (!store.useGps) return
    if (!window.isSecureContext || !navigator.geolocation) {
      toast('GPS needs the app to be opened over HTTPS. Pick your place by hand instead.', 'error')
      store.setUseGps(false)
      return
    }
    const id = navigator.geolocation.watchPosition(
      (pos) => {
        const p = placeAt(store.placeList(), pos.coords.latitude, pos.coords.longitude, pos.coords.accuracy)
        const next = p?.id ?? ''
        if (next !== store.currentPlace) store.setCurrentPlace(next)
      },
      (err) => {
        if (err.code === err.PERMISSION_DENIED) {
          toast('Location access was denied, so GPS is off. Pick your place by hand.', 'error')
          store.setUseGps(false)
        }
      },
      { enableHighAccuracy: false, maximumAge: 60_000, timeout: 30_000 },
    )
    return () => navigator.geolocation.clearWatch(id)
  })
</script>
