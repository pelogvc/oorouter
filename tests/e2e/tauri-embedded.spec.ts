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
        const visiblePort = await browser.tauri.execute(
          `document.getElementById("setting-port") instanceof HTMLInputElement
            ? document.getElementById("setting-port")?.value
            : undefined`,
          mainWindow
        );
        return visiblePort === "19134";
      },
      {
        timeout: 5000,
        timeoutMsg: "settings page did not render the configured proxy port",
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

  it("manages OpenAI client keys and hot-applies authentication", async () => {
    const proxyUrl = `http://127.0.0.1:${process.env.WDIO_PROXY_PORT ?? "19134"}`;
    const invoke = (command: string, args?: Record<string, unknown>) =>
      browser.tauri.execute(
        `window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args ?? {})})`,
        mainWindow
      );
    const invokeError = (command: string, args?: Record<string, unknown>) =>
      browser.tauri.execute(
        `window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args ?? {})})
          .then(() => null, (error) => String(error))`,
        mainWindow
      );
    const readAuthState = async () => {
      const value = await invoke("list_client_api_keys");
      if (!isRecord(value) || typeof value.enabled !== "boolean" || !Array.isArray(value.keys)) {
        throw new Error("invalid client auth state");
      }
      return value as { enabled: boolean; keys: Array<Record<string, unknown>> };
    };
    const clickButton = async (label: string) => {
      const activated = await browser.tauri.execute(
        `(() => {
          const button = Array.from(document.querySelectorAll("button")).find((item) =>
            item.getAttribute("aria-label") === ${JSON.stringify(label)} ||
            item.textContent?.trim() === ${JSON.stringify(label)}
          );
          if (!(button instanceof HTMLButtonElement)) return false;
          button.focus();
          if (document.activeElement !== button) return false;
          button.click();
          return true;
        })()`,
        mainWindow
      );
      expect(activated).toBe(true);
    };
    const toggleAuthControl = async () => {
      const activated = await browser.tauri.execute(
        `(() => {
          const control = document.querySelector('[role="switch"]');
          if (!(control instanceof HTMLButtonElement)) return false;
          control.focus();
          if (document.activeElement !== control) return false;
          control.click();
          return true;
        })()`,
        mainWindow
      );
      expect(activated).toBe(true);
    };
    const expectModelsResponse = async (response: Response) => {
      expect(response.status).toBe(200);
      const body = await response.json();
      expect(body).toHaveProperty("object", "list");
      expect(body).toHaveProperty("data");
      expect(Array.isArray((body as { data?: unknown }).data)).toBe(true);
    };

    await clickButton("Auth");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes("OpenAI · /v1/*") &&
          String(text).includes("Protected when enabled") &&
          String(text).includes("Ollama · /api/*") &&
          String(text).includes("Always accessible without a key") &&
          !String(text).includes("Authentication applies only to OpenAI-compatible endpoints") &&
          !String(text).includes("This is separate from the Codex upstream Auth File");
      },
      { timeoutMsg: "Auth page did not render the compact endpoint scope summary" }
    );
    await browser.waitUntil(
      async () =>
        Boolean(
          await browser.tauri.execute(
            `document.querySelector('[role="switch"]') instanceof HTMLButtonElement`,
            mainWindow
          )
        ),
      { timeoutMsg: "Auth state did not finish loading" }
    );

    const layout = await browser.tauri.execute(
      `JSON.stringify({
        width: innerWidth,
        height: innerHeight,
        horizontalOverflow: document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth,
        hasDesktopOnlyNotice: document.body.innerText.includes("Desktop app required"),
        switchDisabled: document.querySelector('[role="switch"]')?.disabled,
      })`,
      mainWindow
    );
    expect(JSON.parse(String(layout))).toEqual({
      width: 720,
      height: 520,
      horizontalOverflow: false,
      hasDesktopOnlyNotice: false,
      switchDisabled: true,
    });

    const keyFormAlignment = JSON.parse(
      String(
        await browser.tauri.execute(
          `(() => {
            const input = document.getElementById("client-api-key-name");
            const button = Array.from(document.querySelectorAll("button")).find((item) =>
              item.textContent?.includes("Generate key")
            );
            if (!(input instanceof HTMLInputElement) || !(button instanceof HTMLButtonElement)) {
              return JSON.stringify(null);
            }
            const inputRect = input.getBoundingClientRect();
            const buttonRect = button.getBoundingClientRect();
            return JSON.stringify({
              inputTop: inputRect.top,
              inputBottom: inputRect.bottom,
              buttonTop: buttonRect.top,
              buttonBottom: buttonRect.bottom,
            });
          })()`,
          mainWindow
        )
      )
    ) as {
      inputTop: number;
      inputBottom: number;
      buttonTop: number;
      buttonBottom: number;
    };
    expect(keyFormAlignment.inputTop).toBe(keyFormAlignment.buttonTop);
    expect(keyFormAlignment.inputBottom).toBe(keyFormAlignment.buttonBottom);

    await clickButton("dark theme");
    await browser.waitUntil(
      async () =>
        Boolean(
          await browser.tauri.execute(
            `document.documentElement.classList.contains("dark")`,
            mainWindow
          )
        ),
      { timeoutMsg: "dark theme did not apply on the Auth page" }
    );
    expect(
      await browser.tauri.execute(
        `document.querySelector("main").scrollWidth > document.querySelector("main").clientWidth`,
        mainWindow
      )
    ).toBe(false);
    await clickButton("light theme");
    await browser.waitUntil(
      async () =>
        !Boolean(
          await browser.tauri.execute(
            `document.documentElement.classList.contains("dark")`,
            mainWindow
          )
        ),
      { timeoutMsg: "light theme did not apply on the Auth page" }
    );
    expect(
      await browser.tauri.execute(
        `(() => {
          const control = document.querySelector('[role="switch"]');
          const thumb = control?.querySelector('span');
          return control?.className.includes("motion-reduce:transition-none") &&
            thumb?.className.includes("motion-reduce:transition-none");
        })()`,
        mainWindow
      )
    ).toBe(true);

    const initial = await readAuthState();
    expect(initial).toEqual({ enabled: false, keys: [] });
    expect(
      String(await invokeError("set_client_auth_enabled", { enabled: true }))
    ).toContain("create a client API key");
    expect(
      String(await invokeError("reveal_client_api_key", { id: "missing-key" }))
    ).toContain("not found");

    await clickButton("Generate key");
    await browser.waitUntil(
      async () => (await readAuthState()).keys.length === 1,
      { timeoutMsg: "unnamed API key was not created" }
    );
    const oneKey = await readAuthState();
    expect(oneKey.enabled).toBe(false);
    expect(oneKey.keys[0]?.name ?? null).toBe(null);
    expect(oneKey.keys[0]?.redactedValue).toBe("sk-••••••••••••••••");
    expect(oneKey.keys[0]).not.toHaveProperty("token");
    const firstId = String(oneKey.keys[0]?.id);
    const firstSecretValue = await invoke("reveal_client_api_key", { id: firstId });
    if (!isRecord(firstSecretValue) || typeof firstSecretValue.value !== "string") {
      throw new Error("invalid reveal response");
    }
    const firstSecret = firstSecretValue.value;
    expect(/^sk-[A-Za-z0-9]{64}$/.test(firstSecret)).toBe(true);
    const maskedBody = await browser.tauri.execute("document.body.innerText", mainWindow);
    expect(String(maskedBody).includes(firstSecret)).toBe(false);

    await clickButton("Reveal Unnamed key");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes(firstSecret);
      },
      { timeoutMsg: "revealed key was not displayed" }
    );
    const revealA11y = JSON.parse(
      String(
        await browser.tauri.execute(
          `(() => {
            const button = Array.from(document.querySelectorAll("button")).find(
              (item) => item.getAttribute("aria-label") === "Hide Unnamed key"
            );
            const controls = button?.getAttribute("aria-controls") ?? null;
            return JSON.stringify({
              expanded: button?.getAttribute("aria-expanded"),
              controls,
              panelExists: controls ? Boolean(document.getElementById(controls)) : false,
            });
          })()`,
          mainWindow
        )
      )
    );
    expect(revealA11y).toEqual({
      expanded: "true",
      controls: `client-api-key-reveal-${firstId}`,
      panelExists: true,
    });
    await browser.tauri.execute(`window.dispatchEvent(new Event("blur"))`, mainWindow);
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return !String(text).includes(firstSecret);
      },
      { timeoutMsg: "window blur did not mask the revealed key" }
    );
    await clickButton("Reveal Unnamed key");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes(firstSecret);
      },
      { timeoutMsg: "key could not be revealed again after automatic masking" }
    );
    await clickButton("Hide Unnamed key");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return !String(text).includes(firstSecret);
      },
      { timeoutMsg: "hidden key remained in the page" }
    );

    await clickButton("Copy Unnamed key");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes("Unnamed key copied to clipboard.");
      },
      { timeoutMsg: "native copy action did not report success" }
    );
    const copiedBody = await browser.tauri.execute("document.body.innerText", mainWindow);
    expect(String(copiedBody).includes(firstSecret)).toBe(false);

    await toggleAuthControl();
    await browser.waitUntil(
      async () => (await readAuthState()).enabled,
      { timeoutMsg: "authentication did not turn on" }
    );
    expect(
      await browser.tauri.execute(
        `document.querySelector('[role="switch"]')?.getAttribute("aria-checked")`,
        mainWindow
      )
    ).toBe("true");

    const missing = await fetch(`${proxyUrl}/v1/models`);
    expect(missing.status).toBe(401);
    expect(missing.headers.get("www-authenticate")).toBe('Bearer realm="OpenAI API"');
    expect(await missing.json()).toEqual({
      error: {
        message: "Missing bearer authentication in header",
        type: "invalid_request_error",
        param: null,
        code: null,
      },
    });
    const valid = await fetch(`${proxyUrl}/v1/models`, {
      headers: { Authorization: `Bearer ${firstSecret}` },
    });
    await expectModelsResponse(valid);
    const wrong = await fetch(`${proxyUrl}/v1/models`, {
      headers: { Authorization: `Bearer sk-${"Z".repeat(64)}` },
    });
    expect(wrong.status).toBe(401);
    const health = await fetch(`${proxyUrl}/health`);
    expect(health.status).toBe(200);
    const ollama = await fetch(`${proxyUrl}/api/tags`, {
      headers: { Authorization: "Basic invalid" },
    });
    expect(ollama.status).toBe(200);
    const preflight = await fetch(`${proxyUrl}/v1/models`, {
      method: "OPTIONS",
      headers: {
        Origin: "http://localhost:1420",
        "Access-Control-Request-Method": "GET",
        "Access-Control-Request-Headers": "authorization",
      },
    });
    expect(preflight.ok).toBe(true);

    await toggleAuthControl();
    await browser.waitUntil(
      async () => !(await readAuthState()).enabled,
      { timeoutMsg: "authentication did not turn off" }
    );
    expect(
      await browser.tauri.execute(
        `document.querySelector('[role="switch"]')?.getAttribute("aria-checked")`,
        mainWindow
      )
    ).toBe("false");
    await expectModelsResponse(await fetch(`${proxyUrl}/v1/models`));

    const namedInputUpdated = await browser.tauri.execute(
      `(() => {
        const input = document.getElementById("client-api-key-name");
        if (!(input instanceof HTMLInputElement)) return false;
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
        setter?.call(input, "  Desktop test  ");
        input.dispatchEvent(new Event("input", { bubbles: true }));
        return true;
      })()`,
      mainWindow
    );
    expect(namedInputUpdated).toBe(true);
    await clickButton("Generate key");
    await browser.waitUntil(
      async () => (await readAuthState()).keys.length === 2,
      { timeoutMsg: "named API key was not created" }
    );
    const twoKeys = await readAuthState();
    expect(twoKeys.keys[0]?.name).toBe("Desktop test");
    const secondId = String(twoKeys.keys[0]?.id);
    const secondSecretValue = await invoke("reveal_client_api_key", { id: secondId });
    if (!isRecord(secondSecretValue) || typeof secondSecretValue.value !== "string") {
      throw new Error("invalid second reveal response");
    }
    const secondSecret = secondSecretValue.value;

    await toggleAuthControl();
    await browser.waitUntil(async () => (await readAuthState()).enabled);
    await clickButton("Delete Desktop test");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes("Requests using this key will be rejected immediately.");
      },
      { timeoutMsg: "delete confirmation did not open" }
    );
    await clickButton("Delete key");
    await browser.waitUntil(
      async () => {
        const state = await readAuthState();
        return state.enabled && state.keys.length === 1;
      },
      { timeoutMsg: "individual key deletion did not preserve enabled state" }
    );
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(
          `document.activeElement?.getAttribute("aria-label")`,
          mainWindow
        )) === "Delete Unnamed key",
      { timeoutMsg: "focus did not move to the remaining key after deletion" }
    );
    expect(
      (
        await fetch(`${proxyUrl}/v1/models`, {
          headers: { Authorization: `Bearer ${secondSecret}` },
        })
      ).status
    ).toBe(401);
    await expectModelsResponse(
      await fetch(`${proxyUrl}/v1/models`, {
        headers: { Authorization: `Bearer ${firstSecret}` },
      })
    );

    await clickButton("Delete Unnamed key");
    await browser.waitUntil(
      async () => {
        const text = await browser.tauri.execute("document.body.innerText", mainWindow);
        return String(text).includes("Deleting it will also turn OpenAI endpoint authentication off.");
      },
      { timeoutMsg: "last-key auto-off warning did not render" }
    );
    const focusInsideDialog = await browser.tauri.execute(
      `Boolean(document.querySelector('[role="dialog"]')?.contains(document.activeElement))`,
      mainWindow
    );
    expect(focusInsideDialog).toBe(true);
    await clickButton("Cancel");
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(
          `document.activeElement?.getAttribute("aria-label")`,
          mainWindow
        )) === "Delete Unnamed key",
      { timeoutMsg: "Cancel did not restore focus to the delete trigger" }
    );

    await clickButton("Delete Unnamed key");
    await browser.waitUntil(
      async () =>
        Boolean(
          await browser.tauri.execute(
            `Boolean(document.querySelector('[role="dialog"]'))`,
            mainWindow
          )
        ),
      { timeoutMsg: "delete confirmation did not reopen" }
    );
    await browser.keys(["Escape"]);
    await browser.waitUntil(
      async () => {
        const dialog = await browser.tauri.execute(
          `Boolean(document.querySelector('[role="dialog"]'))`,
          mainWindow
        );
        return !dialog;
      },
      { timeoutMsg: "Escape did not cancel deletion" }
    );
    expect(
      await browser.tauri.execute(
        `document.activeElement?.getAttribute("aria-label")`,
        mainWindow
      )
    ).toBe("Delete Unnamed key");
    const afterCancel = await readAuthState();
    expect(afterCancel.enabled).toBe(true);
    expect(afterCancel.keys).toHaveLength(1);
    expect(
      String(
        await invokeError("delete_client_api_key", {
          id: firstId,
          confirmedAutoDisable: false,
        })
      )
    ).toContain("confirm automatic client authentication disable");
    expect(await readAuthState()).toEqual(afterCancel);

    await clickButton("Delete Unnamed key");
    await clickButton("Delete key");
    await browser.waitUntil(
      async () => {
        const state = await readAuthState();
        return !state.enabled && state.keys.length === 0;
      },
      { timeoutMsg: "last key deletion did not atomically disable authentication" }
    );
    await browser.waitUntil(
      async () =>
        (await browser.tauri.execute(`document.activeElement?.textContent?.trim()`, mainWindow)) ===
        "Generate key",
      { timeoutMsg: "focus did not move to Generate key after deleting the last key" }
    );
    await expectModelsResponse(await fetch(`${proxyUrl}/v1/models`));

    const liveRegion = await browser.tauri.execute(
      `Array.from(document.querySelectorAll('[aria-live="polite"]')).some((item) =>
        item.textContent?.includes("Authentication was turned off"))`,
      mainWindow
    );
    expect(liveRegion).toBe(true);
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
