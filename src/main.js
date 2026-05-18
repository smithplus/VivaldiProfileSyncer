const { invoke } = window.__TAURI__.core
const { listen } = window.__TAURI__.event

let profiles = []
let categories = []
let config = {}
let autoSyncTimer = null

// Tauri v2 may return struct fields in camelCase; this ensures snake_case access works
function normaliseConfig(raw) {
  const pick = (snake, camel) => raw[snake] !== undefined ? raw[snake] : raw[camel]
  return {
    auto_sync_enabled:  pick('auto_sync_enabled',  'autoSyncEnabled')  ?? false,
    auto_sync_minutes:  pick('auto_sync_minutes',  'autoSyncMinutes')  ?? 30,
    sync_on_launch:     pick('sync_on_launch',     'syncOnLaunch')     ?? false,
    login_item_enabled: pick('login_item_enabled', 'loginItemEnabled') ?? false,
    default_from:       pick('default_from',       'defaultFrom')      ?? '',
    default_to:         pick('default_to',         'defaultTo')        ?? '',
    selected_keys:      pick('selected_keys',      'selectedKeys')     ?? null,
    advanced_mode:      pick('advanced_mode',       'advancedMode')    ?? false,
  }
}

// ── Boot ──────────────────────────────────────────────────────────────────────

async function init() {
  try {
    [profiles, categories, config] = await Promise.all([
      invoke('get_profiles'),
      invoke('get_categories'),
      invoke('load_config'),
    ])
  } catch (e) {
    console.error('Init failed:', e)
    return
  }

  // Normalise config keys — Tauri may or may not camelCase the response
  config = normaliseConfig(config)

  renderProfileSelects()
  applyConfig()        // sets advanced mode + restores selected_keys, then calls renderCategories()
  checkVivaldiRunning()
  setInterval(checkVivaldiRunning, 5000)

  if (config.sync_on_launch) {
    setTimeout(() => runSync(false), 1500)
  }

  // Listen for tray "Sync Now" click
  listen('tray-sync-now', () => runSync(false))

  // Auto-save when window hides to tray or when quitting
  listen('window-hiding', () => silentSaveConfig())
  listen('tray-quit', async () => {
    await invoke('quit_app', { config: buildConfig() })
  })
}

// ── Tab navigation ────────────────────────────────────────────────────────────

document.querySelectorAll('.tab').forEach(btn => {
  btn.addEventListener('click', () => {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'))
    document.querySelectorAll('.tab-view').forEach(v => v.classList.add('hidden'))
    btn.classList.add('active')
    document.getElementById(`tab-${btn.dataset.tab}`).classList.remove('hidden')

    if (btn.dataset.tab === 'extensions') loadExtensions()
  })
})

// ── Profile selects ───────────────────────────────────────────────────────────

function renderProfileSelects() {
  const ids = ['select-from', 'select-to', 'ext-select-from', 'ext-select-to',
               'cfg-default-from', 'cfg-default-to']
  ids.forEach(id => {
    const sel = document.getElementById(id)
    sel.innerHTML = profiles.map(p => `<option value="${p.id}">${p.name}</option>`).join('')
  })
  if (profiles.length >= 2) {
    document.getElementById('select-to').selectedIndex = 1
    document.getElementById('ext-select-to').selectedIndex = 1
    document.getElementById('cfg-default-to').selectedIndex = 1
  }
}

document.getElementById('btn-swap').addEventListener('click', () => swapSelects('select-from', 'select-to'))
document.getElementById('ext-btn-swap').addEventListener('click', () => {
  swapSelects('ext-select-from', 'ext-select-to')
  loadExtensions()
})

function swapSelects(a, b) {
  const sa = document.getElementById(a), sb = document.getElementById(b)
  const tmp = sa.value; sa.value = sb.value; sb.value = tmp
}

// ── Categories ────────────────────────────────────────────────────────────────

