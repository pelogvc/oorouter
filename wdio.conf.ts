import fs from "node:fs";
import http, { type Server } from "node:http";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { fileURLToPath } from "node:url";

import type { Options } from "@wdio/types";

const rootDir = path.dirname(fileURLToPath(import.meta.url));
const e2eDir = fs.mkdtempSync(path.join(os.tmpdir(), "oorouter-wdio-"));
const authPath = path.join(e2eDir, "auth.json");
const dataHome = path.join(e2eDir, "data");
const appBinaryPath =
  process.env.WDIO_APP_BINARY ?? path.join(rootDir, "target", "debug", "oorouter");
const proxyPort = process.env.WDIO_PROXY_PORT ?? "19134";
const upstreamPort = Number(process.env.WDIO_UPSTREAM_PORT ?? "19135");
const upstreamResponsesUrl = `http://127.0.0.1:${upstreamPort}/backend-api/codex/responses`;
const liveUpdaterCheck = process.env.WDIO_LIVE_UPDATER_CHECK === "true";
const viteUrl = "http://127.0.0.1:1420";
const viteBin = path.join(rootDir, "node_modules", ".bin", "vite");
let viteServer: ChildProcessWithoutNullStreams | undefined;
let upstreamServer: Server | undefined;
let viteLogs = "";

fs.mkdirSync(dataHome, { recursive: true });
fs.writeFileSync(authPath, JSON.stringify({ OPENAI_API_KEY: "sk-proj-wdio-test" }));

function appendViteLog(chunk: Buffer) {
  viteLogs = `${viteLogs}${chunk.toString()}`.slice(-4000);
}

async function assertVitePortAvailable() {
  await new Promise<void>((resolve, reject) => {
    const server = net.createServer();
    server.once("error", (error: NodeJS.ErrnoException) => {
      if (error.code === "EADDRINUSE") {
        reject(new Error(`${viteUrl} is already in use. Stop the existing Vite server before running e2e.`));
      } else {
        reject(error);
      }
    });
    server.once("listening", () => {
      server.close(() => resolve());
    });
    server.listen(1420, "127.0.0.1");
  });
}

async function waitForViteServer() {
  const startedAt = Date.now();
  while (Date.now() - startedAt < 30000) {
    try {
      const response = await fetch(viteUrl);
      if (response.ok) {
        return;
      }
    } catch {
      // Retry until the dev server is ready or the timeout expires.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for Vite dev server at ${viteUrl}\n${viteLogs}`);
}

async function startMockUpstream() {
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
    upstreamServer?.listen(upstreamPort, "127.0.0.1", resolve);
  });
}

async function stopMockUpstream() {
  const server = upstreamServer;
  upstreamServer = undefined;
  if (!server) {
    return;
  }
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

export const config: Options.Testrunner = {
  runner: "local",
  specs: ["./tests/e2e/**/*.spec.ts"],
  maxInstances: 1,
  logLevel: "warn",
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 1,
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath,
        driverProvider: "embedded",
        embeddedPort: 4445,
        statusPollTimeout: 10000,
        commandTimeout: 60000,
        captureBackendLogs: true,
        captureFrontendLogs: true,
        env: {
          AUTH_PATH: authPath,
          XDG_DATA_HOME: dataHome,
          PORT: proxyPort,
          CHATGPT_API_URL: upstreamResponsesUrl,
          LOG_LEVEL: "error",
          OOROUTER_DISABLE_STARTUP_UPDATE_CHECK: liveUpdaterCheck ? "false" : "true",
        },
      },
    ],
  ],
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": {
        application: appBinaryPath,
      },
      "wdio:tauriServiceOptions": {
        driverProvider: "embedded",
        embeddedPort: 4445,
      },
    },
  ],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 90000,
  },
  onPrepare: async () => {
    await assertVitePortAvailable();
    await startMockUpstream();
    viteServer = spawn(
      viteBin,
      ["--host", "127.0.0.1", "--port", "1420", "--strictPort", "--mode", "wdio"],
      {
        cwd: rootDir,
        env: {
          ...process.env,
          VITE_WDIO: "true",
        },
        stdio: ["ignore", "pipe", "pipe"],
      }
    );
    viteServer.stdout.on("data", appendViteLog);
    viteServer.stderr.on("data", appendViteLog);

    await Promise.race([
      waitForViteServer(),
      new Promise<never>((_, reject) => {
        viteServer?.once("exit", (code, signal) => {
          reject(
            new Error(
              `Vite dev server exited before becoming ready (code=${code}, signal=${signal})\n${viteLogs}`
            )
          );
        });
      }),
    ]);
  },
  onComplete: async () => {
    viteServer?.kill();
    await stopMockUpstream();
  },
};
