const test = require('node:test');
const assert = require('node:assert/strict');
const net = require('node:net');
const path = require('node:path');
const { spawn } = require('node:child_process');

async function freePort() {
  const listener = net.createServer();
  await new Promise((resolve) => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise((resolve) => listener.close(resolve));
  return port;
}

test('local watcher rejects unauthenticated control and blank tasks', async () => {
  const port = await freePort();
  const token = 'a'.repeat(64);
  const child = spawn(process.execPath, [path.join(__dirname, 'server.cjs')], {
    cwd: __dirname,
    env: { ...process.env, AGENT_WATCH_PORT: String(port), AGENT_WATCH_TOKEN: token, NEXUS_PARENT_PID: String(process.pid) },
    stdio: ['pipe', 'ignore', 'ignore'],
  });
  try {
    const url = `http://127.0.0.1:${port}`;
    let ready = false;
    for (let i = 0; i < 40; i++) {
      try {
        const response = await fetch(`${url}/health`, { headers: { 'x-nexus-token': token } });
        if (response.ok) { ready = true; break; }
      } catch { /* starting */ }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    assert.equal(ready, true, 'sidecar should start');
    assert.equal((await fetch(`${url}/health`)).status, 403);
    assert.equal((await fetch(`${url}/health`, { headers: { 'x-nexus-token': 'b'.repeat(64) } })).status, 403);
    const blank = await fetch(`${url}/watch`, {
      method: 'POST',
      headers: { 'x-nexus-token': token, 'content-type': 'application/json' },
      body: JSON.stringify({ type: 'claude' }),
    });
    assert.equal(blank.status, 400);
    const watches = await fetch(`${url}/watch`, { headers: { 'x-nexus-token': token } });
    assert.deepEqual((await watches.json()).watches, []);
    const exited = new Promise((resolve) => child.once('exit', resolve));
    child.stdin.end();
    const result = await Promise.race([
      exited,
      new Promise((_, reject) => setTimeout(() => reject(new Error('sidecar survived parent pipe close')), 2000)),
    ]);
    assert.equal(result, 0);
  } finally {
    child.kill();
  }
});

async function startSidecar(token) {
  const port = await freePort();
  const child = spawn(process.execPath, [path.join(__dirname, 'server.cjs')], {
    cwd: __dirname,
    env: { ...process.env, AGENT_WATCH_PORT: String(port), AGENT_WATCH_TOKEN: token, NEXUS_PARENT_PID: String(process.pid) },
    stdio: ['pipe', 'ignore', 'ignore'],
  });
  const url = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 40; i++) {
    try {
      const r = await fetch(`${url}/health`, { headers: { 'x-nexus-token': token } });
      if (r.ok) return { child, url, port };
    } catch { /* starting */ }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  child.kill();
  throw new Error('sidecar did not start');
}

test('a task can never start an agent with a bypass preset or in a missing folder', async () => {
  const token = 'c'.repeat(64);
  const { child, url } = await startSidecar(token);
  try {
    const post = (body) =>
      fetch(`${url}/watch`, {
        method: 'POST',
        headers: { 'x-nexus-token': token, 'content-type': 'application/json' },
        body: JSON.stringify(body),
      });
    for (const preset of ['autonomous', 'yolo', '--dangerously-skip-permissions', 'full-auto']) {
      const r = await post({ type: 'claude', prompt: 'fix it', adapterConfig: { approvalPreset: preset } });
      assert.equal(r.status, 400, preset);
      assert.match((await r.json()).error, /not allowed/);
    }
    const missing = await post({ type: 'claude', prompt: 'fix it', workdir: path.join(__dirname, 'no_such_folder_xyz') });
    assert.equal(missing.status, 400);
    const blank = await post({ type: 'claude', prompt: '   ' });
    assert.equal(blank.status, 400);
    const unknown = await post({ type: 'nope', prompt: 'x' });
    assert.equal(unknown.status, 400);
    assert.deepEqual((await (await fetch(`${url}/watch`, { headers: { 'x-nexus-token': token } })).json()).watches, [], 'nothing was started');
  } finally {
    child.kill();
  }
});

test('a request with the right token but a foreign Host header is refused (DNS rebinding)', async () => {
  const token = 'd'.repeat(64);
  const { child, port } = await startSidecar(token);
  try {
    const status = await new Promise((resolve, reject) => {
      const http = require('node:http');
      const req = http.request(
        { host: '127.0.0.1', port, path: '/health', headers: { host: 'evil.example.com', 'x-nexus-token': token } },
        (res) => { res.resume(); resolve(res.statusCode); },
      );
      req.on('error', reject);
      req.end();
    });
    assert.equal(status, 403);
  } finally {
    child.kill();
  }
});

test('the sidecar refuses to run without a real token', async () => {
  const port = await freePort();
  const child = spawn(process.execPath, [path.join(__dirname, 'server.cjs')], {
    cwd: __dirname,
    env: { ...process.env, AGENT_WATCH_PORT: String(port), AGENT_WATCH_TOKEN: 'short' },
    stdio: ['pipe', 'ignore', 'ignore'],
  });
  const code = await Promise.race([
    new Promise((resolve) => child.once('exit', resolve)),
    new Promise((resolve) => setTimeout(() => resolve('still running'), 3000)),
  ]);
  child.kill();
  assert.equal(code, 2);
});

