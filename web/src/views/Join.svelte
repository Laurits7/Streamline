<script lang="ts">
  // An invite link opened in a browser (Phase 8): it's meant for the invitee's own
  // Streamline, so explain where to paste it.
  const link = location.href
  async function copy() {
    try {
      await navigator.clipboard.writeText(link)
      copied = true
    } catch {
      /* the field can be copied by hand */
    }
  }
  let copied = $state(false)
</script>

<main class="join">
  <div class="card">
    <h1>You're invited to a shared project</h1>
    <p>
      This link lets you work on a project of {location.host} from your own Streamline. Open your Streamline, go to
      <strong>Settings → Projects from other servers</strong>, and paste it there.
    </p>
    <div class="row">
      <input type="text" readonly value={link} aria-label="Invite link" onfocus={(e) => (e.currentTarget as HTMLInputElement).select()} />
      <button class="btn primary" onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
    </div>
    <p class="muted">It works once. If you have an account on {location.host} itself, ask to be added to the project there instead.</p>
  </div>
</main>

<style>
  .join {
    min-height: 100vh;
    display: grid;
    place-items: center;
    padding: 16px;
    background: var(--bg);
  }
  .card {
    max-width: 520px;
    width: 100%;
    padding: 24px;
  }
  h1 {
    font-size: 20px;
    margin-bottom: 12px;
  }
  p {
    margin: 0 0 14px;
    line-height: 1.5;
  }
  .row {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }
  .row input {
    flex: 1;
    min-width: 0;
  }
  .muted {
    font-size: 13px;
  }
</style>