function renderCategories() {
  const list = document.getElementById('category-list')
  const advanced = document.getElementById('toggle-advanced').checked
  // Restore saved key selections, fall back to all-selected only if no config saved yet
  const savedKeys = config.selected_keys ?? null

  list.innerHTML = categories.map(cat => {
    const hasSubKeys = cat.subKeys && cat.subKeys.length > 1
    const subHtml = hasSubKeys ? cat.subKeys.map(sk => {
      // If savedKeys exists, restore exact state; otherwise default to checked
      const checked = savedKeys ? savedKeys.includes(sk.key) : true
      return `
        <label class="sub-key-item">
          <input type="checkbox" data-subkey="${sk.key}" data-cat="${cat.id}" ${checked ? 'checked' : ''} />
          <span class="sub-key-label">${sk.label}</span>
        </label>
      `
    }).join('') : ''

    const expandBtn = (advanced && hasSubKeys)
      ? `<span class="cat-expand">▾</span>`
      : `<span class="cat-expand hidden">▾</span>`

    return `
      <div class="category-item" data-cat="${cat.id}">
        <div class="category-header">
          <input type="checkbox" data-cat-check="${cat.id}" ${!savedKeys || cat.subKeys.some(sk => savedKeys.includes(sk.key)) ? 'checked' : ''} />
          <span class="cat-icon">${cat.icon}</span>
          <div class="cat-text">
            <span class="cat-label">${cat.label}</span>
            <span class="cat-desc">${cat.desc}</span>
          </div>
          ${expandBtn}
        </div>
        ${advanced && hasSubKeys ? `<div class="sub-keys">${subHtml}</div>` : ''}
      </div>
    `
  }).join('')

  // Category header click: toggle expand in advanced mode
  list.querySelectorAll('.category-header').forEach(header => {
    header.addEventListener('click', e => {
      if (e.target.type === 'checkbox') return
      const item = header.closest('.category-item')
      const subKeys = item.querySelector('.sub-keys')
      const arrow = header.querySelector('.cat-expand')
      if (!subKeys || arrow.classList.contains('hidden')) return
      subKeys.classList.toggle('open')
      arrow.classList.toggle('open')
    })
  })

  // Category checkbox: toggle all sub-keys
  list.querySelectorAll('[data-cat-check]').forEach(cb => {
    cb.addEventListener('change', () => {
      const catId = cb.dataset.catCheck
      list.querySelectorAll(`[data-subkey][data-cat="${catId}"]`)
          .forEach(sk => sk.checked = cb.checked)
      silentSaveConfig()
    })
  })

  // Sub-key checkbox: update parent if all/none selected
  list.querySelectorAll('[data-subkey]').forEach(sk => {
    sk.addEventListener('change', () => {
      const catId = sk.dataset.cat
      const allSubs = [...list.querySelectorAll(`[data-subkey][data-cat="${catId}"]`)]
      const catCb = list.querySelector(`[data-cat-check="${catId}"]`)
      catCb.checked = allSubs.some(s => s.checked)
      catCb.indeterminate = catCb.checked && !allSubs.every(s => s.checked)
      silentSaveConfig()
    })
  })
}

document.getElementById('toggle-advanced').addEventListener('change', () => { renderCategories(); silentSaveConfig() })

document.getElementById('btn-all').addEventListener('click', () => {
  const boxes = document.querySelectorAll('[data-cat-check]')
  const allChecked = [...boxes].every(b => b.checked)
  document.querySelectorAll('[data-cat-check], [data-subkey]')
    .forEach(b => { b.checked = !allChecked; b.indeterminate = false })
  document.getElementById('btn-all').textContent = allChecked ? 'Select all' : 'Deselect all'
})

// Collect all individual keys to sync based on current checkboxes
function collectSelectedKeys() {
  const advanced = document.getElementById('toggle-advanced').checked
  const keys = []

  if (advanced) {
    document.querySelectorAll('[data-subkey]:checked').forEach(cb => {
      if (!keys.includes(cb.dataset.subkey)) keys.push(cb.dataset.subkey)
    })
  } else {
    document.querySelectorAll('[data-cat-check]:checked').forEach(cb => {
      const cat = categories.find(c => c.id === cb.dataset.catCheck)
      if (cat) cat.subKeys.forEach(sk => {
        if (!keys.includes(sk.key)) keys.push(sk.key)
      })
    })
  }
  return keys
}

// ── Sync ──────────────────────────────────────────────────────────────────────

async function runSync(dryRun = false) {
  const fromId = document.getElementById('select-from').value
  const toId   = document.getElementById('select-to').value
  const keys   = collectSelectedKeys()

  if (fromId === toId) return showBanner('result-banner', 'error', 'Source and destination must be different.')
  if (keys.length === 0) return showBanner('result-banner', 'error', 'Select at least one setting to sync.')

  const fromName = profiles.find(p => p.id === fromId)?.name ?? fromId
  const toName   = profiles.find(p => p.id === toId)?.name ?? toId

  showBanner('result-banner', 'info', `${dryRun ? 'Simulating' : 'Syncing'} ${fromName} → ${toName}…`)

  try {
    const result = await invoke('sync_profiles', { fromId, toId, keys, dryRun })
    showBanner('result-banner',
      dryRun ? 'info' : 'success',
      dryRun
        ? `Dry run — would copy: ${result.keys.join(', ')}`
        : `✓ Synced ${result.keys.length} keys from ${fromName} to ${toName}`
    )
    updateLastSync()
  } catch (e) {
    showBanner('result-banner', 'error', `Sync failed: ${e}`)
  }
}

