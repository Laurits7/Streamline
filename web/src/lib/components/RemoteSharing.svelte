<script lang="ts">
  // Share a project with people on other Streamline servers (Phase 8): invite links and
  // the people who joined.
  import { api, ApiError } from '../api/client'
  import type { Invite } from '../api/types/Invite'
  import type { RemoteMember } from '../api/types/RemoteMember'
  import { store } from '../store.svelte'
  import { toast } from '../toast.svelte'
  import Sheet from './Sheet.svelte'

  let { projectId, onclose }: { projectId: string; onclose: () => void } = $props()

  const project = $derived(store.projects.get(projectId))
  let members = $state<RemoteMember[]>([])
  let link = $state('')
  let expires = $state('')
  let busy = $state(false)
  const err = (e: unknown) => toast(e instanceof ApiError ? e.message : 'Something went wrong', 'error')

  async function load() {
    try {
      members = await api.get<RemoteMember[]>(`/projects/${projectId}/remote-members`)
    } catch (e) {
      err(e)
    }
  }
  load()

  async function makeLink() {
    busy = true
    try {
      const inv = await api.post<Invite>(`/projects/${projectId}/invites`)
      link = `${location.origin}/join/${inv.token}`
      expires = new Date(inv.expires_at).toLocaleDateString(store.me?.locale || undefined, { day: 'numeric', month: 'long' })
    } catch (e) {
      err(e)
    } finally {
      busy = false
    }
  }
  async function copy() {
    try {
      await navigator.clipboard.writeText(link)
      toast('Link copied')
    } catch {
      toast('Select the link and copy it', 'error')
    }
  }
  async function remove(m: RemoteMember) {
    if (!confirm(`Remove ${m.display_name} (${m.username})? Their copy of “${project?.name}” goes away.`)) return
    try {
      await api.del(`/federation/links/${m.id}`)
      members = members.filter((x) => x.id !== m.id)
    } catch (e) {
      err(e)
    }
  }
  const when = (ts: string | null) =>
    ts ? new Date(ts).toLocaleString(store.me?.locale || undefined, { dateStyle: 'medium', timeStyle: 'short' }) : 'not yet'
</script>

<Sheet title="People on other servers" {onclose}>
  <p class="muted help">
    Someone with their own Streamline can work on “{project?.name}” from there: they join
    {store.groupName(project?.owner_group_id)} for this project only, and see and change its tasks, subprojects and routines.
    Their day plans and everything else stay on their server. Changes go through this server, so while it's down
    their copy is read-only.
  </p>

  {#if members.length}
    <h3>Joined</h3>
    <ul class="people">
      {#each members as m (m.id)}
        <li>
          <div>
            <strong>{m.display_name}</strong>
            <span class="muted">{m.username}</span>
            <span class="muted small">Last checked in {when(m.last_seen_at)}</span>
          </div>
          <button class="btn small danger" onclick={() => remove(m)}>Remove</button>
        </li>
      {/each}
    </ul>
  {/if}

  <h3>Invite someone</h3>
  {#if link}
    <div class="link">
      <input type="text" readonly value={link} aria-label="Invite link" onfocus={(e) => (e.currentTarget as HTMLInputElement).select()} />
      <button class="btn primary" onclick={copy}>Copy</button>
    </div>
    <p class="muted help">
      Send it to one person. They paste it in their own Streamline under Settings → Projects from other servers. It works
      once, until {expires}.
    </p>
    <button class="btn small" onclick={makeLink} disabled={busy}>Another link</button>
  {:else}
    <button class="btn primary" onclick={makeLink} disabled={busy}>{busy ? 'Making a link…' : 'Make an invite link'}</button>
  {/if}
</Sheet>

<style>
  h3 {
    font-size: 13px;
    font-weight: 600;
    margin: 16px 0 8px;
  }
  .help {
    font-size: 13px;
    margin: 4px 0 8px;
  }
  .people {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .people li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .people div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .people span {
    overflow-wrap: anywhere;
  }
  .small {
    font-size: 12px;
  }
  .link {
    display: flex;
    gap: 8px;
  }
  .link input {
    flex: 1;
    min-width: 0;
  }
</style>
