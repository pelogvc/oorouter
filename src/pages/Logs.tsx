import { useEffect, useState, useRef } from "react";
import { cn } from "@/lib/utils";
import { getRecentLogs, listen, type UnlistenFn } from "@/lib/tauri";
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
import { ScrollText, Trash2 } from "lucide-react";

interface LogEntry {
  id: string;
  timestamp: string;
  method: string;
  path: string;
  model?: string;
  status: number;
  duration_ms: number;
  input_tokens?: number;
  output_tokens?: number;
}

function getStatusClasses(status: number): string {
  if (status >= 200 && status < 300) return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20";
  if (status >= 400 && status < 500) return "bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/20";
  if (status >= 500) return "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/20";
  return "";
}

export default function Logs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [modelFilter, setModelFilter] = useState<string>("all");
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const autoScroll = true;
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const init = async () => {
      try {
        const initialLogs = await getRecentLogs(100);
        setLogs(initialLogs);

        unlisten = await listen<LogEntry>("log-entry", (event) => {
          setLogs((prev) => {
            const newLogs = [...prev, event.payload];
            return newLogs.length > 500 ? newLogs.slice(-500) : newLogs;
          });
        });
      } catch {}
    };

    init();
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll]);

  const uniqueModels = Array.from(new Set(logs.map((l) => l.model).filter(Boolean))) as string[];

  const filteredLogs = logs.filter((log) => {
    if (modelFilter !== "all" && log.model !== modelFilter) return false;
    if (statusFilter === "2xx" && (log.status < 200 || log.status >= 300)) return false;
    if (statusFilter === "4xx" && (log.status < 400 || log.status >= 500)) return false;
    if (statusFilter === "5xx" && log.status < 500) return false;
    return true;
  });

  return (
    <div className="flex flex-col h-full">
      <div className="shrink-0 flex items-center justify-between gap-4 px-6 py-3 border-b">
        <div className="flex items-center gap-3">
          <Select value={modelFilter} onValueChange={setModelFilter}>
            <SelectTrigger className="w-[150px] h-8 text-xs">
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
            <SelectTrigger className="w-[130px] h-8 text-xs">
              <SelectValue placeholder="All Status" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Status</SelectItem>
              <SelectItem value="2xx">2xx Success</SelectItem>
              <SelectItem value="4xx">4xx Client</SelectItem>
              <SelectItem value="5xx">5xx Server</SelectItem>
            </SelectContent>
          </Select>

          <Badge variant="secondary" className="text-xs font-mono">
            {filteredLogs.length}/{logs.length}
          </Badge>
        </div>

        <Button
          variant="outline"
          size="sm"
          className="h-8 text-xs text-destructive hover:text-destructive cursor-pointer"
          onClick={() => setLogs([])}
        >
          <Trash2 className="w-3.5 h-3.5 mr-1.5" />
          Clear
        </Button>
      </div>

      <div className="flex-1 overflow-hidden">
        <ScrollArea className="h-full">
          <Table>
            <TableHeader className="sticky top-0 bg-background z-10">
              <TableRow>
                <TableHead className="w-[130px]">Time</TableHead>
                <TableHead>Path</TableHead>
                <TableHead className="w-[100px]">Model</TableHead>
                <TableHead className="w-[60px]">Status</TableHead>
                <TableHead className="w-[80px] text-right">Duration</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredLogs.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={5} className="h-48">
                    <div className="flex flex-col items-center gap-2 text-center">
                      <ScrollText className="w-8 h-8 text-muted-foreground/30" />
                      <p className="text-sm text-muted-foreground">No logs recorded yet</p>
                    </div>
                  </TableCell>
                </TableRow>
              ) : (
                filteredLogs.map((log, index) => (
                  <TableRow key={`${log.id}-${index}`}>
                    <TableCell className="font-mono text-xs text-muted-foreground">
                      {new Date(log.timestamp).toLocaleTimeString()}
                    </TableCell>
                    <TableCell className="font-mono text-xs truncate max-w-[180px]" title={`${log.method} ${log.path}`}>
                      {log.path}
                    </TableCell>
                    <TableCell className="text-xs truncate max-w-[100px]" title={log.model || "-"}>
                      {log.model || <span className="text-muted-foreground">—</span>}
                    </TableCell>
                    <TableCell>
                      <Badge variant="outline" className={cn("font-mono text-[10px]", getStatusClasses(log.status))}>
                        {log.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-right font-mono text-xs">
                      {log.duration_ms}ms
                    </TableCell>
                  </TableRow>
                ))
              )}
              <div ref={scrollRef} />
            </TableBody>
          </Table>
        </ScrollArea>
      </div>
    </div>
  );
}