document.getElementById('btn-sync').addEventListener('click', () => runSync(false))
document.getElementById('btn-dry').addEventListener('click',  () => runSync(true))

function updateLastSync() {
  // stored in config banner area for now
}

// ── Vivaldi running check ─────────────────────────────────────────────────────

async function checkVivaldiRunning() {
  const running = await invoke('is_vivaldi_running')
  document.getElementById('vivaldi-warning').classList.toggle('hidden', !running)
  const btn = document.getElementById('btn-sync')
  btn.disabled = running
  btn.style.opacity = running ? '0.4' : '1'
  btn.style.cursor  = running ? 'not-allowed' : 'pointer'
}

// ── Extensions ────────────────────────────────────────────────────────────────

let extensionsLoaded = false

async function loadExtensions() {
  if (extensionsLoaded) return
  extensionsLoaded = true
  await refreshExtensions()
}

document.getElementById('ext-btn-refresh').addEventListener('click', async () => {
  extensionsLoaded = false
  await refreshExtensions()
  extensionsLoaded = true
})

document.getElementById('ext-select-from').addEventListener('change', async () => {
  extensionsLoaded = false
  await refreshExtensions()
  extensionsLoaded = true
})

async function refreshExtensions() {
  const fromId = document.getElementById('ext-select-from').value
  const toId   = document.getElementById('ext-select-to').value
  const list   = document.getElementById('ext-list')

  list.innerHTML = '<div class="ext-empty">Loading…</div>'

  try {
    const [fromExts, toExts] = await Promise.all([
      invoke('list_extensions', { profileId: fromId }),
      invoke('list_extensions', { profileId: toId }),
    ])

    const toIds = new Set(toExts.map(e => e.id))

    if (fromExts.length === 0) {
      list.innerHTML = '<div class="ext-empty">No extensions found in source profile.</div>'
      return
    }

    list.innerHTML = fromExts.map(ext => {
      const exists = toIds.has(ext.id)
      return `
        <label class="ext-item">
          <input type="checkbox" data-ext-id="${ext.id}" ${exists ? 'disabled' : 'checked'} />
          <span class="ext-icon">🧩</span>
          <div class="ext-info">
            <span class="ext-name">${ext.name}</span>
            <span class="ext-meta">${ext.id} · v${ext.version}</span>
          </div>
          ${!ext.enabled ? '<span class="ext-status-disabled">disabled</span>' : ''}
          ${exists ? '<span class="ext-exists">already in dest</span>' : ''}
        </label>
      `
    }).join('')
  } catch (e) {
    list.innerHTML = `<div class="ext-empty">Error: ${e}</div>`
  }
}

document.getElementById('ext-btn-all').addEventListener('click', () => {
  document.querySelectorAll('[data-ext-id]:not(:disabled)')
    .forEach(cb => cb.checked = true)
})

async function runExtCopy(dryRun = false) {
  const fromId = document.getElementById('ext-select-from').value
  const toId   = document.getElementById('ext-select-to').value
  const extIds = [...document.querySelectorAll('[data-ext-id]:checked')]
    .map(cb => cb.dataset.extId)

  if (fromId === toId) return showBanner('ext-result-banner', 'error', 'Source and destination must be different.')
  if (extIds.length === 0) return showBanner('ext-result-banner', 'error', 'Select at least one extension.')

  showBanner('ext-result-banner', 'info', `${dryRun ? 'Simulating' : 'Copying'} ${extIds.length} extension(s)…`)

  try {
    const copied = await invoke('copy_extensions', { extIds, fromId, toId, dryRun })
    showBanner('ext-result-banner',
      dryRun ? 'info' : 'success',
      dryRun
        ? `Dry run — would copy: ${copied.join(', ')}`
        : `✓ Copied ${copied.length} extension(s). Restart Vivaldi to load them.`
    )
    extensionsLoaded = false
    if (!dryRun) await refreshExtensions()
  } catch (e) {
    showBanner('ext-result-banner', 'error', `Copy failed: ${e}`)
  }
}

