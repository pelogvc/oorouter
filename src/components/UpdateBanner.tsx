import { useEffect, useMemo, useState } from "react";
import {
  checkForUpdates,
  getUpdateState,
  installUpdate,
  listen,
  parseUpdateState,
  restartApp,
  type UpdateState,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { AlertCircle, CheckCircle2, Download, RefreshCw, RotateCcw } from "lucide-react";

function shouldShowUpdateBanner(state: UpdateState | null): state is UpdateState {
  if (!state?.visible) return false;
  return (
    state.status === "available" ||
    state.status === "installing" ||
    state.status === "installed" ||
    state.status === "error"
  );
}

function progressPercent(state: UpdateState): number | null {
  if (!state.contentLength || state.contentLength <= 0) return null;
  return Math.min(100, Math.round((state.downloadedBytes / state.contentLength) * 100));
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return fallback;
}

type FailedAction = "check" | "install" | "restart";

function actionFromResult(result: UpdateState, action: FailedAction): FailedAction | null {
  return result.status === "error" ? action : null;
}

export function UpdateBanner() {
  const [state, setState] = useState<UpdateState | null>(null);
  const [actionBusy, setActionBusy] = useState(false);
  const [lastFailedAction, setLastFailedAction] = useState<FailedAction | null>(null);
  const percent = useMemo(() => (state ? progressPercent(state) : null), [state]);

  useEffect(() => {
    let active = true;
    void getUpdateState()
      .then((nextState) => {
        if (active) setState(nextState);
      })
      .catch(() => undefined);

    const unlisten = listen<unknown>("update-state-changed", (event) => {
      if (!active) return;
      try {
        const nextState = parseUpdateState(event.payload);
        if (active) {
          setState(nextState);
          if (nextState.status !== "error") {
            setLastFailedAction(null);
          }
        }
      } catch (error) {
        setState((current) =>
          current
            ? {
                ...current,
                status: "error",
                error: errorMessage(error, "Failed to read update state."),
                visible: true,
                manual: true,
              }
            : current
        );
      }
    }).catch(() => undefined);

    return () => {
      active = false;
      unlisten.then((fn) => fn?.()).catch(() => undefined);
    };
  }, []);

  if (!shouldShowUpdateBanner(state)) {
    return null;
  }

  const isInstalling = state.status === "installing";
  const versionLabel = state.version ? `v${state.version}` : "new version";
  let title = `Update ${versionLabel} available`;
  if (state.status === "installed") {
    title = "Update installed";
  } else if (state.status === "error") {
    title = "Update failed";
  }

  let detail = state.body || "Install when you are ready to restart the app.";
  if (state.status === "installed") {
    detail = "Restart oorouter to finish applying the update.";
  } else if (state.status === "installing") {
    detail =
      percent === null
        ? "Downloading and verifying the signed updater artifact."
        : `Downloading signed updater artifact (${percent}%).`;
  } else if (state.status === "error") {
    detail =
      state.error || "Update failed. Retry or install the latest GitHub Release manually.";
  }

  let statusIcon = <Download className="h-4 w-4 text-muted-foreground" aria-hidden="true" />;
  if (state.status === "error") {
    statusIcon = <AlertCircle className="h-4 w-4 text-destructive-text" aria-hidden="true" />;
  } else if (state.status === "installed") {
    statusIcon = <CheckCircle2 className="h-4 w-4 text-success" aria-hidden="true" />;
  }

  const handleInstall = async () => {
    setActionBusy(true);
    try {
      const result = await installUpdate();
      setState(result);
      setLastFailedAction(actionFromResult(result, "install"));
    } catch (error) {
      setLastFailedAction("install");
      setState((current) =>
        current
          ? {
              ...current,
              status: "error",
              error: errorMessage(error, "Failed to install the update."),
              visible: true,
              manual: true,
            }
          : current
      );
    } finally {
      setActionBusy(false);
    }
  };

  const handleRetry = async () => {
    setActionBusy(true);
    try {
      if (lastFailedAction === "restart") {
        await restartApp();
        setState((current) =>
          current
            ? {
                ...current,
                status: "installed",
                error: undefined,
                visible: true,
                manual: true,
              }
            : current
        );
        setLastFailedAction(null);
      } else if (lastFailedAction === "install") {
        const result = await installUpdate();
        setState(result);
        setLastFailedAction(actionFromResult(result, "install"));
      } else {
        const result = await checkForUpdates(true);
        setState(result);
        setLastFailedAction(actionFromResult(result, "check"));
      }
    } catch (error) {
      setLastFailedAction(lastFailedAction ?? "check");
      setState((current) =>
        current
          ? {
              ...current,
              status: "error",
              error: errorMessage(error, "Failed to retry the update."),
              visible: true,
              manual: true,
            }
          : current
      );
    } finally {
      setActionBusy(false);
    }
  };

  const handleRestart = async () => {
    setActionBusy(true);
    try {
      await restartApp();
      setLastFailedAction(null);
      setState((current) =>
        current
          ? {
              ...current,
              status: "installed",
              error: undefined,
              visible: true,
              manual: true,
            }
          : current
      );
    } catch (error) {
      setLastFailedAction("restart");
      setState((current) =>
        current
          ? {
              ...current,
              status: "error",
              error: errorMessage(error, "Failed to restart the app."),
              visible: true,
              manual: true,
            }
          : current
      );
    } finally {
      setActionBusy(false);
    }
  };

  return (
    <div className="border-b bg-muted/40 px-6 py-2.5">
      <div className="flex min-h-10 items-center justify-between gap-4">
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border bg-background">
            {statusIcon}
          </div>
          <div className="min-w-0" role="status" aria-live="polite">
            <div className="truncate text-sm font-semibold">{title}</div>
            {detail && (
              <div className="mt-0.5 max-w-[460px] truncate text-xs text-muted-foreground">
                {detail}
              </div>
            )}
            {isInstalling && percent !== null && (
              <div className="mt-1 h-1.5 w-56 overflow-hidden rounded-sm bg-border">
                <div
                  className="h-full bg-foreground transition-[width] duration-200"
                  style={{ width: `${percent}%` }}
                />
              </div>
            )}
          </div>
        </div>

        {state.status === "available" && (
          <Button size="sm" onClick={handleInstall} disabled={actionBusy}>
            <Download className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
            Install
          </Button>
        )}
        {state.status === "installing" && (
          <Button size="sm" variant="outline" disabled>
            <RefreshCw className="mr-2 h-3.5 w-3.5 animate-spin motion-reduce:animate-none" aria-hidden="true" />
            Installing
          </Button>
        )}
        {state.status === "installed" && (
          <Button size="sm" onClick={handleRestart} disabled={actionBusy}>
            <RotateCcw className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
            Restart
          </Button>
        )}
        {state.status === "error" && (
          <Button size="sm" variant="outline" onClick={handleRetry} disabled={actionBusy}>
            <RefreshCw className="mr-2 h-3.5 w-3.5" aria-hidden="true" />
            Retry
          </Button>
        )}
      </div>
    </div>
  );
}
