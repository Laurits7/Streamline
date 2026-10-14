<script lang="ts">
  // Focus options (D-82): mute the speakers while working, and the timer on every screen.
  // Both off by default; the last choice is the next default. The desktop app applies them
  // on every computer it runs on, whichever device started the timer.
  import { desktop } from '../desktop'
  import { store } from '../store.svelte'

  const o = $derived(store.focusTimer.phase === 'idle' ? store.focusOptions() : store.focusTimer)
</script>

<fieldset class="opts">
  <legend class="sr-only">Focus options</legend>
  <label class="check plain">
    <input type="checkbox" checked={o.mute} onchange={(e) => store.setFocusOptions({ mute: (e.currentTarget as HTMLInputElement).checked })} />
    Mute speakers while working
  </label>
  <label class="screens">
    <span>Timer on every screen</span>
    <select value={o.screens} onchange={(e) => store.setFocusOptions({ screens: (e.currentTarget as HTMLSelectElement).value as typeof o.screens })}>
      <option value="off">Off</option>
      <option value="overlay">Small, in a corner</option>
      <option value="fullscreen">Full screen</option>
      <option value="fullscreen_breaks">Full screen on breaks</option>
    </select>
  </label>
  {#if !desktop && (o.mute || o.screens !== 'off')}
    <p class="muted note">Needs the desktop app: computers running it apply these, whichever device started the timer.</p>
  {:else if !desktop}
    <p class="muted note">These need the Streamline desktop app.</p>
  {/if}
</fieldset>

<style>
  .opts {
    border: 0;
    margin: 12px auto 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 10px 18px;
    font-size: 14px;
  }
  .screens {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .screens span {
    color: var(--muted);
  }
  .note {
    flex-basis: 100%;
    text-align: center;
    font-size: 12px;
    margin: 0;
  }
</style>