document.getElementById('ext-btn-copy').addEventListener('click', () => runExtCopy(false))
document.getElementById('ext-btn-dry').addEventListener('click',  () => runExtCopy(true))

// ── Config / Settings ─────────────────────────────────────────────────────────

function applyConfig() {
  document.getElementById('cfg-auto-enabled').checked   = config.auto_sync_enabled  ?? false
  document.getElementById('cfg-auto-minutes').value     = config.auto_sync_minutes  ?? 30
  document.getElementById('cfg-sync-on-launch').checked = config.sync_on_launch     ?? false
  document.getElementById('cfg-login-item').checked     = config.login_item_enabled ?? false
  document.getElementById('toggle-advanced').checked    = config.advanced_mode      ?? false

  if (config.default_from) document.getElementById('cfg-default-from').value = config.default_from
  if (config.default_to)   document.getElementById('cfg-default-to').value   = config.default_to
  if (config.default_from) document.getElementById('select-from').value = config.default_from
  if (config.default_to)   document.getElementById('select-to').value   = config.default_to

  // Re-render categories so saved key selections are applied
  renderCategories()

  if (config.auto_sync_enabled) startAutoSync()
}

document.getElementById('cfg-login-item').addEventListener('change', async e => {
  const enable = e.target.checked
  try {
    await invoke('set_login_item', { enable })
  } catch (err) {
    showBanner('settings-banner', 'error', `Login item: ${err}`)
    e.target.checked = !enable  // revert on error
  }
})

function buildConfig() {
  return {
    auto_sync_enabled:  document.getElementById('cfg-auto-enabled').checked,
    auto_sync_minutes:  parseInt(document.getElementById('cfg-auto-minutes').value),
    sync_on_launch:     document.getElementById('cfg-sync-on-launch').checked,
    login_item_enabled: document.getElementById('cfg-login-item').checked,
    default_from:       document.getElementById('cfg-default-from').value,
    default_to:         document.getElementById('cfg-default-to').value,
    selected_keys:      collectSelectedKeys(),
    advanced_mode:      document.getElementById('toggle-advanced').checked,
  }
}

async function silentSaveConfig() {
  try {
    const newConfig = buildConfig()
    await invoke('save_config', { config: newConfig })
    config = newConfig
  } catch (_) {}
}

document.getElementById('btn-save-config').addEventListener('click', async () => {
  const newConfig = buildConfig()
  try {
    await invoke('save_config', { config: newConfig })
    config = newConfig
    stopAutoSync()
    if (newConfig.auto_sync_enabled) startAutoSync()
    showBanner('settings-banner', 'success', '✓ Settings saved.')
    setTimeout(() => showBanner('settings-banner', 'hidden', ''), 2000)
  } catch (e) {
    showBanner('settings-banner', 'error', `Failed to save: ${e}`)
  }
})

function startAutoSync() {
  const mins = parseInt(document.getElementById('cfg-auto-minutes').value)
  autoSyncTimer = setInterval(() => runSync(false), mins * 60 * 1000)
}

function stopAutoSync() {
  clearInterval(autoSyncTimer)
  autoSyncTimer = null
}

// Auto-save settings controls on change
;['cfg-auto-enabled', 'cfg-auto-minutes', 'cfg-sync-on-launch',
  'cfg-default-from', 'cfg-default-to'].forEach(id => {
  document.getElementById(id)?.addEventListener('change', silentSaveConfig)
})

// ── About links ───────────────────────────────────────────────────────────────

document.getElementById('link-github').addEventListener('click', e => {
  e.preventDefault()
  invoke('shell_open', { url: 'https://github.com/smithplus/VivaldiProfileSyncer' }).catch(() => {})
})

document.getElementById('link-issue').addEventListener('click', e => {
  e.preventDefault()
  invoke('shell_open', { url: 'https://github.com/smithplus/VivaldiProfileSyncer/issues' }).catch(() => {})
})

document.getElementById('link-bmc').addEventListener('click', e => {
  e.preventDefault()
  invoke('shell_open', { url: 'https://buymeacoffee.com/smithplus' }).catch(() => {})
})

// ── Helpers ───────────────────────────────────────────────────────────────────

function showBanner(id, type, msg) {
  const el = document.getElementById(id)
  if (!el) return
  if (type === 'hidden') { el.className = 'banner hidden'; return }
  el.className = `banner ${type}`
  el.textContent = msg
}

// ── Start ─────────────────────────────────────────────────────────────────────

init()
