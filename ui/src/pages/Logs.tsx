import { useEffect, useState, useRef } from "react";
import { getRecentLogs, listen, type UnlistenFn } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
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
import { Activity, Filter, Trash2 } from "lucide-react";

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

export default function Logs() {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [modelFilter, setModelFilter] = useState<string>("all");
  const [statusFilter, setStatusFilter] = useState<string>("all");
  const [autoScroll, setAutoScroll] = useState(true);
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
            if (newLogs.length > 500) {
              return newLogs.slice(newLogs.length - 500);
            }
            return newLogs;
          });
        });
      } catch (error) {
        console.error("Failed to initialize logs:", error);
      }
    };

    init();

    return () => {
      if (unlisten) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    if (autoScroll && scrollRef.current) {
      scrollRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll]);

  const getStatusBadge = (status: number) => {
    if (status >= 200 && status < 300) {
      return <Badge className="bg-emerald-500 hover:bg-emerald-600 text-white border-transparent">{status}</Badge>;
    }
    if (status >= 400 && status < 500) {
      return <Badge className="bg-amber-500 hover:bg-amber-600 text-white border-transparent">{status}</Badge>;
    }
    if (status >= 500) {
      return <Badge variant="destructive">{status}</Badge>;
    }
    return <Badge variant="secondary">{status}</Badge>;
  };

  const uniqueModels = Array.from(new Set(logs.map((l) => l.model).filter(Boolean))) as string[];

  const filteredLogs = logs.filter((log) => {
    if (modelFilter !== "all" && log.model !== modelFilter) return false;
    
    if (statusFilter !== "all") {
      if (statusFilter === "2xx" && (log.status < 200 || log.status >= 300)) return false;
      if (statusFilter === "4xx" && (log.status < 400 || log.status >= 500)) return false;
      if (statusFilter === "5xx" && log.status < 500) return false;
    }
    
    return true;
  });

  return (
    <div className="p-4 space-y-4 h-[calc(100vh-2rem)] flex flex-col">
      <Card className="flex-1 flex flex-col overflow-hidden">
        <CardHeader className="pb-3 shrink-0">
          <div className="flex items-center justify-between">
            <CardTitle className="text-lg flex items-center gap-2">
              <Activity className="w-5 h-5" />
              Real-time Logs
              <Badge variant="secondary" className="ml-2">
                {filteredLogs.length} / {logs.length}
              </Badge>
            </CardTitle>
            <div className="flex items-center gap-2">
              <div className="flex items-center gap-2 mr-4">
                <Filter className="w-4 h-4 text-muted-foreground" />
                <Select value={modelFilter} onValueChange={setModelFilter}>
                  <SelectTrigger className="w-[150px] h-8 text-xs">
                    <SelectValue placeholder="All Models" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Models</SelectItem>
                    {uniqueModels.map((model) => (
                      <SelectItem key={model} value={model}>
                        {model}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>

                <Select value={statusFilter} onValueChange={setStatusFilter}>
                  <SelectTrigger className="w-[110px] h-8 text-xs">
                    <SelectValue placeholder="All Status" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Status</SelectItem>
                    <SelectItem value="2xx">2xx Success</SelectItem>
                    <SelectItem value="4xx">4xx Client Error</SelectItem>
                    <SelectItem value="5xx">5xx Server Error</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <Button
                variant={autoScroll ? "default" : "outline"}
                size="sm"
                className="h-8 text-xs"
                onClick={() => setAutoScroll(!autoScroll)}
              >
                Auto-scroll
              </Button>
              <Button
                variant="outline"
                size="sm"
                className="h-8 text-xs text-destructive hover:text-destructive"
                onClick={() => setLogs([])}
              >
                <Trash2 className="w-4 h-4 mr-1" />
                Clear
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent className="flex-1 overflow-hidden p-0">
          <ScrollArea className="h-full border-t">
            <Table>
              <TableHeader className="sticky top-0 bg-background z-10">
                <TableRow>
                  <TableHead className="w-[180px]">Timestamp</TableHead>
                  <TableHead className="w-[80px]">Method</TableHead>
                  <TableHead>Path</TableHead>
                  <TableHead className="w-[150px]">Model</TableHead>
                  <TableHead className="w-[80px]">Status</TableHead>
                  <TableHead className="w-[100px] text-right">Duration</TableHead>
                  <TableHead className="w-[120px] text-right">Tokens (I/O)</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {filteredLogs.length === 0 ? (
                  <TableRow>
                    <TableCell colSpan={7} className="h-24 text-center text-muted-foreground">
                      No logs found
                    </TableCell>
                  </TableRow>
                ) : (
                  filteredLogs.map((log, index) => (
                    <TableRow key={`${log.id}-${index}`}>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {new Date(log.timestamp).toLocaleString()}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline" className="font-mono text-[10px]">
                          {log.method}
                        </Badge>
                      </TableCell>
                      <TableCell className="font-mono text-xs truncate max-w-[200px]" title={log.path}>
                        {log.path}
                      </TableCell>
                      <TableCell className="text-xs truncate max-w-[150px]" title={log.model || "-"}>
                        {log.model || "-"}
                      </TableCell>
                      <TableCell>{getStatusBadge(log.status)}</TableCell>
                      <TableCell className="text-right font-mono text-xs">
                        {log.duration_ms}ms
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs text-muted-foreground">
                        {log.input_tokens !== undefined || log.output_tokens !== undefined ? (
                          <>
                            <span className="text-blue-500">{log.input_tokens || 0}</span>
                            {" / "}
                            <span className="text-emerald-500">{log.output_tokens || 0}</span>
                          </>
                        ) : (
                          "-"
                        )}
                      </TableCell>
                    </TableRow>
                  ))
                )}
                <div ref={scrollRef} />
              </TableBody>
            </Table>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}
