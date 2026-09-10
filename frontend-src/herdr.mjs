import { WTerm } from '@wterm/dom';

const STORAGE_KEY = 'shellphone_refresh_token';
const params = new URLSearchParams(window.location.search);
const initialToken = params.get('token') || '';
const requestedPane = params.get('pane');
const isTouchDevice = ('ontouchstart' in window) || navigator.maxTouchPoints > 0;

const el = (id) => document.getElementById(id);
const statusEl = el('status');
const terminalEl = el('terminal');
const emptyEl = el('empty');
const toolbarEl = el('toolbar');
const drawerEl = el('drawer');
const treeEl = el('tree');
const currentEl = el('current');

let ws = null;
let reconnectTimer = null;
let sessionEnded = false;
let snapshot = null;
let attachedPaneId = null;
let autoAttached = false;

function send(msg) {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(msg));
  }
}

function sendResize() {
  send({ type: 'resize', cols: term.cols, rows: term.rows });
}

const term = new WTerm(terminalEl, {
  cursorBlink: true,
  autoResize: true,
  onData(data) {
    send({ type: 'input', data });
  },
  onResize() {
    sendResize();
  },
});
await term.init();

// Mobile toolbar
if (isTouchDevice) {
  toolbarEl.style.display = 'flex';
}

const KEY_MAP = {
  Escape: '\x1b',
  Tab: '\t',
  ArrowUp: '\x1b[A',
  ArrowDown: '\x1b[B',
  ArrowLeft: '\x1b[D',
  ArrowRight: '\x1b[C',
  Enter: '\r',
};

toolbarEl.addEventListener('pointerdown', (e) => {
  const btn = e.target.closest('button');
  if (!btn || !btn.dataset.key) return;
  e.preventDefault();
  send({ type: 'input', data: KEY_MAP[btn.dataset.key] || btn.dataset.key });
});

// Layout: top bar + optional toolbar + virtual keyboard
function updateLayout() {
  const vv = window.visualViewport;
  const keyboardOffset = vv ? window.innerHeight - vv.height : 0;
  const viewportHeight = vv ? vv.height : window.innerHeight;
  const toolbarHeight = isTouchDevice ? toolbarEl.offsetHeight : 0;
  if (isTouchDevice) {
    toolbarEl.style.bottom = keyboardOffset + 'px';
  }
  const height = viewportHeight - toolbarHeight - 44;
  terminalEl.style.height = height + 'px';
  emptyEl.style.height = height + 'px';
}

if (window.visualViewport) {
  window.visualViewport.addEventListener('resize', updateLayout);
  window.visualViewport.addEventListener('scroll', updateLayout);
}
updateLayout();

// Touch scroll: herdr owns the scrollback, so swipes become scroll commands
// for the attached viewport instead of local wterm scrolling. Pointer capture
// is required: every incoming frame re-renders wterm's rows, which detaches
// the node a touch started on, and touch events aimed at a detached node
// never bubble back to #terminal.
if (isTouchDevice) {
  const SCROLL_MULTIPLIER = 1;
  const FLING_FRICTION = 0.95;
  const FLING_MIN_VELOCITY = 0.05;
  let lastY = 0;
  let lastT = 0;
  let velocity = 0;
  let pendingRows = 0;
  let flushScheduled = false;
  let flingTimer = null;
  let activePointer = null;
  const rowHeight = () => Math.max(12, terminalEl.clientHeight / Math.max(1, term.rows));

  function queueScroll(px) {
    pendingRows += (px / rowHeight()) * SCROLL_MULTIPLIER;
    if (flushScheduled) return;
    flushScheduled = true;
    requestAnimationFrame(() => {
      flushScheduled = false;
      const lines = Math.trunc(pendingRows);
      if (lines !== 0) {
        pendingRows -= lines;
        send({ type: 'scroll', direction: lines > 0 ? 'down' : 'up', lines: Math.abs(lines) });
      }
    });
  }

  function stopFling() {
    if (flingTimer) cancelAnimationFrame(flingTimer);
    flingTimer = null;
  }

  function fling() {
    if (Math.abs(velocity) < FLING_MIN_VELOCITY) { flingTimer = null; return; }
    queueScroll(velocity * 16);
    velocity *= FLING_FRICTION;
    flingTimer = requestAnimationFrame(fling);
  }

  terminalEl.style.touchAction = 'none';

  terminalEl.addEventListener('pointerdown', (e) => {
    if (e.pointerType !== 'touch') return;
    stopFling();
    activePointer = e.pointerId;
    terminalEl.setPointerCapture(e.pointerId);
    lastY = e.clientY;
    lastT = e.timeStamp;
    velocity = 0;
    pendingRows = 0;
  });

  terminalEl.addEventListener('pointermove', (e) => {
    if (e.pointerId !== activePointer) return;
    const dy = lastY - e.clientY;
    const dt = Math.max(1, e.timeStamp - lastT);
    velocity = 0.7 * velocity + 0.3 * (dy / dt);
    lastY = e.clientY;
    lastT = e.timeStamp;
    queueScroll(dy);
  });

  const release = (e) => {
    if (e.pointerId !== activePointer) return;
    activePointer = null;
    if (Math.abs(velocity) >= FLING_MIN_VELOCITY) flingTimer = requestAnimationFrame(fling);
  };
  terminalEl.addEventListener('pointerup', release);
  terminalEl.addEventListener('pointercancel', release);
  terminalEl.addEventListener('touchmove', (e) => e.preventDefault(), { passive: false });
}

