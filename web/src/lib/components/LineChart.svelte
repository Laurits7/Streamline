<script lang="ts">
  // A small SVG line chart (no chart library). Gaps where there's no value.
  type Series = { label: string; color: string; points: (number | null)[]; min?: number; max?: number }
  let { series, labels, height = 160, format = (v: number) => String(Math.round(v * 10) / 10) }: {
    series: Series[]
    labels: string[]
    height?: number
    format?: (v: number) => string
  } = $props()

  const W = 600
  const PAD = { l: 40, r: 8, t: 10, b: 22 }
  const n = $derived(labels.length)
  const x = (i: number) => PAD.l + (n <= 1 ? 0 : (i * (W - PAD.l - PAD.r)) / (n - 1))
  function scale(s: Series) {
    const vals = s.points.filter((v): v is number => v !== null)
    let lo = s.min ?? Math.min(...vals)
    let hi = s.max ?? Math.max(...vals)
    if (!vals.length) [lo, hi] = [0, 1]
    if (hi === lo) [lo, hi] = [lo - 1, hi + 1]
    return { lo, hi, y: (v: number) => PAD.t + ((hi - v) * (height - PAD.t - PAD.b)) / (hi - lo) }
  }
  function path(s: Series) {
    const { y } = scale(s)
    let d = ''
    let pen = false
    s.points.forEach((v, i) => {
      if (v === null) {
        pen = false
        return
      }
      d += `${pen ? 'L' : 'M'}${x(i).toFixed(1)},${y(v).toFixed(1)}`
      pen = true
    })
    return d
  }
  const tickEvery = $derived(Math.max(1, Math.ceil(n / 7)))
  const primary = $derived(series[0] ? scale(series[0]) : null)
</script>

<svg viewBox="0 0 {W} {height}" role="img" aria-label={series.map((s) => s.label).join(', ')} preserveAspectRatio="none">
  {#if primary}
    {#each [primary.lo, (primary.lo + primary.hi) / 2, primary.hi] as v (v)}
      <line x1={PAD.l} x2={W - PAD.r} y1={primary.y(v)} y2={primary.y(v)} class="grid" />
      <text x={PAD.l - 6} y={primary.y(v) + 4} text-anchor="end" class="axis">{format(v)}</text>
    {/each}
  {/if}
  {#each labels as l, i (i)}
    {#if i % tickEvery === 0}<text x={x(i)} y={height - 6} text-anchor="middle" class="axis">{l}</text>{/if}
  {/each}
  {#each series as s (s.label)}
    {@const sc = scale(s)}
    <path d={path(s)} fill="none" stroke={s.color} stroke-width="2.5" stroke-linejoin="round" stroke-linecap="round" vector-effect="non-scaling-stroke" />
    {#each s.points as v, i (i)}
      {#if v !== null}<circle cx={x(i)} cy={sc.y(v)} r="3" fill={s.color}><title>{labels[i]}: {format(v)}</title></circle>{/if}
    {/each}
  {/each}
</svg>

<style>
  svg {
    width: 100%;
    height: auto;
    display: block;
  }
  .grid {
    stroke: var(--border);
    stroke-width: 1;
  }
  .axis {
    font-size: 11px;
    fill: var(--faint);
  }
</style>
