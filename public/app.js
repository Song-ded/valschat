const DEFAULT_SERVER = window.location.origin;
const HEADER = new TextEncoder().encode('MK2|');
const TAG_LEN = 8;
const DEFAULT_SIZE = 4;
const PAYLOAD_BASE = 0x10;
const MIXED_POOL = new TextEncoder().encode("123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679123456791234567912345679qwertyuuiop[]asdfghjkl;'zxcvbnm,./!@#%$^&*$(^)&(+_|~!!!!!!!.QWERTYUIOPASDFGHJKLZXCVBNM");
const MAX_MESSAGE_CHARS = 80;

const els = {
  serverUrl: document.getElementById('server-url'),
  authUser: document.getElementById('auth-user'),
  authPassword: document.getElementById('auth-password'),
  registerBtn: document.getElementById('register-btn'),
  loginBtn: document.getElementById('login-btn'),
  logoutBtn: document.getElementById('logout-btn'),
  sessionLine: document.getElementById('session-line'),
  createRoomName: document.getElementById('create-room-name'),
  createRoomLimit: document.getElementById('create-room-limit'),
  createRoomBtn: document.getElementById('create-room-btn'),
  joinRoomName: document.getElementById('join-room-name'),
  joinRoomBtn: document.getElementById('join-room-btn'),
  refreshRoomsBtn: document.getElementById('refresh-rooms-btn'),
  roomList: document.getElementById('room-list'),
  roomTitle: document.getElementById('room-title'),
  roomSubtitle: document.getElementById('room-subtitle'),
  leaveRoomBtn: document.getElementById('leave-room-btn'),
  refreshMessagesBtn: document.getElementById('refresh-messages-btn'),
  messages: document.getElementById('messages'),
  messageKey: document.getElementById('message-key'),
  messageInput: document.getElementById('message-input'),
  sendBtn: document.getElementById('send-btn'),
  helpBtn: document.getElementById('help-btn'),
  charCounter: document.getElementById('char-counter'),
  limitInput: document.getElementById('limit-input'),
  setLimitBtn: document.getElementById('set-limit-btn'),
  targetUser: document.getElementById('target-user'),
  kickBtn: document.getElementById('kick-btn'),
  banBtn: document.getElementById('ban-btn'),
  logOutput: document.getElementById('log-output'),
  messageTemplate: document.getElementById('message-template')
};

const state = {
  session: loadSession(),
  activeRoom: null,
  rooms: [],
  lastSeenId: null,
  refreshTimer: null
};

init();

function init() {
  els.serverUrl.value = state.session?.server || DEFAULT_SERVER;
  updateSessionUi();
  updateCharCounter();
  bindEvents();
  if (state.session) {
    refreshRooms();
  }
}

function bindEvents() {
  els.registerBtn.addEventListener('click', () => authenticate('register'));
  els.loginBtn.addEventListener('click', () => authenticate('login'));
  els.logoutBtn.addEventListener('click', logout);
  els.createRoomBtn.addEventListener('click', createRoom);
  els.joinRoomBtn.addEventListener('click', () => joinRoom(els.joinRoomName.value.trim()));
  els.refreshRoomsBtn.addEventListener('click', refreshRooms);
  els.leaveRoomBtn.addEventListener('click', leaveRoom);
  els.refreshMessagesBtn.addEventListener('click', refreshMessagesFull);
  els.sendBtn.addEventListener('click', sendMessage);
  els.helpBtn.addEventListener('click', showHelp);
  els.messageInput.addEventListener('input', updateCharCounter);
  els.setLimitBtn.addEventListener('click', setLimit);
  els.kickBtn.addEventListener('click', () => roomAction('kick'));
  els.banBtn.addEventListener('click', () => roomAction('ban'));
}

async function authenticate(mode) {
  const user = els.authUser.value.trim();
  const password = els.authPassword.value;
  const server = normalizedServer();
  if (!user || !password) {
    log('Enter username and password first.');
    return;
  }

  const path = mode === 'register' ? '/auth/register' : '/auth/login';
  try {
    const response = await fetch(server + path, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ user, password })
    });
    const payload = await parseResponse(response);
    state.session = { server, user: payload.user, token: payload.token };
    saveSession(state.session);
    updateSessionUi();
    log(`${mode} ok: ${payload.user}`);
    await refreshRooms();
  } catch (error) {
    log(error.message);
  }
}

