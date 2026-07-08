import { useEffect, useRef, useState } from "react";
import {
  checkForUpdates,
  getServerStatus,
  getSettings,
  isTauriRuntime,
  listen,
  updateSetting,
} from "@/lib/tauri";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Badge } from "@/components/ui/badge";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
  AlertCircle,
  Download,
  FileKey2,
  Gauge,
  Power,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface SettingsState {
  port: string;
  auth_path: string;
  auto_start: string;
  log_level: string;
}

type SettingsError = {
  key: keyof SettingsState | "load";
  message: string;
};

const DEFAULT_SETTINGS: SettingsState = {
  port: "11434",
  auth_path: "~/.codex/auth.json",
  auto_start: "true",
  log_level: "info",
};

const SETTING_KEYS = new Set<keyof SettingsState>([
  "port",
  "auth_path",
  "auto_start",
  "log_level",
]);

function isSettingsKey(key: string): key is keyof SettingsState {
  return SETTING_KEYS.has(key as keyof SettingsState);
}

function getErrorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "string") return error;
  return String(error);
}

export default function Settings() {
  const [settings, setSettings] = useState<SettingsState>(DEFAULT_SETTINGS);
  const [savedPort, setSavedPort] = useState(DEFAULT_SETTINGS.port);
  const [savedAuthPath, setSavedAuthPath] = useState(DEFAULT_SETTINGS.auth_path);
  const [serverRunning, setServerRunning] = useState(false);
  const [authMode, setAuthMode] = useState("");
  const [tab, setTab] = useState<"general" | "codex">("general");
  const [portChanged, setPortChanged] = useState(false);
  const [authPathChanged, setAuthPathChanged] = useState(false);
  const [error, setError] = useState<SettingsError | null>(null);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateCheckMessage, setUpdateCheckMessage] = useState<string | null>(null);
  const dirtyRef = useRef<Record<"port" | "auth_path", boolean>>({
    port: false,
    auth_path: false,
  });
  const saveSeqRef = useRef<Record<keyof SettingsState, number>>({
    port: 0,
    auth_path: 0,
    auto_start: 0,
    log_level: 0,
  });
  const mountedRef = useRef(true);
  const canEditSettings = isTauriRuntime();
  const runtimeSettingsLocked = canEditSettings && serverRunning;
  const runtimeInputDisabled = !canEditSettings || runtimeSettingsLocked;
  let settingsHeaderBadge: string | null = null;
  if (runtimeSettingsLocked) {
    settingsHeaderBadge = "Stop server to edit";
  } else if (!canEditSettings) {
    settingsHeaderBadge = "Read only";
  }

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    (async () => {
      try {
        const rows = await getSettings();
        const map: Partial<SettingsState> = {};
        for (const row of rows) {
          if (isSettingsKey(row.key)) {
            map[row.key] = row.value;
          }
        }
        if (active) {
          const nextSettings = { ...DEFAULT_SETTINGS, ...map };
          setSettings((prev) => ({
            ...nextSettings,
            port: dirtyRef.current.port ? prev.port : nextSettings.port,
            auth_path: dirtyRef.current.auth_path ? prev.auth_path : nextSettings.auth_path,
          }));
          if (!dirtyRef.current.auth_path) {
            setSavedAuthPath(nextSettings.auth_path);
            setAuthPathChanged(false);
          }
          if (!dirtyRef.current.port) {
            setSavedPort(nextSettings.port);
            setPortChanged(false);
          }
          setError(null);
        }
      } catch {
        if (active) {
          setError({
            key: "load",
            message: "Failed to load settings. Please check the app logs for details.",
          });
        }
      }
    })();

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;

    const refreshStatus = async () => {
      try {
        const status = await getServerStatus();
        if (active) {
          const nextPort = String(status.port);
          setServerRunning(status.running);
          setAuthMode(status.auth_mode);
          if (!dirtyRef.current.port) {
            setSettings((prev) => ({ ...prev, port: nextPort }));
            setSavedPort(nextPort);
            setPortChanged(false);
          }
        }
      } catch {
        if (active) {
          setServerRunning(false);
        }
      }
    };

    refreshStatus();
    const interval = setInterval(refreshStatus, 2000);
    const unlisten = listen("server-status-changed", refreshStatus);
    return () => {
      active = false;
      clearInterval(interval);
      unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);

  useEffect(() => {
    if (!runtimeSettingsLocked) return;
    dirtyRef.current.port = false;
    dirtyRef.current.auth_path = false;
    setPortChanged(false);
    setAuthPathChanged(false);
    setSettings((prev) => ({
      ...prev,
      port: savedPort,
      auth_path: savedAuthPath,
    }));
    setError((prev) => (
      prev?.key === "port" || prev?.key === "auth_path" ? null : prev
    ));
  }, [runtimeSettingsLocked, savedPort, savedAuthPath]);

  useEffect(() => {
    if (!error) return;
    setTab(error.key === "auth_path" ? "codex" : "general");
  }, [error]);

  const save = async (key: keyof SettingsState, value: string): Promise<boolean> => {
    if ((key === "port" || key === "auth_path") && runtimeSettingsLocked) {
      setSettings((prev) => ({
        ...prev,
        port: savedPort,
        auth_path: savedAuthPath,
      }));
      dirtyRef.current.port = false;
      dirtyRef.current.auth_path = false;
      setPortChanged(false);
      setAuthPathChanged(false);
      setError({
        key,
        message: "Stop the server before changing runtime settings.",
      });
      return false;
    }

    if (key === "port" && value === savedPort) {
      dirtyRef.current.port = false;
      setPortChanged(false);
      setError((prev) => (prev?.key === key ? null : prev));
      return true;
    }
    if (key === "auth_path" && value === savedAuthPath) {
      dirtyRef.current.auth_path = false;
      setAuthPathChanged(false);
      setError((prev) => (prev?.key === key ? null : prev));
      return true;
    }

    if (key === "port") {
      const port = Number(value);
      if (!Number.isInteger(port) || port < 1 || port > 65535) {
        setError({ key, message: "Port must be between 1 and 65535" });
        return false;
      }
    }

    const saveSeq = saveSeqRef.current[key] + 1;
    saveSeqRef.current[key] = saveSeq;

    try {
      await updateSetting(key, value);
      if (saveSeqRef.current[key] !== saveSeq) return true;
      setError((prev) => (prev?.key === key ? null : prev));
      if (key === "port") {
        setSavedPort(value);
        dirtyRef.current.port = false;
        setPortChanged(false);
      } else if (key === "auth_path") {
        setAuthPathChanged(value !== savedAuthPath);
        setSavedAuthPath(value);
        dirtyRef.current.auth_path = false;
      }
      return true;
    } catch (err) {
      if (saveSeqRef.current[key] !== saveSeq) return true;
      setError({
        key,
        message: getErrorMessage(err),
      });
      return false;
    }
  };

  const checkUpdates = async () => {
    setCheckingUpdates(true);
    setUpdateCheckMessage(null);
    try {
      const result = await checkForUpdates(true);
      if (!mountedRef.current) return;
      if (result.status === "available" && result.version) {
        setUpdateCheckMessage(`Version ${result.version} is available.`);
      } else if (result.status === "error") {
        setUpdateCheckMessage(
          result.error ??
            "Update check failed. Please try again or install the latest release manually."
        );
      } else if (result.status === "checking") {
        setUpdateCheckMessage("Update check is already in progress.");
      } else if (result.status === "installing") {
        setUpdateCheckMessage("Update installation is in progress.");
      } else if (result.status === "installed") {
        setUpdateCheckMessage("Update installed. Restart the app to finish applying it.");
      } else {
        setUpdateCheckMessage("Up to date.");
      }
    } catch (err) {
      if (mountedRef.current) {
        setUpdateCheckMessage(getErrorMessage(err));
      }
    } finally {
      if (mountedRef.current) {
        setCheckingUpdates(false);
      }
    }
  };

  const generalError = error && error.key !== "auth_path" ? error : null;
  const codexError = error && error.key === "auth_path" ? error : null;

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as "general" | "codex")}
      className="flex h-full flex-col gap-3 p-4"
    >
      <div className="flex h-11 shrink-0 items-center justify-between rounded-lg border bg-card px-2">
        <TabsList>
          <TabsTrigger value="general">
            <SlidersHorizontal aria-hidden="true" className="h-3.5 w-3.5" />
            General
          </TabsTrigger>
          <TabsTrigger value="codex">
            <FileKey2 aria-hidden="true" className="h-3.5 w-3.5" />
            Codex
          </TabsTrigger>
        </TabsList>
        {settingsHeaderBadge && (
          <span className="pr-2 text-[11px] font-medium text-muted-foreground">
            {settingsHeaderBadge}
          </span>
        )}
      </div>

      <TabsContent value="general" className="min-h-0 flex-1">
        <Card className="h-full overflow-hidden">
          <CardContent className="p-0 divide-y divide-border">
            {generalError && (
              <div role="alert" className="flex items-center gap-2 bg-destructive-text/10 px-4 py-3 text-xs text-destructive-text">
                <AlertCircle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                {generalError.message}
              </div>
            )}
            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <label htmlFor="setting-port" className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <Gauge className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Port
                </label>
                <p className="mt-1 text-xs text-muted-foreground">Proxy listen port</p>
              </div>
              <Input
                id="setting-port"
                type="number"
                min={1}
                max={65535}
                value={settings.port}
                onChange={(e) => {
                  setSettings((prev) => ({ ...prev, port: e.target.value }));
                  dirtyRef.current.port = true;
                  setPortChanged(true);
                }}
                onBlur={() => void save("port", settings.port)}
                disabled={runtimeInputDisabled}
                className="w-32 font-mono"
              />
              {portChanged && (
                <p className="col-start-2 flex items-center gap-1.5 text-xs text-warning">
                  <AlertCircle className="h-3 w-3 shrink-0" aria-hidden="true" />
                  Restart required for port change
                </p>
              )}
            </div>

            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <label htmlFor="setting-auto-start" className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <Power className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Auto Start
                </label>
                <p className="mt-1 text-xs text-muted-foreground">Launch at login</p>
              </div>
              <Switch
                id="setting-auto-start"
                aria-label="Launch at login"
                checked={settings.auto_start === "true"}
                disabled={!canEditSettings}
                onCheckedChange={(checked) => {
                  const value = checked ? "true" : "false";
                  const previousValue = settings.auto_start;
                  setSettings((prev) => ({ ...prev, auto_start: value }));
                  void save("auto_start", value).then((saved) => {
                    if (!saved) {
                      setSettings((prev) => ({ ...prev, auto_start: previousValue }));
                    }
                  });
                }}
              />
            </div>

            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <label className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <Download className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Updates
                </label>
                <p className="mt-1 text-xs text-muted-foreground">GitHub Releases channel</p>
              </div>
              <div className="flex min-w-0 items-center gap-3">
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  onClick={() => void checkUpdates()}
                  disabled={!canEditSettings || checkingUpdates}
                >
                  <RefreshCw
                    aria-hidden="true"
                    className={`mr-2 h-3.5 w-3.5 ${checkingUpdates ? "animate-spin motion-reduce:animate-none" : ""}`}
                  />
                  Check
                </Button>
                {updateCheckMessage && (
                  <span className="truncate text-xs text-muted-foreground">
                    {updateCheckMessage}
                  </span>
                )}
              </div>
            </div>

            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <label htmlFor="setting-log-level" className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <SlidersHorizontal className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Log Level
                </label>
                <p className="mt-1 text-xs text-muted-foreground">Server log verbosity</p>
              </div>
              <Select value={settings.log_level} disabled>
                <SelectTrigger id="setting-log-level" aria-label="Log level" className="w-32">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="debug">debug</SelectItem>
                  <SelectItem value="info">info</SelectItem>
                  <SelectItem value="warn">warn</SelectItem>
                  <SelectItem value="error">error</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </CardContent>
        </Card>
      </TabsContent>

      <TabsContent value="codex" className="min-h-0 flex-1">
        <Card className="h-full overflow-hidden">
          <CardContent className="p-0 divide-y divide-border">
            {codexError && (
              <div role="alert" className="flex items-center gap-2 bg-destructive-text/10 px-4 py-3 text-xs text-destructive-text">
                <AlertCircle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
                {codexError.message}
              </div>
            )}
            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <label htmlFor="setting-auth-path" className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <FileKey2 className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Auth File
                </label>
                <p className="mt-1 text-xs text-muted-foreground">Codex credential path</p>
              </div>
              <Input
                id="setting-auth-path"
                type="text"
                value={settings.auth_path}
                onChange={(e) => {
                  setSettings((prev) => ({ ...prev, auth_path: e.target.value }));
                  dirtyRef.current.auth_path = true;
                  setAuthPathChanged(true);
                }}
                onBlur={() => void save("auth_path", settings.auth_path)}
                disabled={runtimeInputDisabled}
                className="font-mono text-xs"
                placeholder="~/.codex/auth.json"
              />
              {authPathChanged && (
                <p className="col-start-2 flex items-center gap-1.5 text-xs text-warning">
                  <AlertCircle className="h-3 w-3 shrink-0" aria-hidden="true" />
                  Auth file updated
                </p>
              )}
            </div>

            <div className="grid grid-cols-[180px_minmax(0,1fr)] items-center gap-4 p-4">
              <div className="min-w-0">
                <div className="flex items-center gap-2 text-sm font-medium text-foreground">
                  <ShieldCheck className="h-4 w-4 text-muted-foreground" aria-hidden="true" />
                  Mode
                </div>
                <p className="mt-1 text-xs text-muted-foreground">Resolved credential type</p>
              </div>
              <Badge variant="outline" className="h-6 w-fit rounded-md px-2 font-mono text-[11px]">
                {authMode || "—"}
              </Badge>
            </div>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  );
}
