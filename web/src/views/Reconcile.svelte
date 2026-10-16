<script lang="ts">
  // Tick off a printed sheet (7.3, D-14): opened from its QR code, without signing in. It
  // can only mark this sheet's tasks as done.
  import { untrack } from 'svelte'
  import { api, ApiError } from '../lib/api/client'
  import type { Reconcile } from '../lib/api/types/Reconcile'

  let { token }: { token: string } = $props()
  let sheet = $state<Reconcile | null>(null)
  let error = $state('')
  let picked = $state<Set<string>>(new Set())
  let saving = $state(false)
  let saved = $state(false)

  $effect(() => {
    untrack(() =>
      api
        .get<Reconcile>(`/reconcile/${token}`)
        .then((s) => (sheet = s))
        .catch((e) => (error = e instanceof ApiError && e.status === 404 ? 'This link has expired or is not valid. Print the day again for a new one.' : "Can't reach Streamline.")),
    )
  })
  const toggle = (code: string) => {
    const next = new Set(picked)
    if (next.has(code)) next.delete(code)
    else next.add(code)
    picked = next
  }
  async function save() {
    saving = true
    try {
      sheet = await api.post<Reconcile>(`/reconcile/${token}`, { codes: [...picked] })
      picked = new Set()
      saved = true
    } catch {
      error = 'Could not save. Try again.'
    } finally {
      saving = false
    }
  }
</script>

<main>
  <div class="card">
    {#if error}
      <h1>Printed sheet</h1>
      <p class="muted">{error}</p>
    {:else if !sheet}
      <p class="muted">Loading…</p>
    {:else}
      <h1>{new Date(sheet.date + 'T12:00:00Z').toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })}</h1>
      <p class="muted">{sheet.name}'s printed sheet. Tick what's done.</p>
      <ul>
        {#each sheet.items as it (it.code)}
          <li class:done={it.done} class:gone={it.gone}>
            <label>
              <input type="checkbox" checked={it.done || picked.has(it.code)} disabled={it.done || it.gone} onchange={() => toggle(it.code)} />
              <span class="t">{#if it.start_time}<b>{it.start_time}</b> {/if}{it.title}</span>
              <span class="code">{it.code}</span>
            </label>
          </li>
        {/each}
      </ul>
      <button class="btn primary" disabled={!picked.size || saving} onclick={save}>
        {saving ? 'Saving…' : picked.size ? `Mark ${picked.size} done` : 'Mark done'}
      </button>
      {#if saved && !picked.size}<p class="ok">Saved.</p>{/if}
    {/if}
  </div>
</main>

<style>
  main {
    min-height: 100dvh;
    display: grid;
    place-items: start center;
    padding: 24px 16px;
  }
  .card {
    width: min(520px, 100%);
    padding: 20px;
  }
  h1 {
    font-size: 22px;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 12px 0;
  }
  li label {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 4px;
    border-bottom: 1px solid var(--border);
    font-size: 16px;
  }
  li input {
    width: 22px;
    height: 22px;
    flex: none;
  }
  li .t {
    flex: 1;
  }
  li.done .t,
  li.gone .t {
    color: var(--muted);
    text-decoration: line-through;
  }
  .code {
    font-family: ui-monospace, monospace;
    font-size: 12px;
    color: var(--muted);
  }
  .ok {
    color: var(--ok, #15803d);
    margin: 8px 0 0;
  }
</style>
