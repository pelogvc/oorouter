import { browser, expect } from "@wdio/globals";
import { withExecuteOptions } from "@wdio/tauri-service";

const LATEST_UPDATE_METADATA_URL =
  "https://github.com/pelogvc/oorouter/releases/latest/download/latest.json";

describe("Tauri embedded WebDriver smoke", () => {
  const mainWindow = withExecuteOptions({ windowLabel: "main" });
  const liveUpdaterCheck = process.env.WDIO_LIVE_UPDATER_CHECK === "true";

  function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null;
  }

  function formatVersion(version: string): string {
    return version.startsWith("v") ? version : `v${version}`;
  }

  async function getMainWindowSnapshot() {
    const raw = await browser.tauri.execute(
      `JSON.stringify({
        href: window.location.href,
        title: document.title,
        readyState: document.readyState,
        bodyText: document.body?.innerText ?? null,
        bodyHtml: document.body?.innerHTML?.slice(0, 1000) ?? null,
        hasTauri: Boolean(window.__TAURI__),
        hasWdioBridge: Boolean(window.wdioTauri),
        scripts: Array.from(document.scripts).map((script) => ({
          src: script.src,
          type: script.type,
        })),
      })`,
      mainWindow
    );
    return JSON.parse(String(raw)) as {
      href: string;
      title: string;
      readyState: string;
      bodyText: string | null;
      bodyHtml: string | null;
      hasTauri: boolean;
      hasWdioBridge: boolean;
      scripts: Array<{ src: string; type: string }>;
    };
  }

  it("loads the desktop UI", async () => {
    let snapshot = await getMainWindowSnapshot();
    try {
      await browser.waitUntil(
        async () => {
          snapshot = await getMainWindowSnapshot();
          return (
            snapshot.bodyText?.includes("Dashboard") === true &&
            snapshot.bodyText.includes("oorouter")
          );
        },
        {
          timeout: 30000,
          timeoutMsg: "desktop UI did not render",
        }
      );
    } catch (error) {
      throw new Error(
        `${error instanceof Error ? error.message : String(error)}\n${JSON.stringify(snapshot, null, 2)}`
      );
    }

    const bodyText = snapshot.bodyText ?? "";
    expect(bodyText).toContain("Dashboard");
    expect(bodyText).toContain("Models");
    expect(bodyText).toContain("Settings");
  });

  it("exposes the Tauri WDIO bridge and can call IPC commands", async () => {
    let snapshot = await getMainWindowSnapshot();
    await browser.waitUntil(
      async () => {
        snapshot = await getMainWindowSnapshot();
        return snapshot.hasWdioBridge && snapshot.hasTauri;
      },
      {
        timeout: 30000,
        timeoutMsg: `WDIO Tauri bridge was not initialized: ${JSON.stringify(snapshot)}`,
      }
    );

    const windows = await browser.tauri.execute(
      "window.__TAURI__.core.invoke('plugin:wdio|list_windows')",
      mainWindow
    );
    expect(windows).toContain("main");

    const status = await browser.tauri.execute(
      "window.__TAURI__.core.invoke('get_server_status')",
      mainWindow
    );
    expect(status).toHaveProperty("running");
    expect(status).toHaveProperty("port", 19134);
    expect(status).toHaveProperty("auth_mode", "ApiKey");

    const settings = await browser.tauri.execute(
      "window.__TAURI__.core.invoke('get_settings')",
      mainWindow
    );
    expect(settings).toContainEqual({ key: "port", value: "19134" });

    await browser.tauri.execute(
      `Array.from(document.querySelectorAll("button"))
        .find((button) => button.textContent?.includes("Settings"))
        ?.click()`,
      mainWindow
    );
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes("Proxy listen port");
      },
      {
        timeout: 5000,
        timeoutMsg: "settings page did not render",
      }
    );
    const visiblePort = await browser.tauri.execute(
      `document.querySelector('input[type="number"]')?.value`,
      mainWindow
    );
    expect(visiblePort).toBe("19134");

    const updateState = await browser.tauri.execute(
      "window.__TAURI__.core.invoke('get_update_state')",
      mainWindow
    );
    expect(isRecord(updateState)).toBe(true);
    expect(updateState).toHaveProperty("currentVersion");
    if (!isRecord(updateState) || typeof updateState.currentVersion !== "string") {
      throw new Error(`invalid update state: ${JSON.stringify(updateState)}`);
    }

    const expectedCurrentVersion = formatVersion(updateState.currentVersion);
    await browser.waitUntil(
      async () => {
        const displayedVersion = await browser.tauri.execute(
          "document.getElementById('current-app-version')?.textContent?.trim()",
          mainWindow
        );
        return displayedVersion === expectedCurrentVersion;
      },
      {
        timeout: 5000,
        timeoutMsg: `settings did not display current version ${expectedCurrentVersion}`,
      }
    );

    if (!liveUpdaterCheck) {
      expect(updateState).toHaveProperty("status", "idle");
      expect(updateState).toHaveProperty("visible", false);
      const displayedLatestVersion = await browser.tauri.execute(
        "document.getElementById('latest-app-version')?.textContent?.trim()",
        mainWindow
      );
      expect(displayedLatestVersion).toBe("Not checked");

      await browser.tauri.execute(
        `window.__TAURI__.event.emit("update-state-changed", {
          status: "idle",
          currentVersion: ${JSON.stringify(updateState.currentVersion)},
          version: null,
          date: null,
          body: null,
          downloadedBytes: 0,
          contentLength: null,
          error: null,
          visible: false,
          manual: true,
        })`,
        mainWindow
      );
      await browser.waitUntil(
        async () => {
          const latestText = await browser.tauri.execute(
            "document.getElementById('latest-app-version')?.textContent ?? ''",
            mainWindow
          );
          return (
            String(latestText).includes(expectedCurrentVersion) &&
            String(latestText).includes("Up to date")
          );
        },
        {
          timeout: 5000,
          timeoutMsg: "settings did not display the up-to-date version state",
        }
      );
    }
  });

  if (liveUpdaterCheck) {
    it("detects the latest GitHub updater release from startup background check", async function () {
      const latestResponse = await fetch(LATEST_UPDATE_METADATA_URL);
      expect(latestResponse.ok).toBe(true);
      const latestMetadata = (await latestResponse.json()) as { version?: unknown };
      expect(typeof latestMetadata.version).toBe("string");
      const latestVersion = String(latestMetadata.version);

      const initialState = await browser.tauri.execute(
        "window.__TAURI__.core.invoke('get_update_state')",
        mainWindow
      );
      if (isRecord(initialState) && initialState.currentVersion === latestVersion) {
        this.skip();
      }

      await browser.waitUntil(
        async () => {
          const state = await browser.tauri.execute(
            "window.__TAURI__.core.invoke('get_update_state')",
            mainWindow
          );
          return (
            isRecord(state) &&
            "status" in state &&
            "version" in state &&
            state.status === "available" &&
            state.version === latestVersion
          );
        },
        {
          timeout: 30000,
          timeoutMsg: "startup update check did not detect the latest release",
        }
      );

      const updateState = await browser.tauri.execute(
        "window.__TAURI__.core.invoke('get_update_state')",
        mainWindow
      );
      expect(updateState).toHaveProperty("status", "available");
      expect(updateState).toHaveProperty("currentVersion");
      expect(updateState).toHaveProperty("version", latestVersion);
      expect(updateState).toHaveProperty("visible", true);
      expect(updateState).toHaveProperty("manual", false);
      expect(updateState).not.toHaveProperty("currentVersion", latestVersion);

      await browser.waitUntil(
        async () => {
          const displayedLatestVersion = await browser.tauri.execute(
            "document.getElementById('latest-app-version')?.textContent ?? ''",
            mainWindow
          );
          return String(displayedLatestVersion).includes(formatVersion(latestVersion));
        },
        {
          timeout: 5000,
          timeoutMsg: `settings did not display latest version ${latestVersion}`,
        }
      );
    });
  }
});
