import { useEffect, useRef, useState } from "react";
import {
  getServerStatus,
  isTauriRuntime,
  startServer,
  stopServer,
  type ServerStatus,
} from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Play,
  Square,
  Server,
  Clock,
  Activity,
  AlertCircle,
  Route,
} from "lucide-react";

function formatUptime(secs: number): string {
  if (secs === 0) return "0s";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;

  const parts = [];
  if (h > 0) parts.push(`${h}h`);
  if (m > 0) parts.push(`${m}m`);
  if (s > 0 || parts.length === 0) parts.push(`${s}s`);

  return parts.join(" ");
}

const ENDPOINTS = [
  { method: "GET", path: "/api/tags" },
  { method: "GET", path: "/v1/models" },
  { method: "POST", path: "/v1/chat/completions" },
];

export default function Home() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const statusRequestSeqRef = useRef(0);

  const fetchStatus = async ({ clearActionError = true } = {}) => {
    const requestSeq = statusRequestSeqRef.current + 1;
    statusRequestSeqRef.current = requestSeq;
    try {
      const res = await getServerStatus();
      if (statusRequestSeqRef.current !== requestSeq) return;
      setStatus(res);
      if (clearActionError) {
        setActionError(null);
      }
    } catch (err) {
      if (statusRequestSeqRef.current !== requestSeq) return;
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      const requestSeq = statusRequestSeqRef.current + 1;
      statusRequestSeqRef.current = requestSeq;
      try {
        const res = await getServerStatus();
        if (active && statusRequestSeqRef.current === requestSeq) {
          setStatus(res);
          if (!res.error) {
            setActionError(null);
          }
        }
      } catch (err) {
        if (active && statusRequestSeqRef.current === requestSeq) {
          setActionError(err instanceof Error ? err.message : String(err));
        }
      }
    };

    refresh();
    const interval = setInterval(refresh, 2000);
    return () => {
      active = false;
      statusRequestSeqRef.current += 1;
      clearInterval(interval);
    };
  }, []);

  const handleStart = async () => {
    setLoading(true);
    setActionError(null);
    try {
      await startServer();
      await fetchStatus();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      await fetchStatus({ clearActionError: false });
      setActionError(message);
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true);
    setActionError(null);
    try {
      await stopServer();
      await fetchStatus();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  if (!status) {
    return (
      <div className="flex h-full flex-col gap-4 p-4">
        <div className="h-28 rounded-lg border bg-card p-4">
          <div className="h-4 w-24 animate-pulse rounded bg-muted" />
          <div className="mt-4 h-7 w-48 animate-pulse rounded bg-muted" />
          <div className="mt-2 h-4 w-64 animate-pulse rounded bg-muted" />
        </div>
        <div className="grid grid-cols-2 gap-3">
          {[1, 2].map((i) => (
            <div key={i} className="h-24 rounded-lg border bg-card p-4">
              <div className="h-3 w-14 animate-pulse rounded bg-muted" />
              <div className="mt-4 h-6 w-20 animate-pulse rounded bg-muted" />
            </div>
          ))}
        </div>
        <div className="min-h-32 flex-1 rounded-lg border bg-card p-4" />
      </div>
    );
  }

  const hasServerError = Boolean(status.error);
  const hasActionError = Boolean(actionError);
  const hasError = hasServerError || hasActionError;
  const canControlServer = isTauriRuntime();
  let statusBarClassName = "bg-muted";
  let statusIconClassName = "border-muted bg-muted text-muted-foreground";
  let statusBadgeClassName = "border-border bg-muted text-muted-foreground";
  let headline = "Server Offline";
  let description = "Port is closed";

  if (hasServerError) {
    statusBarClassName = "bg-destructive";
    statusIconClassName = "border-destructive/20 bg-destructive/10 text-destructive";
    statusBadgeClassName = "border-destructive/30 bg-destructive/10 text-destructive";
    headline = "Error";
    description = "Runtime reported a server error";
  } else if (status.running) {
    statusBarClassName = "bg-emerald-500 dark:bg-emerald-400";
    statusIconClassName =
      "border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400";
    statusBadgeClassName =
      "border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
    headline = "Server Running";
    description = `Listening on localhost:${status.port}`;
  }

  return (
    <div className="flex h-full flex-col gap-4 p-4">
      <Card className="overflow-hidden">
        <div className={`h-1 w-full ${statusBarClassName}`} />
        <CardContent className="p-0">
          <div className="flex items-center justify-between gap-4 p-4">
            <div className="flex min-w-0 items-center gap-3">
              <div className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-md border ${statusIconClassName}`}>
                <Activity className="h-5 w-5" />
              </div>
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <h2 className="truncate text-base font-semibold text-foreground">{headline}</h2>
                  <Badge variant="outline" className={`h-5 rounded px-1.5 text-[10px] uppercase ${statusBadgeClassName}`}>
                    {status.running ? "Online" : hasServerError ? "Error" : "Offline"}
                  </Badge>
                </div>
                <p className="mt-0.5 truncate text-xs text-muted-foreground">{description}</p>
              </div>
            </div>

            {!canControlServer ? (
              <Button
                variant="outline"
                size="sm"
                className="h-8 shrink-0 px-3 text-xs"
                disabled
                title="Server controls are available in the Tauri app window."
              >
                {status.running ? (
                  <Square className="mr-1.5 h-3.5 w-3.5 fill-current" />
                ) : (
                  <Play className="mr-1.5 h-3.5 w-3.5 fill-current" />
                )}
                {status.running ? "Running" : "Offline"}
              </Button>
            ) : status.running ? (
              <Button
                variant="destructive"
                size="sm"
                className="h-8 shrink-0 px-3 text-xs"
                onClick={handleStop}
                disabled={loading}
              >
                <Square className="mr-1.5 h-3.5 w-3.5 fill-current" />
                {loading ? "Stopping" : "Stop"}
              </Button>
            ) : (
              <Button
                size="sm"
                className="h-8 shrink-0 bg-emerald-600 px-3 text-xs text-white hover:bg-emerald-700"
                onClick={handleStart}
                disabled={loading}
              >
                <Play className="mr-1.5 h-3.5 w-3.5 fill-current" />
                {loading ? "Starting" : "Start"}
              </Button>
            )}
          </div>

          {hasError && (
            <div className="flex items-center gap-2 border-t border-destructive/20 bg-destructive/10 px-4 py-2 text-xs font-medium text-destructive">
              <AlertCircle className="h-3.5 w-3.5 shrink-0" />
              {status.error || actionError}
            </div>
          )}
        </CardContent>
      </Card>

      <div className="grid grid-cols-2 gap-3">
        <Card>
          <CardContent className="flex h-24 flex-col justify-between p-4">
            <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
              <Server className="h-3.5 w-3.5" />
              Port
            </div>
            <div className="font-mono text-2xl font-semibold text-foreground">
              {status.port}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="flex h-24 flex-col justify-between p-4">
            <div className="flex items-center gap-2 text-xs font-medium uppercase text-muted-foreground">
              <Clock className="h-3.5 w-3.5" />
              Uptime
            </div>
            <div className="font-mono text-2xl font-semibold text-foreground">
              {formatUptime(status.uptime_secs)}
            </div>
          </CardContent>
        </Card>
      </div>

      <Card className="min-h-0 flex-1">
        <CardContent className="flex h-full flex-col p-0">
          <div className="flex h-10 shrink-0 items-center justify-between border-b px-4">
            <div className="flex items-center gap-2 text-xs font-semibold uppercase text-muted-foreground">
              <Route className="h-3.5 w-3.5" />
              Routes
            </div>
            <Badge variant="secondary" className="h-5 rounded px-1.5 font-mono text-[10px]">
              localhost:{status.port}
            </Badge>
          </div>
          <div className="divide-y divide-border">
            {ENDPOINTS.map((endpoint) => (
              <div key={`${endpoint.method}:${endpoint.path}`} className="grid grid-cols-[60px_minmax(0,1fr)] items-center gap-3 px-4 py-3">
                <Badge variant="outline" className="h-5 w-fit rounded px-1.5 font-mono text-[10px]">
                  {endpoint.method}
                </Badge>
                <span className="truncate font-mono text-xs text-foreground">{endpoint.path}</span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