async function logout() {
  if (!state.session) {
    log('Not authorized in this browser.');
    return;
  }

  try {
    await authorizedFetch('/auth/logout', { method: 'POST' });
  } catch (_) {
  }

  clearSession();
  state.session = null;
  state.activeRoom = null;
  state.lastSeenId = null;
  state.rooms = [];
  renderRooms();
  renderMessages([]);
  updateSessionUi();
  log('Logged out in this browser.');
}

async function refreshRooms() {
  if (!state.session) {
    renderRooms();
    return;
  }
  try {
    state.rooms = await authorizedFetch('/rooms');
    renderRooms();
  } catch (error) {
    log(error.message);
  }
}

async function createRoom() {
  const name = els.createRoomName.value.trim();
  const limit = Number(els.createRoomLimit.value || 25);
  if (!name) {
    log('Room name is empty.');
    return;
  }
  try {
    await authorizedFetch('/rooms', {
      method: 'POST',
      body: JSON.stringify({ name, limit })
    });
    state.activeRoom = name;
    state.lastSeenId = null;
    log(`Created room ${name}`);
    await refreshRooms();
    await refreshMessagesFull();
  } catch (error) {
    log(error.message);
  }
}

async function joinRoom(name) {
  if (!name) {
    log('Room name is empty.');
    return;
  }
  try {
    await authorizedFetch(`/rooms/${encodeURIComponent(name)}/join`, { method: 'POST' });
    state.activeRoom = name;
    state.lastSeenId = null;
    log(`Joined room ${name}`);
    await refreshRooms();
    await refreshMessagesFull();
  } catch (error) {
    log(error.message);
  }
}

async function leaveRoom() {
  if (!state.activeRoom) {
    log('No active room.');
    return;
  }
  try {
    await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/leave`, { method: 'POST' });
    log(`Left room ${state.activeRoom}`);
    state.activeRoom = null;
    state.lastSeenId = null;
    renderMessages([]);
    updateRoomHeader();
    await refreshRooms();
  } catch (error) {
    log(error.message);
  }
}

async function setLimit() {
  if (!state.activeRoom) {
    log('No active room.');
    return;
  }
  const limit = Number(els.limitInput.value);
  if (!limit) {
    log('Invalid room limit.');
    return;
  }
  try {
    await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/limit`, {
      method: 'POST',
      body: JSON.stringify({ limit })
    });
    log(`Room limit changed to ${limit}`);
    await refreshRooms();
  } catch (error) {
    log(error.message);
  }
}

async function roomAction(kind) {
  if (!state.activeRoom) {
    log('No active room.');
    return;
  }
  const target = els.targetUser.value.trim();
  if (!target) {
    log('Target user is empty.');
    return;
  }
  try {
    await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/${kind}`, {
      method: 'POST',
      body: JSON.stringify({ target })
    });
    log(`${kind} ok: ${target}`);
    await refreshRooms();
  } catch (error) {
    log(error.message);
  }
}

async function sendMessage() {
  if (!state.activeRoom) {
    log('Join a room first.');
    return;
  }
  const key = els.messageKey.value;
  const text = els.messageInput.value;
  if (!key) {
    log('Message key is empty.');
    return;
  }
  if (!text.trim()) {
    log('Message is empty.');
    return;
  }
  if ([...text].length > MAX_MESSAGE_CHARS) {
    log(`Message must be at most ${MAX_MESSAGE_CHARS} characters.`);
    return;
  }

  try {
    const ciphertext = hexEncode(encryptMessage(key, text));
    await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/messages`, {
      method: 'POST',
      body: JSON.stringify({ ciphertext })
    });
    els.messageInput.value = '';
    updateCharCounter();
    await refreshMessagesIncremental();
  } catch (error) {
    log(error.message);
  }
}

