<script lang="ts">
  // A project's mark (D-74): its colour in its shape, so projects can be told apart in
  // lists. Ideas (D-72) are an empty grey circle until they're activated.
  import type { Project } from '../api/types/Project'

  let { project, size = 10 }: { project: Project | null | undefined; size?: number } = $props()
  const idea = $derived(project?.status === 'idea')
  const fill = $derived(project?.color ?? 'var(--faint)')
</script>

<svg class="mark" viewBox="0 0 12 12" width={size} height={size} aria-hidden="true">
  {#if idea}
    <circle cx="6" cy="6" r="4.6" fill="none" stroke="var(--faint)" stroke-width="1.6" />
  {:else if project?.shape === 'square'}
    <rect x="1.4" y="1.4" width="9.2" height="9.2" rx="1.6" {fill} />
  {:else if project?.shape === 'triangle'}
    <path d="M6 .9 11.3 10.6H.7Z" {fill} stroke={fill} stroke-width="1" stroke-linejoin="round" />
  {:else if project?.shape === 'diamond'}
    <path d="M6 .4 11.6 6 6 11.6.4 6Z" {fill} />
  {:else if project?.shape === 'hexagon'}
    <polygon points="11.02,8.90 6.00,11.80 0.98,8.90 0.98,3.10 6.00,0.20 11.02,3.10" {fill} />
  {:else if project?.shape === 'star'}
    <polygon points="6.00,0.40 7.53,4.30 11.71,4.55 8.47,7.20 9.53,11.25 6.00,9.00 2.47,11.25 3.53,7.20 0.29,4.55 4.47,4.30" {fill} stroke={fill} stroke-width=".6" stroke-linejoin="round" />
  {:else}
    <circle cx="6" cy="6" r="5" {fill} />
  {/if}
</svg>

<style>
  .mark {
    flex: none;
    display: inline-block;
    vertical-align: middle;
  }
</style>
