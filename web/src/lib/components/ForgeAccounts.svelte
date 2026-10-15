<script lang="ts">
  // Tasks from GitHub, GitLab and Gitea/Forgejo (7.6, D-97). The token is write-only: the
  // server never sends it back.
  import { api, ApiError } from '../api/client'
  import type { ForgeAccountView } from '../api/types/ForgeAccountView'
  import type { ForgeRule } from '../api/types/ForgeRule'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'

  type Kind = 'github' | 'gitlab' | 'gitea'
  const KINDS: { id: Kind; name: string; placeholder: string; tokenHelp: string }[] = [
    {
      id: 'github',
      name: 'GitHub',
      placeholder: 'https://github.com (or your GitHub Enterprise address)',
      tokenHelp:
        'A fine-grained personal access token (Settings → Developer settings) with read access to issues and pull requests of the repositories you want, or a classic token with “repo”.',
    },
    {
      id: 'gitlab',
      name: 'GitLab',
      placeholder: 'https://gitlab.com (or your own GitLab)',
      tokenHelp: 'A personal access token (Preferences → Access tokens) with the “read_api” scope.',
    },
    {
      id: 'gitea',
      name: 'Gitea / Forgejo',
      placeholder: 'https://codeberg.org',
      tokenHelp: 'An access token (Settings → Applications) with read access to issues and repositories.',
    },
  ]
  const kindName = (k: string) => KINDS.find((x) => x.id === k)?.name ?? k

  let accounts = $state<ForgeAccountView[]>([])
  let loaded = $state(false)
  async function load() {
    try {
      accounts = await api.get<ForgeAccountView[]>('/forges')
    } catch {
      /* offline: the section shows the form only */
    }
    loaded = true
  }
  load()

  const err = (e: unknown) => toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')
  const replace = (a: ForgeAccountView) => (accounts = accounts.map((x) => (x.id === a.id ? a : x)))
  const when = (ts: string | null) =>
    ts ? new Date(ts).toLocaleString(store.me?.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' }) : 'never'

  // Connecting
  let form = $state({ kind: 'github' as Kind, url: '', token: '' })
  const formKind = $derived(KINDS.find((k) => k.id === form.kind)!)
  let busy = $state('')
  let tested = $state<{ ok: boolean; login: string | null; error: string | null } | null>(null)
  const input = () => ({ kind: form.kind, url: form.url.trim(), token: form.token.trim() })
  async function run(what: string, fn: () => Promise<void>) {
    busy = what
    try {
      await fn()
    } catch (e) {
      err(e)
    } finally {
      busy = ''
    }
  }
  const test = () => run('test', async () => void (tested = await api.post('/forges/test', input())))
  function connect(e: Event) {
    e.preventDefault()
    run('connect', async () => {
      const a = await api.post<ForgeAccountView>('/forges', input())
      accounts = [...accounts, a]
      form = { kind: form.kind, url: '', token: '' }
      tested = null
      toast(a.status === 'ok' ? `Connected as ${a.login}` : 'Connected, but the first check failed (see below)', a.status === 'ok' ? 'info' : 'error')
    })
  }

  // Each account
  const patch = (a: ForgeAccountView, body: Record<string, unknown>) =>
    run(a.id, async () => void replace(await api.patch<ForgeAccountView>(`/forges/${a.id}`, body)))
  const sync = (a: ForgeAccountView) => run(a.id, async () => void replace(await api.post<ForgeAccountView>(`/forges/${a.id}/sync`)))
  async function remove(a: ForgeAccountView) {
    if (!confirm(`Remove ${a.login} on ${kindName(a.kind)}? The tasks it brought in stay, as ordinary tasks.`)) return
    await run(a.id, async () => {
      await api.del(`/forges/${a.id}`)
      accounts = accounts.filter((x) => x.id !== a.id)
    })
  }
  let newToken = $state<Record<string, string>>({})
  const saveToken = (a: ForgeAccountView) =>
    patch(a, { token: newToken[a.id] }).then(() => {
      newToken[a.id] = ''
    })

  // Repository rules
  const projects = $derived(store.projectList())
  let newRule = $state<Record<string, ForgeRule>>({})
  $effect(() => {
    for (const a of accounts) if (!newRule[a.id]) newRule[a.id] = { repo: '', project_id: '' }
  })
  function addRule(a: ForgeAccountView) {
    const r = newRule[a.id]
    if (!r?.repo.trim() || !r.project_id) return
    patch(a, { rules: [...a.rules, { repo: r.repo.trim(), project_id: r.project_id }] }).then(() => {
      newRule[a.id] = { repo: '', project_id: '' }
    })
  }
  const removeRule = (a: ForgeAccountView, i: number) => patch(a, { rules: a.rules.filter((_, j) => j !== i) })
  const reasons: { key: 'issues' | 'prs' | 'reviews' | 'mentions'; label: string }[] = [
    { key: 'issues', label: 'Issues assigned to me' },
    { key: 'prs', label: 'Pull/merge requests assigned to me' },
    { key: 'reviews', label: 'Reviews requested from me' },
    { key: 'mentions', label: 'Where I’m mentioned' },
  ]
</script>

<section class="card" id="forges">
  <h2>GitHub, GitLab and others</h2>
  <p class="help muted">
    Bring what's yours on GitHub, GitLab or Gitea/Forgejo in as tasks, with a link back. Checked every 10 minutes; a task is
    completed by itself once its issue is closed, merged or no longer yours. Read-only: Streamline never changes anything
    there.
  </p>

  {#each accounts as a (a.id)}
    <div class="account">
      <div class="top">
        <strong>{kindName(a.kind)}</strong>
        <span class="muted">{a.login} · {a.url.replace(/^https?:\/\//, '')}</span>
      </div>
      <p class="status" class:bad={a.status === 'error'} role="status">
        {#if a.status === 'error'}
          Last check failed: {a.last_error}. Tasks stay as they were.
        {:else if a.status === 'ok'}
          Checked {when(a.last_sync_at)}.
        {:else}Will be checked within a minute.{/if}
      </p>
      <fieldset>
        <legend class="muted">Bring in</legend>
        {#each reasons as r (r.key)}
          <label class="check">
            <input type="checkbox" checked={a[r.key]} disabled={busy === a.id} onchange={(e) => patch(a, { [r.key]: (e.currentTarget as HTMLInputElement).checked })} />
            {r.label}
          </label>
        {/each}
      </fieldset>

      <fieldset>
        <legend class="muted">Projects</legend>
        {#each a.rules as r, i (i)}
          <div class="rule">
            <code>{r.repo}</code>
            <span class="muted">→</span>
            <span class="grow">{store.projects.get(r.project_id)?.name ?? '(deleted project)'}</span>
            <button class="btn small" onclick={() => removeRule(a, i)} aria-label="Remove the rule for {r.repo}">Remove</button>
          </div>
        {/each}
        {#if newRule[a.id]}
          <div class="rule">
            <input type="text" bind:value={newRule[a.id].repo} placeholder="owner/repo or owner/*" aria-label="Repository" />
            <select bind:value={newRule[a.id].project_id} aria-label="Project">
              <option value="">Project…</option>
              {#each projects as p (p.id)}<option value={p.id}>{p.name}</option>{/each}
            </select>
            <button class="btn small" onclick={() => addRule(a)} disabled={!newRule[a.id].repo.trim() || !newRule[a.id].project_id}>Add</button>
          </div>
        {/if}
        <p class="help muted">The first matching rule decides; anything else lands in the inbox. Applies to tasks that arrive from now on.</p>
      </fieldset>

      <div class="row">
        <input type="password" bind:value={newToken[a.id]} placeholder="New token" autocomplete="new-password" aria-label="New token for {a.login}" />
        <button class="btn small" onclick={() => saveToken(a)} disabled={!newToken[a.id]?.trim() || busy === a.id}>Replace token</button>
      </div>
      <div class="row">
        <button class="btn small" onclick={() => sync(a)} disabled={!!busy}>{busy === a.id ? 'Checking…' : 'Check now'}</button>
        <button class="btn small danger" onclick={() => remove(a)} disabled={!!busy}>Remove</button>
      </div>
    </div>
  {/each}

  {#if loaded}
    <form onsubmit={connect}>
      <h3 class="muted">{accounts.length ? 'Connect another account' : 'Connect an account'}</h3>
      <div class="kinds" role="radiogroup" aria-label="Kind of server">
        {#each KINDS as k (k.id)}
          <button type="button" class="chip" class:on={form.kind === k.id} role="radio" aria-checked={form.kind === k.id} onclick={() => ((form.kind = k.id), (tested = null))}>{k.name}</button>
        {/each}
      </div>
      <label>
        <span>Server address{form.kind === 'gitea' ? '' : ' (leave empty for ' + (form.kind === 'github' ? 'github.com' : 'gitlab.com') + ')'}</span>
        <input type="url" bind:value={form.url} placeholder={formKind.placeholder} required={form.kind === 'gitea'} autocomplete="url" />
      </label>
      <label>
        <span>Access token</span>
        <input type="password" bind:value={form.token} required autocomplete="new-password" />
      </label>
      <p class="help muted">{formKind.tokenHelp} It's stored encrypted and never shown again.</p>
      <div class="row">
        <button class="btn" type="button" onclick={test} disabled={!!busy || !form.token.trim()}>{busy === 'test' ? 'Testing…' : 'Test token'}</button>
        <button class="btn primary" type="submit" disabled={!!busy || !form.token.trim()}>{busy === 'connect' ? 'Connecting…' : 'Connect'}</button>
      </div>
      {#if tested}
        <p class="status" class:bad={!tested.ok} role="status">{tested.ok ? `Works: the token belongs to ${tested.login}.` : tested.error}</p>
      {/if}
    </form>
  {/if}
</section>

<style>
  section {
    padding: 18px;
    margin-bottom: 14px;
  }
  h2 {
    font-size: 16px;
    margin-bottom: 12px;
  }
  h3 {
    font-size: 13px;
    font-weight: 600;
    margin: 4px 0 8px;
  }
  .help {
    font-size: 13px;
    margin: 4px 0 12px;
  }
  .account {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 10px 12px;
    margin-bottom: 12px;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    align-items: baseline;
  }
  .top .muted {
    font-size: 13px;
    overflow-wrap: anywhere;
  }
  .status {
    font-size: 13px;
    margin: 6px 0 4px;
    color: var(--muted);
  }
  .status.bad {
    color: var(--danger);
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 10px 0 0;
  }
  legend {
    font-size: 12px;
    margin-bottom: 4px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
    font-size: 14px;
  }
  .rule {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
  }
  .rule input {
    flex: 1 1 160px;
    min-width: 0;
  }
  .rule select {
    flex: 1 1 140px;
    min-width: 0;
  }
  .rule .grow {
    flex: 1;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
  .row input {
    flex: 1 1 160px;
    min-width: 0;
  }
  .kinds {
    margin-bottom: 10px;
  }
  label {
    display: block;
    margin-bottom: 12px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
</style>
