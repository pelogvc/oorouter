import { useEffect, useState } from "react";
import { getTokenUsage } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ArrowDownToLine, ArrowUpFromLine, Hash, BarChart3 } from "lucide-react";

interface TokenUsageRow {
  date: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  request_count: number;
}

const PERIOD_OPTIONS = [7, 14, 30] as const;

export default function TokenUsage() {
  const [rows, setRows] = useState<TokenUsageRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [days, setDays] = useState(7);

  useEffect(() => {
    (async () => {
      setLoading(true);
      try {
        const data = await getTokenUsage(days);
        setRows(data);
      } catch {}
      setLoading(false);
    })();
  }, [days]);

  const totalInput = rows.reduce((s, r) => s + r.input_tokens, 0);
  const totalOutput = rows.reduce((s, r) => s + r.output_tokens, 0);
  const totalRequests = rows.reduce((s, r) => s + r.request_count, 0);

  const stats = [
    { label: "Input Tokens", value: totalInput, icon: ArrowDownToLine, color: "text-blue-500" },
    { label: "Output Tokens", value: totalOutput, icon: ArrowUpFromLine, color: "text-emerald-500" },
    { label: "Requests", value: totalRequests, icon: Hash, color: "text-amber-500 dark:text-amber-400" },
  ];

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold flex items-center gap-2 text-foreground">
          <BarChart3 className="w-5 h-5" />
          Token Usage
        </h2>
        <div className="flex items-center gap-0.5 bg-muted rounded-lg p-0.5">
          {PERIOD_OPTIONS.map((d) => (
            <button
              key={d}
              onClick={() => setDays(d)}
              className={cn(
                "px-3 py-1.5 text-xs font-medium rounded-md transition-colors duration-150 cursor-pointer",
                days === d
                  ? "bg-background text-foreground shadow-sm"
                  : "text-muted-foreground hover:text-foreground"
              )}
            >
              {d}d
            </button>
          ))}
        </div>
      </div>

      {loading ? (
        <>
          <div className="grid grid-cols-3 gap-4">
            {[1, 2, 3].map((i) => (
              <Card key={i} className="shadow-sm">
                <CardContent className="p-4 space-y-2">
                  <div className="h-4 w-20 bg-muted rounded animate-pulse" />
                  <div className="h-8 w-28 bg-muted rounded animate-pulse" />
                </CardContent>
              </Card>
            ))}
          </div>
          <Card className="shadow-sm">
            <CardContent className="p-4 space-y-3">
              {[1, 2, 3].map((i) => (
                <div key={i} className="h-8 bg-muted rounded animate-pulse" />
              ))}
            </CardContent>
          </Card>
        </>
      ) : rows.length === 0 ? (
        <Card className="shadow-sm">
          <CardContent className="py-16 flex flex-col items-center gap-3 text-center">
            <BarChart3 className="w-10 h-10 text-muted-foreground/30" />
            <div className="space-y-1">
              <p className="text-sm font-medium text-muted-foreground">No usage data</p>
              <p className="text-xs text-muted-foreground/70">No token usage recorded in the last {days} days</p>
            </div>
          </CardContent>
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-3 gap-4">
            {stats.map((stat) => {
              const Icon = stat.icon;
              return (
                <Card key={stat.label} className="shadow-sm">
                  <CardContent className="p-4 flex flex-col gap-1.5">
                    <div className="flex items-center gap-2 text-sm text-muted-foreground">
                      <Icon className={cn("w-4 h-4", stat.color)} />
                      {stat.label}
                    </div>
                    <div className="text-2xl font-semibold font-mono text-foreground">
                      {stat.value.toLocaleString()}
                    </div>
                  </CardContent>
                </Card>
              );
            })}
          </div>

          <Card className="shadow-sm">
            <CardContent className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Date</TableHead>
                    <TableHead>Model</TableHead>
                    <TableHead className="text-right">Input</TableHead>
                    <TableHead className="text-right">Output</TableHead>
                    <TableHead className="text-right">Requests</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((row, i) => (
                    <TableRow key={i}>
                      <TableCell className="font-mono text-xs text-muted-foreground">
                        {row.date}
                      </TableCell>
                      <TableCell>
                        <Badge variant="outline" className="font-mono text-xs">
                          {row.model}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs">
                        {row.input_tokens.toLocaleString()}
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs">
                        {row.output_tokens.toLocaleString()}
                      </TableCell>
                      <TableCell className="text-right font-mono text-xs">
                        {row.request_count}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </CardContent>
          </Card>
        </>
      )}
    </div>
  );
}
