import { useEffect, useMemo, useRef, useState } from "react";
import {
  getRecentLogs,
  getServerStatus,
  isTauriRuntime,
  startServer,
  stopServer,
  type LogEntry,
  type ServerStatus,
} from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Play, Square, AlertCircle, Waves } from "lucide-react";

function formatUptime(secs: number): string {
  if (secs <= 0) return "0s";
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

type Tone = "running" | "error" | "offline";

function Sparkline({ values }: { values: number[] }) {
  const points = useMemo(() => {
    if (values.length < 2) return null;
    const max = Math.max(...values);
    const min = Math.min(...values);
    const span = max - min || 1;
    const step = 100 / (values.length - 1);
    return values.map((v, i) => {
      const x = i * step;
      const y = 100 - ((v - min) / span) * 100;
      return `${x.toFixed(2)},${y.toFixed(2)}`;
    });
  }, [values]);

  if (!points) return null;
  const line = points.join(" ");
  const area = `0,100 ${line} 100,100`;

  return (
    <svg
      viewBox="0 0 100 100"
      preserveAspectRatio="none"
      className="h-full w-full text-primary"
      aria-hidden="true"
    >
      <defs>
        <linearGradient id="spark-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="currentColor" stopOpacity="0.16" />
          <stop offset="100%" stopColor="currentColor" stopOpacity="0" />
        </linearGradient>
      </defs>
      <polygon points={area} fill="url(#spark-fill)" />
      <polyline
        points={line}
        fill="none"
        stroke="currentColor"
        strokeWidth={1.75}
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function MetaStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 px-4 py-2.5">
      <div className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
        {label}
      </div>
      <div className="mt-0.5 truncate font-mono text-sm font-semibold tabular-nums text-foreground">
        {value}
      </div>
    </div>
  );
}

export default function Home() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [logs, setLogs] = useState<LogEntry[]>([]);
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
      if (clearActionError) setActionError(null);
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
          if (!res.error) setActionError(null);
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

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        const next = await getRecentLogs(60);
        if (active) setLogs(next);
      } catch {
        if (active) setLogs([]);
      }
    };
    refresh();
    const interval = setInterval(refresh, 3000);
    return () => {
      active = false;
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

  const traffic = useMemo(() => {
    if (logs.length === 0) return { count: 0, avg: 0, series: [] as number[] };
    const ordered = [...logs].sort(
      (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );
    const durations = ordered.map((l) => l.duration_ms);
    const avg = Math.round(durations.reduce((s, d) => s + d, 0) / durations.length);
    return { count: logs.length, avg, series: durations.slice(-32) };
  }, [logs]);

  if (!status) {
    return (
      <div className="flex h-full flex-col gap-3 p-4">
        <div className="h-[132px] animate-pulse rounded-lg border bg-card" />
        <div className="h-[104px] animate-pulse rounded-lg border bg-card" />
        <div className="min-h-0 flex-1 animate-pulse rounded-lg border bg-card" />
      </div>
    );
  }

  const hasServerError = Boolean(status.error);
  const hasError = hasServerError || Boolean(actionError);
  const canControlServer = isTauriRuntime();
  const tone: Tone = hasServerError ? "error" : status.running ? "running" : "offline";

  const headline =
    tone === "running" ? "Server running" : tone === "error" ? "Server error" : "Server offline";
  const dotClass = {
    running: "bg-success",
    error: "bg-destructive-text",
    offline: "bg-muted-foreground/50",
  }[tone];

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <Card className="overflow-hidden">
        <CardContent className="p-4">
          <div className="flex items-start justify-between gap-4">
            <div className="flex min-w-0 items-center gap-3">
              <span className="relative flex h-3 w-3 shrink-0">
                {tone === "running" && (
                  <span
                    aria-hidden="true"
                    className="absolute inline-flex h-full w-full animate-ping rounded-full bg-success opacity-40 [animation-duration:2.5s] motion-reduce:hidden"
                  />
                )}
                <span className={cn("h-3 w-3 rounded-full", dotClass)} />
              </span>
              <div className="min-w-0">
                <h2 className="truncate text-lg font-semibold tracking-tight text-foreground">
                  {headline}
                </h2>
                <p className="mt-0.5 truncate font-mono text-xs text-muted-foreground">
                  localhost:{status.port}
                </p>
              </div>
            </div>

            {!canControlServer ? (
              <Button
                variant="outline"
                size="sm"
                className="h-8 shrink-0 px-3 text-xs"
                disabled
                aria-label={`${status.running ? "Running" : "Offline"} — server controls are available in the Tauri app window`}
                title="Server controls are available in the Tauri app window."
              >
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
                <Square aria-hidden="true" className="mr-1.5 h-3 w-3 fill-current" />
                {loading ? "Stopping" : "Stop"}
              </Button>
            ) : (
              <Button
                size="sm"
                className="h-8 shrink-0 px-3 text-xs"
                onClick={handleStart}
                disabled={loading}
              >
                <Play aria-hidden="true" className="mr-1.5 h-3 w-3 fill-current" />
                {loading ? "Starting" : "Start"}
              </Button>
            )}
          </div>

          {hasError && (
            <div
              role="alert"
              className="mt-3 flex items-center gap-2 rounded-md border border-destructive-text/20 bg-destructive-text/10 px-3 py-2 text-xs font-medium text-destructive-text"
            >
              <AlertCircle aria-hidden="true" className="h-3.5 w-3.5 shrink-0" />
              <span className="truncate">{status.error || actionError}</span>
            </div>
          )}
        </CardContent>

        <div className="grid grid-cols-3 divide-x divide-border border-t">
          <MetaStat label="Port" value={String(status.port)} />
          <MetaStat label="Uptime" value={formatUptime(status.uptime_secs)} />
          <MetaStat label="Auth" value={status.auth_mode} />
        </div>
      </Card>

      <Card>
        <CardContent className="flex items-center gap-4 p-4">
          <div className="min-w-0">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              Recent traffic
            </div>
            <div className="mt-1 flex items-baseline gap-1.5">
              <span className="font-mono text-2xl font-semibold tabular-nums text-foreground">
                {traffic.count}
              </span>
              <span className="text-xs text-muted-foreground">requests</span>
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              {traffic.count > 0 ? (
                <>avg {traffic.avg} ms</>
              ) : (
                "Waiting for requests"
              )}
            </div>
          </div>
          <div className="h-12 min-w-0 flex-1">
            {traffic.series.length >= 2 ? (
              <Sparkline values={traffic.series} />
            ) : (
              <div className="flex h-full items-center justify-end gap-1.5 text-xs text-muted-foreground/70">
                <Waves aria-hidden="true" className="h-4 w-4" />
                No traffic yet
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      <Card className="min-h-0 flex-1">
        <CardContent className="flex h-full flex-col p-0">
          <div className="flex h-10 shrink-0 items-center justify-between border-b px-4">
            <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              Endpoints
            </div>
            <Badge variant="secondary" className="h-5 rounded px-1.5 font-mono text-[10px]">
              localhost:{status.port}
            </Badge>
          </div>
          <div className="divide-y divide-border">
            {ENDPOINTS.map((endpoint) => (
              <div
                key={`${endpoint.method}:${endpoint.path}`}
                className="flex items-center gap-3 px-4 py-2.5"
              >
                <Badge
                  variant="outline"
                  className="h-5 w-11 shrink-0 justify-center rounded px-0 font-mono text-[10px]"
                >
                  {endpoint.method}
                </Badge>
                <span className="truncate font-mono text-xs text-foreground">
                  {endpoint.path}
                </span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
