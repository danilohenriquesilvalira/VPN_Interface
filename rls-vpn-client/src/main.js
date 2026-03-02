import { invoke } from '@tauri-apps/api/core';

// ─── estado global ────────────────────────────────────────────────────────────
let config  = null;
let status  = { softether_ready: false, connection_ready: false, connected: false, vpn_state: 'not_installed', message: 'A iniciar...', raw_status: '' };
let lastRaw = '';
let installing   = false;
let setupBusy    = false;
let connectBusy  = false;

// ─── helpers DOM ──────────────────────────────────────────────────────────────
const el = id => document.getElementById(id);

function addLog(msg) {
  const time = new Date().toLocaleTimeString('pt-PT');
  const div  = document.createElement('div');
  div.className   = 'log-line';
  div.textContent = `[${time}] ${msg}`;
  const box = el('log');
  box.removeAttribute('hidden');
  box.prepend(div);
  while (box.children.length > 20) box.lastChild.remove();
}

// ─── render ───────────────────────────────────────────────────────────────────
function render() {
  const card     = el('status-card');
  const btnSetup = el('btn-setup');
  const btnConn  = el('btn-connect');

  // status card
  card.className = 'status-card ' + (status.vpn_state ?? 'disconnected');
  const LABELS = {
    connected:      'LIGADO',
    connecting:     'A LIGAR...',
    not_configured: 'NÃO CONFIGURADO',
    disconnected:   'DESLIGADO',
    not_installed:  'A INSTALAR...',
  };
  el('status-label').textContent = LABELS[status.vpn_state] ?? 'DESLIGADO';
  el('status-msg').textContent   = status.message ?? '';

  // badge de IP
  el('badge-row').hidden = !status.connected;
  if (status.connected) el('ip-badge').textContent = status.local_ip ?? 'A obter IP...';

  // ── botão 1: Configurar ────────────────────────────────────────────────────
  if (installing || !status.softether_ready) {
    btnSetup.textContent = installing ? '↻ A instalar...' : 'SoftEther indisponível';
    btnSetup.disabled = true;
    btnSetup.className = 'btn btn-loading';
    btnConn.hidden = true;
    return;
  }

  if (setupBusy) {
    btnSetup.textContent = '↻ A configurar...';
    btnSetup.disabled    = true;
    btnSetup.className   = 'btn btn-loading';
  } else {
    btnSetup.textContent = 'Configurar';
    btnSetup.disabled    = connectBusy;
    btnSetup.className   = 'btn btn-setup';
  }

  // ── botão 2: Ligar / Desligar ──────────────────────────────────────────────
  if (!status.connection_ready) {
    btnConn.hidden = true;
    return;
  }

  btnConn.hidden = false;

  if (connectBusy) {
    btnConn.textContent = status.connected ? '↻ A desligar...' : '↻ A ligar...';
    btnConn.disabled    = true;
    btnConn.className   = 'btn btn-loading';
  } else if (status.vpn_state === 'connecting') {
    btnConn.textContent = '↻ A ligar...';
    btnConn.disabled    = true;
    btnConn.className   = 'btn btn-loading';
  } else if (status.connected) {
    btnConn.textContent = 'Desligar';
    btnConn.disabled    = setupBusy;
    btnConn.className   = 'btn btn-disconnect';
  } else {
    btnConn.textContent = 'Ligar';
    btnConn.disabled    = setupBusy;
    btnConn.className   = 'btn btn-connect';
  }
}

// ─── polling de status ────────────────────────────────────────────────────────
async function refreshStatus() {
  try {
    status = await invoke('get_status');
    render();
    // Mostrar raw_status no log apenas quando muda — ajuda a diagnosticar
    // o que o SoftEther realmente retorna nesta máquina
    if (status.raw_status && status.raw_status !== lastRaw) {
      lastRaw = status.raw_status;
      addLog('[DEBUG vpncmd] ' + status.raw_status);
    }
  } catch (_) {}
}

// ─── acções ───────────────────────────────────────────────────────────────────
async function handleSetup() {
  if (!config || setupBusy) return;
  setupBusy = true;
  render();
  addLog('A criar adaptador de rede e conta VPN...');
  try {
    addLog(await invoke('setup_connection', { config }));
  } catch (e) {
    addLog('Erro ao configurar: ' + e);
  } finally {
    setupBusy = false;
    await refreshStatus();
  }
}

async function handleConnect() {
  if (!config || connectBusy) return;
  connectBusy = true;
  render();
  addLog('A ligar à rede RLS Automação...');
  try {
    addLog(await invoke('connect', { config }));
  } catch (e) {
    addLog('Erro ao ligar: ' + e);
  } finally {
    connectBusy = false;
    await refreshStatus();
  }
}

async function handleDisconnect() {
  if (!config || connectBusy) return;
  connectBusy = true;
  render();
  addLog('A desligar...');
  try {
    addLog(await invoke('disconnect', { config }));
  } catch (e) {
    addLog('Erro ao desligar: ' + e);
  } finally {
    connectBusy = false;
    await refreshStatus();
  }
}

// ─── event listeners ──────────────────────────────────────────────────────────
el('btn-setup').addEventListener('click', handleSetup);

el('btn-connect').addEventListener('click', () => {
  if (status.connected) handleDisconnect();
  else handleConnect();
});

el('settings-toggle').addEventListener('click', () => {
  const panel = el('settings-panel');
  const btn   = el('settings-toggle');
  const open  = !panel.hidden;
  panel.hidden = open;
  btn.textContent = '⚙ Configurações avançadas ' + (open ? '▾' : '▴');
  btn.setAttribute('aria-expanded', String(!open));
});

el('btn-save').addEventListener('click', () => {
  config = {
    ...config,
    host:     el('set-host').value.trim(),
    port:     parseInt(el('set-port').value) || 443,
    hub:      el('set-hub').value.trim(),
    username: el('set-user').value.trim(),
    password: el('set-password').value,
  };
  updateInfoRows();
  el('settings-panel').hidden = true;
  el('settings-toggle').textContent = '⚙ Configurações avançadas ▾';
  addLog('Configurações guardadas.');
});

el('btn-reset').addEventListener('click', async () => {
  if (!config) return;
  addLog('A fazer reset completo...');
  try {
    addLog(await invoke('clean_reset', { config }));
    await refreshStatus();
  } catch (e) {
    addLog('Erro: ' + e);
  }
});

// ─── helpers ──────────────────────────────────────────────────────────────────
function updateInfoRows() {
  if (!config) return;
  el('info-server').textContent  = config.host + ':' + config.port;
  el('info-user').textContent    = config.username + ' @ ' + config.hub;
  el('info-account').textContent = config.account_name;
}

function populateSettings() {
  if (!config) return;
  el('set-host').value     = config.host;
  el('set-port').value     = String(config.port);
  el('set-hub').value      = config.hub;
  el('set-user').value     = config.username;
  el('set-password').value = config.password;
}

// ─── init ─────────────────────────────────────────────────────────────────────
async function init() {
  config = await invoke('get_default_config');
  updateInfoRows();
  populateSettings();

  installing = true;
  render();
  try {
    addLog(await invoke('check_and_install_softether'));
  } catch (e) {
    addLog('Aviso: ' + e);
  } finally {
    installing = false;
  }

  await refreshStatus();
  setInterval(refreshStatus, 4000);
}

init().catch(console.error);
