import { WTerm } from '@wterm/dom';

const STORAGE_KEY = 'shellphone_refresh_token';
const params = new URLSearchParams(window.location.search);
const initialToken = params.get('token') || '';
const statusEl = document.getElementById('status');
const terminalEl = document.getElementById('terminal');
const toolbarEl = document.getElementById('toolbar');
const isTouchDevice = ('ontouchstart' in window) || navigator.maxTouchPoints > 0;

let ws = null;
let reconnectTimer = null;
let sessionEnded = false;

function sendResize() {
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'resize', cols: term.cols, rows: term.rows }));
  }
}

const term = new WTerm(terminalEl, {
  cursorBlink: true,
  autoResize: true,
  onData(data) {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify({ type: 'input', data }));
    }
  },
  onResize(cols, rows) {
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
  const key = KEY_MAP[btn.dataset.key] || btn.dataset.key;
  if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify({ type: 'input', data: key }));
  }
});

// Mobile: resize terminal when virtual keyboard appears/disappears
function updateLayout() {
  const vv = window.visualViewport;
  const keyboardOffset = vv ? window.innerHeight - vv.height : 0;
  const viewportHeight = vv ? vv.height : window.innerHeight;
  const toolbarHeight = isTouchDevice ? toolbarEl.offsetHeight : 0;
  if (isTouchDevice) {
    toolbarEl.style.bottom = keyboardOffset + 'px';
  }
  terminalEl.style.height = (viewportHeight - toolbarHeight) + 'px';
}

if (window.visualViewport) {
  window.visualViewport.addEventListener('resize', updateLayout);
  window.visualViewport.addEventListener('scroll', updateLayout);
}
updateLayout();

// Touch scroll for alternate buffer: dispatch synthetic wheel events
// wterm sends one mouse escape sequence per wheel event (ignores deltaY magnitude),
// so we accumulate touch delta and dispatch multiple events to scroll proportionally.
if (isTouchDevice) {
  let lastY = 0;
  let scrollAccum = 0;
  let inAltSwipe = false;
  const THRESHOLD = 6;

  terminalEl.addEventListener('touchstart', (e) => {
    lastY = e.touches[0].pageY;
    scrollAccum = 0;
    inAltSwipe = term.bridge && term.bridge.usingAltScreen();
  }, { passive: true });

  terminalEl.addEventListener('touchmove', (e) => {
    if (!inAltSwipe) return;
    e.preventDefault();
    const dy = lastY - e.touches[0].pageY;
    lastY = e.touches[0].pageY;
    scrollAccum += dy;

    while (Math.abs(scrollAccum) >= THRESHOLD) {
      const down = scrollAccum > 0;
      scrollAccum -= down ? THRESHOLD : -THRESHOLD;
      terminalEl.dispatchEvent(new WheelEvent('wheel', {
        deltaY: down ? 1 : -1,
        deltaMode: WheelEvent.DOM_DELTA_PIXEL,
        bubbles: true,
        cancelable: true,
      }));
    }
  }, { passive: false });
}

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

function connect(useRefresh) {
  const url = getWsUrl(useRefresh);
  if (!url) {
    statusEl.textContent = 'No credentials — scan QR code again';
    statusEl.className = 'error';
    return;
  }

  ws = new WebSocket(url);
  ws.binaryType = 'arraybuffer';

  ws.addEventListener('open', () => {
    statusEl.textContent = 'Connected';
    statusEl.className = 'connected';
    setTimeout(() => { statusEl.style.display = 'none'; }, 2000);
    sendResize();
  });

  ws.addEventListener('message', (event) => {
    if (event.data instanceof ArrayBuffer) {
      term.write(new Uint8Array(event.data));
    } else {
      const msg = JSON.parse(event.data);
      if (msg.type === 'refresh_token') {
        localStorage.setItem(STORAGE_KEY, msg.token);
      } else if (msg.type === 'exit') {
        sessionEnded = true;
        const code = msg.code != null ? msg.code : '?';
        term.write(`\r\n\x1b[90m[process exited with code ${code}]\x1b[0m\r\n`);
        statusEl.textContent = 'Session closed';
        statusEl.className = 'error';
        statusEl.style.display = 'block';
        localStorage.removeItem(STORAGE_KEY);
      }
    }
  });

  ws.addEventListener('close', () => {
    if (sessionEnded) return;
    statusEl.textContent = 'Reconnecting...';
    statusEl.className = 'reconnecting';
    statusEl.style.display = 'block';
    scheduleReconnect();
  });

  ws.addEventListener('error', () => {
    statusEl.textContent = 'Connection error';
    statusEl.className = 'error';
    statusEl.style.display = 'block';
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
