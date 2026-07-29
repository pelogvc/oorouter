import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import {
  getRecentLogs,
  isTauriRuntime,
  listen,
  type LogEntry,
  type UnlistenFn,
} from "@/lib/tauri";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ScrollText, Trash2, Activity } from "lucide-react";

function getStatusClasses(status: number): string {
  if (status >= 200 && status < 300) return "bg-success/10 text-success border-success/25";
  if (status >= 400 && status < 500) return "bg-warning/10 text-warning border-warning/25";
  if (status >= 500) return "bg-destructive-text/10 text-destructive-text border-destructive-text/25";
  return "";
}

function formatLogTime(timestamp: string): string {
  const date = new Date(timestamp);
  const hours = date.getHours().toString().padStart(2, "0");
  const minutes = date.getMinutes().toString().padStart(2, "0");
  const seconds = date.getSeconds().toString().padStart(2, "0");
  return `${hours}:${minutes}:${seconds}`;
}

export default function Logs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [modelFilter, setModelFilter] = useState<string>("all");
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let unlisten: UnlistenFn | undefined;
    let interval: ReturnType<typeof setInterval> | undefined;
    const shouldPoll = !isTauriRuntime();

    const refreshLogs = async () => {
      const nextLogs = await getRecentLogs(100);
      if (active) {
        setLogs(nextLogs);
      }
    };

    const init = async () => {
      try {
        await refreshLogs();

        if (shouldPoll) {
          interval = setInterval(() => {
            refreshLogs().catch(() => undefined);
          }, 1500);
        } else {
          unlisten = await listen<LogEntry>("log-entry", (event) => {
            setLogs((prev) => {
              const newLogs = [event.payload, ...prev];
              return newLogs.length > 500 ? newLogs.slice(0, 500) : newLogs;
            });
          });
          if (!active) {
            unlisten();
          }
        }
      } catch {
        if (active) {
          setError("Failed to load logs. Please try again.");
        }
      }
    };

    init();
    return () => {
      active = false;
      if (interval) {
        clearInterval(interval);
      }
      unlisten?.();
    };
  }, []);

  const uniqueModels = Array.from(new Set(logs.map((l) => l.model).filter(Boolean))) as string[];

  const filteredLogs = logs.filter((log) => {
    if (modelFilter !== "all" && log.model !== modelFilter) return false;
    if (statusFilter === "2xx" && (log.status < 200 || log.status >= 300)) return false;
    if (statusFilter === "4xx" && (log.status < 400 || log.status >= 500)) return false;
    if (statusFilter === "5xx" && log.status < 500) return false;
    return true;
  });

  return (
    <div className="flex h-full flex-col">
      <div className="flex h-[57px] shrink-0 items-center justify-between gap-2 overflow-hidden border-b px-3">
        <div className="flex min-w-0 items-center gap-2 overflow-hidden">
          <div className="flex shrink-0 items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            <Activity aria-hidden="true" className="h-3.5 w-3.5" />
            Traffic
          </div>
          <Select value={modelFilter} onValueChange={setModelFilter}>
            <SelectTrigger aria-label="Filter by model" className="h-8 w-[140px] shrink-0 text-xs">
              <SelectValue placeholder="All Models" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Models</SelectItem>
              {uniqueModels.map((model) => (
                <SelectItem key={model} value={model}>{model}</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger aria-label="Filter by status" className="h-8 w-[120px] shrink-0 text-xs">
              <SelectValue placeholder="All Status" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Status</SelectItem>
              <SelectItem value="2xx">2xx Success</SelectItem>
              <SelectItem value="4xx">4xx Client</SelectItem>
              <SelectItem value="5xx">5xx Server</SelectItem>
            </SelectContent>
          </Select>

          <Badge variant="secondary" className="shrink-0 font-mono text-xs">
            {filteredLogs.length}/{logs.length}
          </Badge>
          {error && (
            <Badge
              variant="outline"
              role="alert"
              className="min-w-0 truncate border-destructive-text/30 text-xs text-destructive-text"
            >
              {error}
            </Badge>
          )}
        </div>

        <Button
          variant="outline"
          size="sm"
          className="h-8 shrink-0 cursor-pointer text-xs text-destructive-text hover:text-destructive-text"
          onClick={() => setLogs([])}
        >
          <Trash2 aria-hidden="true" className="w-3.5 h-3.5 mr-1.5" />
          Clear
        </Button>
      </div>

      <div className="flex-1 overflow-hidden">
        <ScrollArea type="always" className="h-full">
          <Table className="table-fixed" containerClassName="overflow-visible">
            <TableHeader className="sticky top-0 z-10 bg-background">
              <TableRow>
                <TableHead className="h-9 w-[72px] px-3 text-[11px] uppercase tracking-wider">Time</TableHead>
                <TableHead className="h-9 px-3 text-[11px] uppercase tracking-wider">Request</TableHead>
                <TableHead className="h-9 w-[76px] px-3 text-[11px] uppercase tracking-wider">Model</TableHead>
                <TableHead className="h-9 w-[58px] px-3 text-[11px] uppercase tracking-wider">Status</TableHead>
                <TableHead className="h-9 w-[96px] px-3 text-right text-[11px] uppercase tracking-wider">Tokens</TableHead>
                <TableHead className="h-9 w-[64px] px-3 text-right text-[11px] uppercase tracking-wider">MS</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredLogs.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={6} className="h-48">
                    <div className="flex flex-col items-center gap-2 text-center">
                      <ScrollText aria-hidden="true" className="w-8 h-8 text-muted-foreground/30" />
                      <p className="text-sm text-muted-foreground">No logs recorded yet</p>
                    </div>
                  </TableCell>
                </TableRow>
              ) : (
                filteredLogs.map((log, index) => (
                  <TableRow key={`${log.id}-${index}`}>
                    <TableCell className="px-3 py-2 font-mono text-xs text-muted-foreground">
                      {formatLogTime(log.timestamp)}
                    </TableCell>
                    <TableCell className="min-w-0 px-3 py-2" title={`${log.method} ${log.path}`}>
                      <div className="flex min-w-0 items-center gap-2">
                        <Badge variant="outline" className="h-5 shrink-0 rounded px-1.5 font-mono text-[10px]">
                          {log.method}
                        </Badge>
                        <span className="truncate font-mono text-xs">{log.path}</span>
                      </div>
                    </TableCell>
                    <TableCell className="max-w-[76px] truncate px-3 py-2 text-xs" title={log.model || "-"}>
                      {log.model || <span className="text-muted-foreground">—</span>}
                    </TableCell>
                    <TableCell className="px-3 py-2">
                      <Badge variant="outline" className={cn("font-mono text-[10px]", getStatusClasses(log.status))}>
                        {log.status}
                      </Badge>
                    </TableCell>
                    <TableCell
                      className="max-w-[96px] truncate px-3 py-2 text-right font-mono text-xs text-muted-foreground"
                      title={`${log.input_tokens ?? "—"}/${log.output_tokens ?? "—"}`}
                    >
                      {log.input_tokens ?? "—"}/{log.output_tokens ?? "—"}
                    </TableCell>
                    <TableCell
                      className="max-w-[64px] truncate px-3 py-2 text-right font-mono text-xs"
                      title={String(log.duration_ms)}
                    >
                      {log.duration_ms}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </ScrollArea>
      </div>
    </div>
  );
}