// Snapshot rendering
const STATUS_RANK = { blocked: 0, done: 1, working: 2, idle: 3, unknown: 4 };

function orderedPanes() {
  if (!snapshot) return [];
  const wsOrder = new Map(snapshot.workspaces.map((w, i) => [w.workspace_id, i]));
  const tabOrder = new Map(snapshot.tabs.map((t, i) => [t.tab_id, i]));
  return [...snapshot.panes].sort((a, b) =>
    (wsOrder.get(a.workspace_id) ?? 0) - (wsOrder.get(b.workspace_id) ?? 0) ||
    (tabOrder.get(a.tab_id) ?? 0) - (tabOrder.get(b.tab_id) ?? 0));
}

function paneName(pane) {
  return pane.label || pane.title || pane.terminal_title_stripped || pane.pane_id;
}

function shortCwd(cwd) {
  if (!cwd) return '';
  return cwd.replace(/^\/(?:home|Users)\/[^/]+/, '~');
}

function findPane(paneId) {
  return snapshot ? snapshot.panes.find((p) => p.pane_id === paneId) : null;
}

function renderCurrent() {
  const pane = attachedPaneId ? findPane(attachedPaneId) : null;
  const dot = currentEl.querySelector('.dot');
  const text = currentEl.querySelector('.text');
  const crumbs = currentEl.querySelector('.crumbs');
  el('focus').disabled = !pane;
  if (!pane) {
    dot.className = 'dot';
    text.textContent = attachedPaneId ? attachedPaneId : 'No pane attached';
    crumbs.textContent = 'herdr';
    emptyEl.hidden = !!attachedPaneId;
    return;
  }
  emptyEl.hidden = true;
  const workspace = snapshot.workspaces.find((w) => w.workspace_id === pane.workspace_id);
  const tab = snapshot.tabs.find((t) => t.tab_id === pane.tab_id);
  dot.className = `dot ${pane.agent_status}`;
  text.textContent = paneName(pane);
  crumbs.textContent = [workspace?.label, tab?.label].filter(Boolean).join(' › ') || pane.pane_id;

  const panes = orderedPanes();
  const idx = panes.findIndex((p) => p.pane_id === attachedPaneId);
  el('prev').disabled = panes.length < 2;
  el('next').disabled = panes.length < 2;
  el('prev').dataset.target = panes.length ? panes[(idx - 1 + panes.length) % panes.length].pane_id : '';
  el('next').dataset.target = panes.length ? panes[(idx + 1) % panes.length].pane_id : '';
}

function renderTree() {
  treeEl.textContent = '';
  if (!snapshot) return;
  const attention = orderedPanes().filter((p) => p.agent_status === 'blocked' || p.agent_status === 'done');
  if (attention.length) {
    const section = document.createElement('div');
    section.className = 'ws';
    const head = document.createElement('div');
    head.className = 'ws-head';
    head.innerHTML = '<span class="dot blocked"></span>Needs attention';
    section.appendChild(head);
    attention
      .sort((a, b) => STATUS_RANK[a.agent_status] - STATUS_RANK[b.agent_status])
      .forEach((pane) => section.appendChild(paneRow(pane)));
    treeEl.appendChild(section);
  }
  for (const workspace of snapshot.workspaces) {
    const section = document.createElement('div');
    section.className = 'ws';
    const head = document.createElement('div');
    head.className = 'ws-head';
    head.innerHTML = `<span class="dot ${workspace.agent_status}"></span>${escapeHtml(workspace.label)} <span class="n">${workspace.number}</span>`;
    section.appendChild(head);
    const tabs = snapshot.tabs.filter((t) => t.workspace_id === workspace.workspace_id);
    for (const tab of tabs) {
      const panes = snapshot.panes.filter((p) => p.tab_id === tab.tab_id);
      if (tabs.length > 1 || panes.length > 1) {
        const tabHead = document.createElement('div');
        tabHead.className = 'tab-head';
        tabHead.innerHTML = `<span class="dot ${tab.agent_status}"></span>${escapeHtml(tab.label)}`;
        section.appendChild(tabHead);
      }
      panes.forEach((pane) => section.appendChild(paneRow(pane)));
    }
    treeEl.appendChild(section);
  }
}

