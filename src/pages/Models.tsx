import { useEffect, useState } from "react";
import { getModels, type Model } from "@/lib/tauri";
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
import { Cpu, Eye, EyeOff, Loader2, AlertCircle, Boxes } from "lucide-react";

const ALIASES: Record<string, string> = {
  codex: "gpt-5.3-codex",
  spark: "gpt-5.3-codex-spark",
  gpt5: "gpt-5.5",
};

export default function Models() {
  const [models, setModels] = useState<Model[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    (async () => {
      try {
        if (active) {
          setLoading(true);
          setError(null);
        }
        const data = await getModels();
        if (active) {
          setModels(data);
        }
      } catch (err) {
        if (active) {
          setError(err instanceof Error ? err.message : String(err));
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
  }, []);

  if (loading) {
    return (
      <div className="flex h-full flex-col gap-3 p-4">
        <div className="flex h-10 items-center justify-between rounded-lg border bg-card px-4">
          <div className="h-3 w-28 animate-pulse rounded bg-muted" />
          <Loader2 className="h-4 w-4 animate-spin text-muted-foreground" />
        </div>
        <Card className="min-h-0 flex-1">
          <CardContent className="space-y-2 p-4">
            {[1, 2, 3, 4, 5].map((i) => (
              <div key={i} className="h-10 animate-pulse rounded bg-muted" />
            ))}
          </CardContent>
        </Card>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-4">
        <Card className="border-destructive/30">
          <CardContent className="flex flex-col items-center gap-3 py-10 text-center">
            <AlertCircle className="h-8 w-8 text-destructive" />
            <div className="space-y-1">
              <p className="text-sm font-medium text-destructive">Failed to load models</p>
              <p className="text-xs text-muted-foreground">{error}</p>
            </div>
          </CardContent>
        </Card>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-3 p-4">
      <div className="flex h-10 shrink-0 items-center justify-between rounded-lg border bg-card px-4">
        <div className="flex items-center gap-2 text-xs font-semibold uppercase text-muted-foreground">
          <Cpu className="h-3.5 w-3.5" />
          Model Registry
        </div>
        <Badge variant="secondary" className="h-5 rounded px-1.5 font-mono text-[10px]">
          {models.length} models
        </Badge>
      </div>

      {models.length === 0 ? (
        <Card className="min-h-0 flex-1">
          <CardContent className="flex h-full flex-col items-center justify-center gap-3 py-16 text-center">
            <Boxes className="h-10 w-10 text-muted-foreground/30" />
            <p className="text-sm text-muted-foreground">No models available</p>
          </CardContent>
        </Card>
      ) : (
        <Card className="min-h-0 flex-1 overflow-hidden">
          <CardContent className="h-full p-0">
            <Table>
              <TableHeader className="sticky top-0 z-10 bg-card">
                <TableRow>
                  <TableHead className="h-9 px-3 text-[11px] uppercase">Model</TableHead>
                  <TableHead className="h-9 w-[110px] px-3 text-[11px] uppercase">Alias</TableHead>
                  <TableHead className="h-9 w-[90px] px-3 text-right text-[11px] uppercase">Context</TableHead>
                  <TableHead className="h-9 w-[86px] px-3 text-[11px] uppercase">Vision</TableHead>
                  <TableHead className="h-9 w-[88px] px-3 text-[11px] uppercase">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {models.map((model) => (
                  <TableRow key={model.id}>
                    <TableCell className="px-3 py-2">
                      <div className="flex min-w-0 items-center gap-2">
                        <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary">
                          <Cpu className="h-3.5 w-3.5" />
                        </div>
                        <span className="truncate font-mono text-xs font-semibold text-foreground">
                          {model.id}
                        </span>
                      </div>
                    </TableCell>
                    <TableCell className="px-3 py-2">
                      <span className="block truncate font-mono text-[11px] text-muted-foreground">
                        {ALIASES[model.id] || "-"}
                      </span>
                    </TableCell>
                    <TableCell className="px-3 py-2 text-right font-mono text-xs">
                      {Math.round(model.context_length / 1000)}K
                    </TableCell>
                    <TableCell className="px-3 py-2">
                      <Badge
                        variant="outline"
                        className={
                          model.supports_vision
                            ? "h-5 gap-1 rounded px-1.5 text-[10px] border-sky-500/25 bg-sky-500/10 text-sky-700 dark:text-sky-300"
                            : "h-5 gap-1 rounded px-1.5 text-[10px] text-muted-foreground"
                        }
                      >
                        {model.supports_vision ? (
                          <Eye className="h-3 w-3" />
                        ) : (
                          <EyeOff className="h-3 w-3" />
                        )}
                        {model.supports_vision ? "Yes" : "No"}
                      </Badge>
                    </TableCell>
                    <TableCell className="px-3 py-2">
                      <Badge
                        variant="outline"
                        className={
                          model.visible
                            ? "h-5 gap-1 rounded px-1.5 text-[10px] border-emerald-500/25 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                            : "h-5 gap-1 rounded px-1.5 text-[10px] text-muted-foreground"
                        }
                      >
                        {model.visible ? "Visible" : "Hidden"}
                      </Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
