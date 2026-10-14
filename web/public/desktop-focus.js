// The desktop app's focus timer windows (D-82). The app sends the readout every second
// (`focus-screen`); full screen can be hidden until the next interval.
const t = window.__TAURI__
const $ = (id) => document.getElementById(id)
const hide = () => t.core.invoke('focus_hide').catch(() => {})

t.event.listen('focus-screen', ({ payload: p }) => {
  document.body.className = `${p.mode} ${p.break ? 'brk' : 'work'}`
  $('label').textContent = p.label
  $('time').textContent = p.time
  $('task').textContent = p.mode === 'fullscreen' && p.task ? p.task : ''
})
$('hide').addEventListener('click', hide)
document.addEventListener('keydown', (e) => e.key === 'Escape' && hide())
