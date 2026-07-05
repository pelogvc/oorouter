import { browser, expect } from "@wdio/globals";
import { withExecuteOptions } from "@wdio/tauri-service";

describe("Tauri embedded WebDriver smoke", () => {
  const mainWindow = withExecuteOptions({ windowLabel: "main" });

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
    expect(updateState).toHaveProperty("status", "idle");
    expect(updateState).toHaveProperty("visible", false);
  });
});