async function refreshMessagesFull() {
  if (!state.activeRoom) {
    renderMessages([]);
    updateRoomHeader();
    return;
  }
  try {
    const messages = await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/messages`);
    const decoded = messages.map(toDisplayMessage);
    state.lastSeenId = decoded.length ? decoded[decoded.length - 1].id : null;
    renderMessages(decoded);
    updateRoomHeader();
  } catch (error) {
    log(error.message);
  }
}

async function refreshMessagesIncremental() {
  if (!state.activeRoom) {
    return;
  }
  const suffix = state.lastSeenId ? `?after_id=${state.lastSeenId}` : '';
  try {
    const messages = await authorizedFetch(`/rooms/${encodeURIComponent(state.activeRoom)}/messages${suffix}`);
    if (!messages.length) {
      return;
    }
    const decoded = messages.map(toDisplayMessage);
    appendMessages(decoded);
    state.lastSeenId = decoded[decoded.length - 1].id;
    updateRoomHeader();
  } catch (error) {
    log(error.message);
  }
}

function renderRooms() {
  els.roomList.innerHTML = '';
  updateRoomHeader();
  for (const room of state.rooms) {
    const item = document.createElement('div');
    item.className = 'room-item' + (room.name === state.activeRoom ? ' active' : '');
    item.innerHTML = `<strong>${escapeHtml(room.name)}</strong><span>${escapeHtml(room.owner)} - ${room.members}/${room.limit}</span>`;
    item.addEventListener('click', async () => {
      state.activeRoom = room.name;
      state.lastSeenId = null;
      await refreshMessagesFull();
      renderRooms();
    });
    els.roomList.appendChild(item);
  }
}

function renderMessages(messages) {
  els.messages.innerHTML = '';
  appendMessages(messages);
}

function appendMessages(messages) {
  for (const message of messages) {
    const node = els.messageTemplate.content.firstElementChild.cloneNode(true);
    node.querySelector('.message-meta').textContent = `[${formatTimestamp(message.timestamp)}] ${message.from}`;
    node.querySelector('.message-text').textContent = message.text;
    els.messages.appendChild(node);
  }
  els.messages.scrollTop = els.messages.scrollHeight;
}

function updateRoomHeader() {
  if (!state.activeRoom) {
    els.roomTitle.textContent = 'No active room';
    els.roomSubtitle.textContent = state.session ? 'Choose a room or create a new one.' : 'Login and join a room to begin.';
    return;
  }
  const room = state.rooms.find((entry) => entry.name === state.activeRoom);
  els.roomTitle.textContent = state.activeRoom;
  els.roomSubtitle.textContent = room
    ? `Owner: ${room.owner} - Members: ${room.members}/${room.limit}`
    : 'Room is active.';
}

function updateSessionUi() {
  if (state.session) {
    els.sessionLine.textContent = `${state.session.user} @ ${state.session.server}`;
    els.serverUrl.value = state.session.server;
    startPolling();
  } else {
    els.sessionLine.textContent = 'Not authorized';
    stopPolling();
  }
}

function updateCharCounter() {
  const count = [...els.messageInput.value].length;
  els.charCounter.textContent = `${count} / ${MAX_MESSAGE_CHARS}`;
}

function showHelp() {
  log('Commands: create room, join room, leave room, set limit, kick, ban, refresh, logout. Message limit: 80 chars.');
}

function log(message) {
  const line = document.createElement('div');
  line.className = 'log-entry';
  line.textContent = `${new Date().toLocaleTimeString()}  ${message}`;
  els.logOutput.prepend(line);
}

function startPolling() {
  stopPolling();
  state.refreshTimer = window.setInterval(() => {
    if (state.activeRoom) {
      refreshMessagesIncremental();
    }
  }, 2500);
}

function stopPolling() {
  if (state.refreshTimer) {
    window.clearInterval(state.refreshTimer);
    state.refreshTimer = null;
  }
}

function normalizedServer() {
  return (els.serverUrl.value.trim() || DEFAULT_SERVER).replace(/\/$/, '');
}

async function authorizedFetch(path, options = {}) {
  if (!state.session) {
    throw new Error('Not authorized in this browser.');
  }
  const headers = {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${state.session.token}`,
    ...(options.headers || {})
  };
  const response = await fetch(state.session.server + path, {
    ...options,
    headers
  });
  return parseResponse(response);
}

async function parseResponse(response) {
  const text = await response.text();
  let payload = null;
  if (text) {
    try {
      payload = JSON.parse(text);
    } catch (_) {
      payload = text;
    }
  }
  if (!response.ok) {
    if (payload && typeof payload === 'object' && payload.error) {
      throw new Error(payload.error);
    }
    throw new Error(`Request failed: ${response.status}`);
  }
  return payload;
}