function paneRow(pane) {
  const row = document.createElement('button');
  row.className = 'pane' + (pane.pane_id === attachedPaneId ? ' attached' : '');
  row.dataset.pane = pane.pane_id;
  const agent = pane.display_agent || pane.agent;
  row.innerHTML = `
    <span class="dot ${pane.agent_status}"></span>
    <span class="meta">
      <div class="name">${escapeHtml(paneName(pane))}</div>
      <div class="sub">${escapeHtml(pane.pane_id)} · ${escapeHtml(shortCwd(pane.cwd))}</div>
    </span>
    ${agent ? `<span class="agent ${pane.agent_status}">${escapeHtml(agent)} · ${pane.agent_status}</span>` : ''}`;
  return row;
}

function escapeHtml(value) {
  return String(value ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function attach(paneId) {
  if (!paneId) return;
  send({ type: 'attach', pane_id: paneId });
  closeDrawer();
}

function openDrawer() {
  renderTree();
  drawerEl.hidden = false;
}

function closeDrawer() {
  drawerEl.hidden = true;
  term.focus();
}

el('menu').addEventListener('click', openDrawer);
el('empty-open').addEventListener('click', openDrawer);
currentEl.addEventListener('click', openDrawer);
el('close').addEventListener('click', closeDrawer);
el('prev').addEventListener('click', (e) => attach(e.currentTarget.dataset.target));
el('next').addEventListener('click', (e) => attach(e.currentTarget.dataset.target));
el('focus').addEventListener('click', () => {
  if (attachedPaneId) send({ type: 'focus', pane_id: attachedPaneId });
});
treeEl.addEventListener('click', (e) => {
  const row = e.target.closest('.pane');
  if (row) attach(row.dataset.pane);
});

// WebSocket connection
function getWsUrl(useRefresh) {
  const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const base = `${proto}//${window.location.host}/ws`;
  if (useRefresh) {
    const refresh = localStorage.getItem(STORAGE_KEY);
    if (refresh) return `${base}?refresh=${encodeURIComponent(refresh)}`;
    return null;
  }
  return `${base}?token=${encodeURIComponent(initialToken)}`;
}

function showStatus(text, cls) {
  statusEl.textContent = text;
  statusEl.className = cls;
  statusEl.style.display = 'block';
}

function handleControl(msg) {
  switch (msg.type) {
    case 'refresh_token':
      localStorage.setItem(STORAGE_KEY, msg.token);
      break;
    case 'snapshot': {
      const { type, attached_pane_id, ...rest } = msg;
      snapshot = rest;
      attachedPaneId = attached_pane_id;
      if (!attachedPaneId && !autoAttached) {
        autoAttached = true;
        const wanted = requestedPane && findPane(requestedPane) ? requestedPane : snapshot.focused_pane_id;
        if (wanted) attach(wanted);
      }
      renderCurrent();
      if (!drawerEl.hidden) renderTree();
      break;
    }
    case 'attached':
      attachedPaneId = msg.pane_id;
      renderCurrent();
      if (!drawerEl.hidden) renderTree();
      sendResize();
      break;
    case 'detached':
      if (attachedPaneId === msg.pane_id) {
        attachedPaneId = null;
        term.write(`\r\n\x1b[90m[${msg.pane_id} detached: ${msg.reason}]\x1b[0m\r\n`);
        renderCurrent();
      }
      break;
    case 'error':
      showStatus(msg.message, 'error');
      setTimeout(() => { statusEl.style.display = 'none'; }, 4000);
      break;
    case 'exit':
      sessionEnded = true;
      showStatus('Session closed', 'error');
      localStorage.removeItem(STORAGE_KEY);
      break;
  }
}

function connect(useRefresh) {
  const url = getWsUrl(useRefresh);
  if (!url) {
    showStatus('No credentials — scan QR code again', 'error');
    return;
  }

  ws = new WebSocket(url);
  ws.binaryType = 'arraybuffer';

  ws.addEventListener('open', () => {
    showStatus('Connected', 'connected');
    setTimeout(() => { statusEl.style.display = 'none'; }, 2000);
    sendResize();
  });

  ws.addEventListener('message', (event) => {
    if (event.data instanceof ArrayBuffer) {
      term.write(new Uint8Array(event.data));
    } else {
      handleControl(JSON.parse(event.data));
    }
  });

  ws.addEventListener('close', () => {
    if (sessionEnded) return;
    showStatus('Reconnecting...', 'reconnecting');
    scheduleReconnect();
  });

  ws.addEventListener('error', () => {
    showStatus('Connection error', 'error');
  });
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect(true);
  }, 2000);
}

window._term = term;
window._ws = () => ws;

const hasRefresh = localStorage.getItem(STORAGE_KEY);
connect(!!hasRefresh && !initialToken);
