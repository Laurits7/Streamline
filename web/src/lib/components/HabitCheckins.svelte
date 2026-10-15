<script lang="ts">
  // The day's habit goals (D-105): did you manage? Yes / partly / no, with a short note.
  import type { HabitCheck } from '../api/types/HabitCheck'
  import { store } from '../store.svelte'

  let { date }: { date: string } = $props()
  const habits = $derived(store.habitsFor(date))
  const choices: { value: HabitCheck['result']; label: string; mark: string }[] = [
    { value: 'yes', label: 'Yes', mark: '✓' },
    { value: 'partly', label: 'Partly', mark: '~' },
    { value: 'no', label: 'No', mark: '✗' },
  ]
  function saveNote(goalId: string, value: string) {
    const c = store.habitCheck(goalId, date)
    if (c && c.note !== value.trim()) store.answerHabit(goalId, date, c.result, value.trim())
  }
</script>

{#if habits.length}
  <div class="habits" role="group" aria-label="Habits">
    <p class="title">Did you manage?</p>
    {#each habits as g (g.id)}
      {@const c = store.habitCheck(g.id, date)}
      <div class="habit">
        <span class="name">{g.title}</span>
        <div class="answers" role="radiogroup" aria-label="{g.title} on this day">
          {#each choices as ch (ch.value)}
            <button
              class="chip {ch.value}"
              class:on={c?.result === ch.value}
              role="radio"
              aria-checked={c?.result === ch.value}
              title={c?.result === ch.value ? 'Click again to clear' : ch.label}
              onclick={() => store.answerHabit(g.id, date, ch.value)}>{ch.mark} {ch.label}</button>
          {/each}
        </div>
        {#if c}
          <input
            class="note"
            type="text"
            maxlength="1000"
            value={c.note}
            placeholder={c.result === 'yes' ? 'A note (optional)' : 'What got in the way? (optional)'}
            aria-label="Note for {g.title}"
            onchange={(e) => saveNote(g.id, (e.currentTarget as HTMLInputElement).value)} />
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .habits {
    margin-bottom: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .title {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .habit {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
  }
  .name {
    flex: 1 1 140px;
    font-weight: 550;
    min-width: 0;
  }
  .answers {
    display: flex;
    gap: 6px;
  }
  .chip.yes.on {
    background: var(--success-soft, var(--accent-soft));
    color: var(--success, var(--accent));
  }
  .chip.no.on {
    background: var(--danger-soft, var(--accent-soft));
    color: var(--danger);
  }
  .note {
    flex: 1 1 100%;
    font-size: 13px;
  }
</style>
