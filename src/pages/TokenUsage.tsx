import { useEffect, useState, type ReactNode } from "react";
import { getTokenUsage, type TokenUsageRow } from "@/lib/tauri";
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

const PERIOD_OPTIONS = [7, 14, 30] as const;

export default function TokenUsage() {
  const [rows, setRows] = useState<TokenUsageRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [days, setDays] = useState(7);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    (async () => {
      if (active) {
        setLoading(true);
        setError(null);
      }
      try {
        const data = await getTokenUsage(days);
        if (active) {
          setRows(data);
        }
      } catch {
        if (active) {
          setRows([]);
          setError("Usage data is temporarily unavailable.");
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    })();

    return () => {
      active = false;
    };
  }, [days]);

  const totalInput = rows.reduce((s, r) => s + r.input_tokens, 0);
  const totalOutput = rows.reduce((s, r) => s + r.output_tokens, 0);
  const totalRequests = rows.reduce((s, r) => s + r.request_count, 0);
  const totalTokens = totalInput + totalOutput;

  const stats = [
    { label: "Input", value: totalInput, icon: ArrowDownToLine, primary: false },
    { label: "Output", value: totalOutput, icon: ArrowUpFromLine, primary: true },
    { label: "Requests", value: totalRequests, icon: Hash, primary: false },
  ];

  let content: ReactNode;
  if (loading) {
    content = (
      <>
        <div className="grid grid-cols-3 gap-3">
          {[1, 2, 3].map((i) => (
            <Card key={i}>
              <CardContent className="space-y-3 p-4">
                <div className="h-3 w-16 animate-pulse rounded bg-muted" />
                <div className="h-7 w-24 animate-pulse rounded bg-muted" />
              </CardContent>
            </Card>
          ))}
        </div>
        <Card className="min-h-0 flex-1">
          <CardContent className="space-y-2 p-4">
            {[1, 2, 3].map((i) => (
              <div key={i} className="h-9 animate-pulse rounded bg-muted" />
            ))}
          </CardContent>
        </Card>
      </>
    );
  } else if (error) {
    content = (
      <Card className="min-h-0 flex-1 border-destructive-text/30">
        <CardContent className="flex h-full flex-col items-center justify-center gap-3 py-16 text-center" role="alert">
          <BarChart3 className="h-10 w-10 text-destructive-text/70" aria-hidden="true" />
          <div className="space-y-1">
            <p className="text-sm font-medium text-destructive-text">Failed to load usage data</p>
            <p className="text-xs text-muted-foreground">{error}</p>
          </div>
        </CardContent>
      </Card>
    );
  } else if (rows.length === 0) {
    content = (
      <Card className="min-h-0 flex-1">
        <CardContent className="flex h-full flex-col items-center justify-center gap-3 py-16 text-center">
          <BarChart3 aria-hidden="true" className="h-10 w-10 text-muted-foreground/30" />
          <div className="space-y-1">
            <p className="text-sm font-medium text-muted-foreground">No usage data</p>
            <p className="text-xs text-muted-foreground/70">No token usage recorded in the last {days} days</p>
          </div>
        </CardContent>
      </Card>
    );
  } else {
    content = (
      <>
        <div className="grid grid-cols-3 gap-3">
          {stats.map((stat) => {
            const Icon = stat.icon;
            return (
              <Card
                key={stat.label}
                className={cn(stat.primary && "border-primary/25 bg-primary/[0.06]")}
              >
                <CardContent className="flex h-24 flex-col justify-between p-4">
                  <div className="flex items-center gap-2 text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                    <Icon
                      aria-hidden="true"
                      className={cn(
                        "h-3.5 w-3.5",
                        stat.primary ? "text-primary" : "text-muted-foreground"
                      )}
                    />
                    {stat.label}
                  </div>
                  <div className="font-mono text-2xl font-semibold tracking-tight text-foreground">
                    {stat.value.toLocaleString()}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>

        <Card className="min-h-0 flex-1 overflow-hidden">
          <CardContent className="h-full p-0">
            <Table>
              <TableHeader className="sticky top-0 z-10 bg-card">
                <TableRow>
                  <TableHead className="h-9 px-3 text-[11px] uppercase tracking-wider">Date</TableHead>
                  <TableHead className="h-9 px-3 text-[11px] uppercase tracking-wider">Model</TableHead>
                  <TableHead className="h-9 px-3 text-right text-[11px] uppercase tracking-wider">Input</TableHead>
                  <TableHead className="h-9 px-3 text-right text-[11px] uppercase tracking-wider">Output</TableHead>
                  <TableHead className="h-9 px-3 text-right text-[11px] uppercase tracking-wider">Total</TableHead>
                  <TableHead className="h-9 px-3 text-right text-[11px] uppercase tracking-wider">Req</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((row) => (
                  <TableRow key={`${row.date}:${row.model}`}>
                    <TableCell className="px-3 py-2 font-mono text-xs text-muted-foreground">
                      {row.date}
                    </TableCell>
                    <TableCell className="px-3 py-2">
                      <Badge variant="outline" className="h-5 rounded px-1.5 font-mono text-[10px]">
                        {row.model}
                      </Badge>
                    </TableCell>
                    <TableCell className="px-3 py-2 text-right font-mono text-xs">
                      {row.input_tokens.toLocaleString()}
                    </TableCell>
                    <TableCell className="px-3 py-2 text-right font-mono text-xs">
                      {row.output_tokens.toLocaleString()}
                    </TableCell>
                    <TableCell className="px-3 py-2 text-right font-mono text-xs">
                      {row.total_tokens.toLocaleString()}
                    </TableCell>
                    <TableCell className="px-3 py-2 text-right font-mono text-xs">
                      {row.request_count}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="flex h-10 shrink-0 items-center justify-between rounded-lg border bg-card px-4">
        <div className="flex items-center gap-2 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          <BarChart3 aria-hidden="true" className="h-3.5 w-3.5" />
          Token Usage
          {rows.length > 0 && !loading && (
            <Badge variant="secondary" className="ml-1 h-5 rounded px-1.5 font-mono text-[10px]">
              {totalTokens.toLocaleString()} total
            </Badge>
          )}
        </div>
        <div
          className="flex items-center gap-0.5 rounded-md bg-muted p-0.5"
          role="group"
          aria-label="Token usage period"
        >
          {PERIOD_OPTIONS.map((d) => (
            <button
              key={d}
              onClick={() => setDays(d)}
              aria-pressed={days === d}
              aria-label={`Last ${d} days`}
              className={cn(
                "h-7 rounded px-2 text-xs font-medium transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
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

      {content}
    </div>
  );
}
