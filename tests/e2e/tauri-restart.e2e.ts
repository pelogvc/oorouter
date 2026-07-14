import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs";
import http, { type Server } from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import {
  cleanupWdioSession,
  createTauriCapabilities,
  startWdioSession,
  withExecuteOptions,
} from "@wdio/tauri-service";

interface ClientApiKeySummary {
  id: string;
  name: string | null;
  redactedValue: string;
  createdAt: string;
}

interface ClientAuthState {
  enabled: boolean;
  keys: ClientApiKeySummary[];
}

interface ClientApiKeySecret {
  id: string;
  value: string;
}

const rootDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const restartTargetDir = path.join(rootDir, "target", "restart-e2e");
const appBinaryPath =
  process.env.WDIO_APP_BINARY ?? path.join(restartTargetDir, "debug", "oorouter");
const mainWindow = withExecuteOptions({ windowLabel: "main" });

let upstreamServer: Server | undefined;
let authPath = "";
let dataHome = "";

function sleep(durationMs: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, durationMs));
}

async function runCommand(
  command: string,
  args: string[],
  env: NodeJS.ProcessEnv = process.env
): Promise<void> {
  const child = spawn(command, args, {
    cwd: rootDir,
    env,
    stdio: "inherit",
  });
  const exitCode = await new Promise<number>((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (signal) {
        reject(new Error(`${command} terminated by ${signal}`));
        return;
      }
      resolve(code ?? 1);
    });
  });
  if (exitCode !== 0) {
    throw new Error(`${command} exited with code ${exitCode}`);
  }
}

async function buildTestBinary(): Promise<void> {
  await runCommand(process.execPath, ["run", "build:e2e"]);
  await runCommand("cargo", ["build", "-p", "oorouter"], {
    ...process.env,
    CARGO_TARGET_DIR: restartTargetDir,
    TAURI_CONFIG: JSON.stringify({ build: { devUrl: null } }),
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function parseAuthState(value: unknown): ClientAuthState {
  assert.ok(isRecord(value), "client auth state must be an object");
  assert.equal(typeof value.enabled, "boolean", "client auth enabled must be a boolean");
  assert.ok(Array.isArray(value.keys), "client auth keys must be an array");
  return value as unknown as ClientAuthState;
}

function parseSecret(value: unknown): ClientApiKeySecret {
  assert.ok(isRecord(value), "revealed key must be an object");
  assert.equal(typeof value.id, "string", "revealed key id must be a string");
  assert.equal(typeof value.value, "string", "revealed key value must be a string");
  return value as unknown as ClientApiKeySecret;
}

async function getFreePort(): Promise<number> {
  const server = net.createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address === "object", "free port lookup must return an address");
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return address.port;
}

async function isPortOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ host: "127.0.0.1", port });
    const settle = (open: boolean) => {
      socket.destroy();
      resolve(open);
    };
    socket.setTimeout(500);
    socket.once("connect", () => settle(true));
    socket.once("error", () => settle(false));
    socket.once("timeout", () => settle(false));
  });
}

async function waitForPortClosed(port: number, label: string): Promise<void> {
  const startedAt = Date.now();
  while (Date.now() - startedAt < 15_000) {
    if (!(await isPortOpen(port))) {
      return;
    }
    await sleep(100);
  }
  throw new Error(`${label} port ${port} remained open after session cleanup`);
}

async function waitForProxy(proxyUrl: string): Promise<void> {
  const startedAt = Date.now();
  while (Date.now() - startedAt < 15_000) {
    try {
      const response = await fetch(`${proxyUrl}/health`);
      if (response.ok) {
        return;
      }
    } catch {
      // The desktop app starts the proxy asynchronously after the WebDriver is ready.
    }
    await sleep(100);
  }
  throw new Error(`proxy did not become ready at ${proxyUrl}`);
}

async function waitForTauriBridge(
  browser: Awaited<ReturnType<typeof startWdioSession>>
): Promise<void> {
  const startedAt = Date.now();
  while (Date.now() - startedAt < 15_000) {
    const ready = await browser.execute(
      "return Boolean(window.__TAURI__?.core?.invoke && window.wdioTauri)"
    );
    if (ready) {
      return;
    }
    await sleep(100);
  }
  throw new Error("Tauri frontend bridge did not become ready");
}

