<script lang="ts">
  // Projects joined from other Streamline servers (Phase 8): join with an invite link,
  // see how each copy is doing, leave.
  import { api, ApiError } from '../api/client'
  import type { JoinView } from '../api/types/JoinView'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'

  let invite = $state('')
  let busy = $state('')
  const joins = $derived([...store.remoteProjects.values()])
  const err = (e: unknown) => toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')
  const host = (url: string) => {
    try {
      return new URL(url).host
    } catch {
      return url
    }
  }
  const when = (ts: string | null) =>
    ts ? new Date(ts).toLocaleString(store.me?.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' }) : 'not yet'

  async function join(e: Event) {
    e.preventDefault()
    busy = 'join'
    try {
      const j = await api.post<JoinView>('/federation/joins', { invite: invite.trim(), origin: location.origin })
      store.remoteProjects.set(j.id, j)
      invite = ''
      await store.sync().catch(() => {})
      const name = store.projects.get(j.root_project_id)?.name
      toast(name ? `Joined “${name}”` : 'Joined')
    } catch (e) {
      err(e)
    } finally {
      busy = ''
    }
  }
  async function sync(j: JoinView) {
    busy = j.id
    try {
      store.remoteProjects.set(j.id, await api.post<JoinView>(`/federation/joins/${j.id}/sync`))
    } catch (e) {
      err(e)
    } finally {
      busy = ''
    }
  }
  async function leave(j: JoinView) {
    const name = store.projects.get(j.root_project_id)?.name ?? 'this project'
    if (!confirm(`Leave “${name}”? Its copy disappears from here; on ${host(j.home_url)} it stays as it is.`)) return
    busy = j.id
    try {
      await api.del(`/federation/joins/${j.id}`)
      store.remoteProjects.delete(j.id)
    } catch (e) {
      err(e)
    } finally {
      busy = ''
    }
  }
</script>

<section class="card" id="remote-projects">
  <h2>Projects from other servers</h2>
  <p class="help muted">
    Someone with their own Streamline can share a project with you: paste the invite link they sent. You'll see and work
    on it here like your own; changes go to their server, so while it's down the project is read-only here. Your day
    plans, journal and everything else stay on this server.
  </p>
  {#each joins as j (j.id)}
    {@const p = store.projects.get(j.root_project_id)}
    <div class="line">
      <div class="what">
        {#if p}<a href="/projects/{p.id}">{p.name}</a>{:else}<span>(not loaded yet)</span>{/if}
        <span class="muted small">from {host(j.home_url)}</span>
        <span class="small" class:bad={j.status === 'error'}>
          {#if j.status === 'error'}Can't reach it: {j.last_error}{:else}Up to date as of {when(j.last_sync_at)}{/if}
        </span>
      </div>
      <div class="actions">
        <button class="btn small" onclick={() => sync(j)} disabled={!!busy}>{busy === j.id ? 'Checking…' : 'Check now'}</button>
        <button class="btn small danger" onclick={() => leave(j)} disabled={!!busy}>Leave</button>
      </div>
    </div>
  {/each}
  <form onsubmit={join}>
    <label>
      <span>Invite link</span>
      <input type="url" bind:value={invite} placeholder="https://their-server/join/…" required />
    </label>
    <button class="btn primary" type="submit" disabled={!!busy || !invite.trim()}>{busy === 'join' ? 'Joining…' : 'Join'}</button>
  </form>
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
  .help {
    font-size: 13px;
    margin: 4px 0 12px;
  }
  .line {
    display: flex;
    justify-content: space-between;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--border);
  }
  .what {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .what a {
    font-weight: 600;
  }
  .small {
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .bad {
    color: var(--danger);
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  form {
    margin-top: 12px;
  }
  label {
    display: block;
    margin-bottom: 10px;
  }
  label span {
    display: block;
    font-size: 12px;
    color: var(--muted);
    margin-bottom: 4px;
  }
</style>
