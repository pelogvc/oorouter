import { useEffect, useState } from "react";
import { getTokenUsage } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Coins } from "lucide-react";

interface TokenUsageRow {
  date: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  request_count: number;
}
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
      } catch {
        // running outside Tauri
      } finally {
        setLoading(false);
      }
    })();
  }, [days]);

  const totalInput = rows.reduce((s, r) => s + r.input_tokens, 0);
  const totalOutput = rows.reduce((s, r) => s + r.output_tokens, 0);
  const totalRequests = rows.reduce((s, r) => s + r.request_count, 0);

  return (
    <div className="p-4 space-y-4">
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-lg flex items-center gap-2">
              <Coins className="w-5 h-5" />
              Token Usage
            </CardTitle>
            <div className="flex gap-1">
              {[7, 14, 30].map((d) => (
                <button
                  key={d}
                  onClick={() => setDays(d)}
                  className={`px-2 py-0.5 text-xs rounded border transition-colors ${days === d ? "bg-primary text-primary-foreground border-primary" : "border-border hover:bg-muted"}`}
                >
                  {d}d
                </button>
              ))}
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {loading ? (
            <p className="text-sm text-muted-foreground">Loading...</p>
          ) : rows.length === 0 ? (
            <p className="text-sm text-muted-foreground">No usage data for this period.</p>
          ) : (
            <div className="space-y-4">
              <div className="grid grid-cols-3 gap-3 text-center">
                <div className="rounded-lg border p-2">
                  <p className="text-xs text-muted-foreground">Input</p>
                  <p className="text-sm font-semibold">{totalInput.toLocaleString()}</p>
                </div>
                <div className="rounded-lg border p-2">
                  <p className="text-xs text-muted-foreground">Output</p>
                  <p className="text-sm font-semibold">{totalOutput.toLocaleString()}</p>
                </div>
                <div className="rounded-lg border p-2">
                  <p className="text-xs text-muted-foreground">Requests</p>
                  <p className="text-sm font-semibold">{totalRequests.toLocaleString()}</p>
                </div>
              </div>
              <div className="overflow-auto max-h-64">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b text-muted-foreground">
                      <th className="text-left py-1 pr-2">Date</th>
                      <th className="text-left py-1 pr-2">Model</th>
                      <th className="text-right py-1 pr-2">Input</th>
                      <th className="text-right py-1 pr-2">Output</th>
                      <th className="text-right py-1">Reqs</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((row, i) => (
                      <tr key={i} className="border-b last:border-0 hover:bg-muted/50">
                        <td className="py-1 pr-2 font-mono">{row.date}</td>
                        <td className="py-1 pr-2">
                          <Badge variant="outline" className="text-xs font-mono">{row.model}</Badge>
                        </td>
                        <td className="py-1 pr-2 text-right">{row.input_tokens.toLocaleString()}</td>
                        <td className="py-1 pr-2 text-right">{row.output_tokens.toLocaleString()}</td>
                        <td className="py-1 text-right">{row.request_count}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