async function startMockUpstream(): Promise<number> {
  upstreamServer = http.createServer((request, response) => {
    const url = new URL(request.url ?? "/", `http://${request.headers.host ?? "localhost"}`);
    if (request.method === "GET" && url.pathname === "/backend-api/codex/models") {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ models: [{ slug: "gpt-5.6-sol" }] }));
      return;
    }

    response.writeHead(404, { "content-type": "application/json" });
    response.end(JSON.stringify({ error: "not found" }));
  });

  await new Promise<void>((resolve, reject) => {
    upstreamServer?.once("error", reject);
    upstreamServer?.listen(0, "127.0.0.1", resolve);
  });
  const address = upstreamServer.address();
  assert.ok(address && typeof address === "object", "mock upstream must have an address");
  return address.port;
}

async function stopMockUpstream(): Promise<void> {
  const server = upstreamServer;
  upstreamServer = undefined;
  if (!server) {
    return;
  }
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

function createCapabilities(
  embeddedPort: number,
  proxyPort: number,
  upstreamPort: number
) {
  const capabilities = createTauriCapabilities(appBinaryPath, {
    driverProvider: "embedded",
    logLevel: "warn",
    commandTimeout: 60_000,
    startTimeout: 60_000,
  });
  capabilities["wdio:tauriServiceOptions"] = {
    ...capabilities["wdio:tauriServiceOptions"],
    driverProvider: "embedded",
    embeddedPort,
    statusPollTimeout: 10_000,
    commandTimeout: 60_000,
    startTimeout: 60_000,
    env: {
      AUTH_PATH: authPath,
      XDG_DATA_HOME: dataHome,
      PORT: String(proxyPort),
      CHATGPT_API_URL: `http://127.0.0.1:${upstreamPort}/backend-api/codex/responses`,
      LOG_LEVEL: "error",
      OOROUTER_DISABLE_STARTUP_UPDATE_CHECK: "true",
    },
  };
  return capabilities;
}

async function invoke(
  browser: Awaited<ReturnType<typeof startWdioSession>>,
  command: string,
  args: Record<string, unknown> = {}
): Promise<unknown> {
  return browser.tauri.execute(
    `window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)})`,
    mainWindow
  );
}

async function withIndependentSession<T>(
  embeddedPort: number,
  proxyPort: number,
  upstreamPort: number,
  run: (browser: Awaited<ReturnType<typeof startWdioSession>>) => Promise<T>
): Promise<T> {
  const capabilities = createCapabilities(embeddedPort, proxyPort, upstreamPort);
  const previousWebdriverPort = process.env.TAURI_WEBDRIVER_PORT;
  process.env.TAURI_WEBDRIVER_PORT = String(embeddedPort);
  try {
    const browser = await startWdioSession(capabilities, { rootDir });
    try {
      await waitForTauriBridge(browser);
      await waitForProxy(`http://127.0.0.1:${proxyPort}`);
      return await run(browser);
    } finally {
      try {
        await cleanupWdioSession(browser);
      } finally {
        await Promise.all([
          waitForPortClosed(proxyPort, "proxy"),
          waitForPortClosed(embeddedPort, "embedded WebDriver"),
        ]);
      }
    }
  } finally {
    if (previousWebdriverPort === undefined) {
      delete process.env.TAURI_WEBDRIVER_PORT;
    } else {
      process.env.TAURI_WEBDRIVER_PORT = previousWebdriverPort;
    }
  }
}

async function assertAuthorizedModelsResponse(proxyUrl: string, secret: string): Promise<void> {
  const response = await fetch(`${proxyUrl}/v1/models`, {
    headers: { Authorization: `Bearer ${secret}` },
  });
  assert.equal(response.status, 200, "persisted key must authorize /v1/models");
  const body: unknown = await response.json();
  assert.ok(isRecord(body), "models response must be an object");
  assert.equal(body.object, "list", "models response must be an OpenAI list");
  assert.ok(Array.isArray(body.data), "models response must contain a data array");
}

async function main(): Promise<void> {
  await buildTestBinary();
  assert.ok(fs.existsSync(appBinaryPath), `Tauri binary not found at ${appBinaryPath}`);
  const testDir = fs.mkdtempSync(path.join(os.tmpdir(), "oorouter-restart-e2e-"));
  authPath = path.join(testDir, "auth.json");
  dataHome = path.join(testDir, "data");
  let embeddedPort: number | undefined;
  let proxyPort: number | undefined;

  try {
    fs.mkdirSync(dataHome, { recursive: true });
    fs.writeFileSync(authPath, JSON.stringify({ OPENAI_API_KEY: "sk-proj-restart-e2e" }), {
      mode: 0o600,
    });

    [embeddedPort, proxyPort] = await Promise.all([getFreePort(), getFreePort()]);
    assert.notEqual(embeddedPort, proxyPort, "test ports must be distinct");
    const upstreamPort = await startMockUpstream();
    const proxyUrl = `http://127.0.0.1:${proxyPort}`;
    let firstSessionId = "";
    let keyId = "";
    let secret = "";

    await withIndependentSession(embeddedPort, proxyPort, upstreamPort, async (browser) => {
      firstSessionId = browser.sessionId;
      const initial = parseAuthState(await invoke(browser, "list_client_api_keys"));
      assert.deepEqual(initial, { enabled: false, keys: [] });

      const created = parseAuthState(
        await invoke(browser, "create_client_api_key", { name: "Restart persistence" })
      );
      assert.equal(created.enabled, false);
      assert.equal(created.keys.length, 1);
      keyId = created.keys[0]?.id ?? "";
      assert.ok(keyId, "created key must have an id");
      assert.equal(created.keys[0]?.name, "Restart persistence");
      assert.equal(created.keys[0]?.redactedValue, "sk-••••••••••••••••");

      const revealed = parseSecret(await invoke(browser, "reveal_client_api_key", { id: keyId }));
      assert.equal(revealed.id, keyId);
      assert.equal(
        /^sk-[A-Za-z0-9]{64}$/.test(revealed.value),
        true,
        "generated key must have the expected shape"
      );
      secret = revealed.value;

      const enabled = parseAuthState(
        await invoke(browser, "set_client_auth_enabled", { enabled: true })
      );
      assert.equal(enabled.enabled, true);
      assert.equal(enabled.keys.length, 1);
    });

    assert.ok(
      fs.existsSync(path.join(dataHome, "oorouter", "proxy.db")),
      "phase 1 must persist client auth to SQLite"
    );

    await withIndependentSession(embeddedPort, proxyPort, upstreamPort, async (browser) => {
      assert.notEqual(browser.sessionId, firstSessionId, "phase 2 must use a new WebDriver session");
      const restored = parseAuthState(await invoke(browser, "list_client_api_keys"));
      assert.equal(restored.enabled, true, "enabled state must survive the Tauri restart");
      assert.equal(restored.keys.length, 1, "key summary must survive the Tauri restart");
      assert.equal(restored.keys[0]?.id, keyId, "the same key must be restored from SQLite");
      assert.equal(restored.keys[0]?.name, "Restart persistence");

      const missing = await fetch(`${proxyUrl}/v1/models`);
      assert.equal(missing.status, 401, "restored auth must reject a missing bearer token");
      assert.equal(missing.headers.get("www-authenticate"), 'Bearer realm="OpenAI API"');

      await assertAuthorizedModelsResponse(proxyUrl, secret);
    });

    process.stdout.write("Tauri restart client-auth persistence E2E passed.\n");
  } finally {
    try {
      await stopMockUpstream();
      const closeChecks: Promise<void>[] = [];
      if (proxyPort !== undefined) {
        closeChecks.push(waitForPortClosed(proxyPort, "proxy"));
      }
      if (embeddedPort !== undefined) {
        closeChecks.push(waitForPortClosed(embeddedPort, "embedded WebDriver"));
      }
      await Promise.all(closeChecks);
    } finally {
      fs.rmSync(testDir, { recursive: true, force: true });
    }
  }
}

await main();