function saveSession(session) {
  localStorage.setItem('valschat-session', JSON.stringify(session));
}

function loadSession() {
  const raw = localStorage.getItem('valschat-session');
  if (!raw) return null;
  try {
    return JSON.parse(raw);
  } catch (_) {
    return null;
  }
}

function clearSession() {
  localStorage.removeItem('valschat-session');
}

function toDisplayMessage(message) {
  const bytes = hexDecode(message.ciphertext);
  const text = decodeBytes(decryptMessage(els.messageKey.value || 'start', bytes));
  return { ...message, text };
}

function decodeBytes(bytes) {
  try {
    return new TextDecoder().decode(bytes);
  } catch (_) {
    return Array.from(bytes, (byte) => String.fromCharCode(byte)).join('');
  }
}

function formatTimestamp(timestamp) {
  return new Date(timestamp * 1000).toLocaleString();
}

function escapeHtml(value) {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');
}

function hexEncode(bytes) {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

function hexDecode(text) {
  const out = new Uint8Array(text.length / 2);
  for (let i = 0; i < text.length; i += 2) {
    out[i / 2] = parseInt(text.slice(i, i + 2), 16);
  }
  return out;
}

function keyBytes(key) {
  if (!key) throw new Error('Key must not be empty.');
  return new TextEncoder().encode(key);
}

function xorshift64(state) {
  state.value ^= state.value << 13n;
  state.value ^= state.value >> 7n;
  state.value ^= state.value << 17n;
  state.value &= 0xffffffffffffffffn;
  return state.value;
}

function seedFromPlaintext(key, plaintext) {
  const random = crypto.getRandomValues(new Uint32Array(2));
  let seed = (BigInt(random[0]) << 32n) ^ BigInt(random[1]) ^ (BigInt(plaintext.length) * 0x9e3779b97f4a7c15n);
  for (const byte of key) {
    seed ^= BigInt(byte) * 0x100000001b3n;
    seed = ((seed << 7n) | (seed >> (64n - 7n))) & 0xffffffffffffffffn;
  }
  if (seed === 0n) seed = 0x123456789abcdef0n;
  return { value: seed };
}

function randomRange(state, start, end) {
  if (end <= start) return start;
  return start + Number(xorshift64(state) % BigInt(end - start + 1));
}

function randomJump(state) {
  while (true) {
    const value = randomRange(state, 1, 9);
    if (value !== 8) return value;
  }
}

function filteredPool(key) {
  const set = new Set(key);
  const pool = [];
  for (const byte of MIXED_POOL) {
    if (!set.has(byte) && !(byte >= PAYLOAD_BASE && byte < PAYLOAD_BASE + 16)) {
      pool.push(byte);
    }
  }
  if (pool.length) return Uint8Array.from(pool);

  const fallback = [];
  for (let byte = 1; byte <= 255; byte += 1) {
    if (!set.has(byte) && !(byte >= PAYLOAD_BASE && byte < PAYLOAD_BASE + 16)) {
      fallback.push(byte);
    }
  }
  return Uint8Array.from(fallback);
}

function randomFromPool(state, pool) {
  return pool[Number(xorshift64(state) % BigInt(pool.length))];
}

function addRandomWordsAuto(output, state, pool) {
  const count = randomRange(state, Math.floor(DEFAULT_SIZE / 2), DEFAULT_SIZE * 10);
  for (let i = 0; i < count; i += 1) {
    output.push(randomFromPool(state, pool));
  }
}

function addRandomWordsFixed(output, state, count, pool) {
  for (let i = 0; i < Math.max(0, count - 1); i += 1) {
    output.push(randomFromPool(state, pool));
  }
}

function toPayloadBytes(plaintext) {
  const encoded = [];
  for (const byte of plaintext) {
    encoded.push(PAYLOAD_BASE + (byte >> 4));
    encoded.push(PAYLOAD_BASE + (byte & 0x0f));
  }
  return Uint8Array.from(encoded);
}

function fromPayloadBytes(payload) {
  if (payload.length % 2 !== 0) throw new Error('Decoded payload has odd length.');
  const output = [];
  for (let i = 0; i < payload.length; i += 2) {
    const high = decodePayloadNibble(payload[i]);
    const low = decodePayloadNibble(payload[i + 1]);
    output.push((high << 4) | low);
  }
  return Uint8Array.from(output);
}

function decodePayloadNibble(byte) {
  if (byte >= PAYLOAD_BASE && byte < PAYLOAD_BASE + 16) {
    return byte - PAYLOAD_BASE;
  }
  throw new Error('Invalid payload nibble.');
}

function computeTag(key, body) {
  let left = 0x243f6a8885a308d3n;
  let right = 0x13198a2e03707344n;
  for (const byte of key) {
    left ^= BigInt(byte);
    left = rotl64(left, 5n) * 0x100000001b3n & 0xffffffffffffffffn;
    right ^= BigInt(byte) << 1n;
    right = rotl64(right, 9n) * 0x9e3779b97f4a7c15n & 0xffffffffffffffffn;
  }
  for (const byte of body) {
    left ^= BigInt(byte);
    left = (rotl64(left, 7n) + 0xa5a5a5a5a5a5a5a5n) & 0xffffffffffffffffn;
    right ^= BigInt(byte) << 1n;
    right = (rotl64(right, 11n) + 0x3c6ef372fe94f82bn) & 0xffffffffffffffffn;
  }
  const mixed = (left ^ rotl64(right, 17n) ^ (BigInt(body.length) * 0x27d4eb2f165667c5n)) & 0xffffffffffffffffn;
  const bytes = new Uint8Array(TAG_LEN);
  let current = mixed;
  for (let i = 0; i < TAG_LEN; i += 1) {
    bytes[i] = Number(current & 0xffn);
    current >>= 8n;
  }
  return bytes;
}

function rotl64(value, shift) {
  const bits = 64n;
  return ((value << shift) | (value >> (bits - shift))) & 0xffffffffffffffffn;
}

function encryptMessage(keyText, text) {
  const key = keyBytes(keyText);
  const plaintext = new TextEncoder().encode(text);
  const stateRng = seedFromPlaintext(key, plaintext);
  const pool = filteredPool(key);
  const payload = toPayloadBytes(plaintext);
  const body = [];

  payload.forEach((payloadByte, index) => {
    const marker = key[index % key.length];
    const jump = randomJump(stateRng);
    addRandomWordsAuto(body, stateRng, pool);
    body.push(marker);
    body.push('0'.charCodeAt(0) + jump);
    addRandomWordsFixed(body, stateRng, jump, pool);
    body.push(payloadByte);
    addRandomWordsAuto(body, stateRng, pool);
  });

  body.reverse();
  const tag = computeTag(key, Uint8Array.from(body));
  return Uint8Array.from([...HEADER, ...tag, ...body]);
}

function decryptMessage(keyText, ciphertext) {
  const key = keyBytes(keyText);
  let body = ciphertext;
  if (startsWithBytes(body, HEADER)) {
    body = body.slice(HEADER.length);
  }
  if (body.length < TAG_LEN) {
    return body;
  }
  const tag = body.slice(0, TAG_LEN);
  const cipherBody = body.slice(TAG_LEN);
  const expected = computeTag(key, cipherBody);
  if (!equalBytes(tag, expected)) {
    return cipherBody;
  }

  const reversed = Uint8Array.from(cipherBody).reverse();
  const payload = [];
  for (let index = 0; index < reversed.length; index += 1) {
    if (index + 1 >= reversed.length) continue;
    const expectedMarker = key[payload.length % key.length];
    if (reversed[index] !== expectedMarker) continue;
    const digit = reversed[index + 1];
    if (digit < 49 || digit > 57) continue;
    const jump = digit - 48;
    const targetIndex = index + jump + 1;
    if (targetIndex < reversed.length) {
      const candidate = reversed[targetIndex];
      if (candidate >= PAYLOAD_BASE && candidate < PAYLOAD_BASE + 16) {
        payload.push(candidate);
      }
    }
  }

  if (!payload.length) {
    return reversed;
  }

  try {
    return fromPayloadBytes(Uint8Array.from(payload));
  } catch (_) {
    return Uint8Array.from(payload);
  }
}

function startsWithBytes(bytes, prefix) {
  if (bytes.length < prefix.length) return false;
  for (let i = 0; i < prefix.length; i += 1) {
    if (bytes[i] !== prefix[i]) return false;
  }
  return true;
}

function equalBytes(a, b) {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i += 1) {
    if (a[i] !== b[i]) return false;
  }
  return true;
}
