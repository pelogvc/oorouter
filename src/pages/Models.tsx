import { useEffect, useState } from "react";
import { getModels } from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Cpu, Eye, EyeOff, Loader2, AlertCircle, Boxes } from "lucide-react";

type Model = {
  id: string;
  name: string;
  visible: boolean;
  context_length: number;
  supports_vision: boolean;
};

const ALIASES: Record<string, string> = {
  codex: "gpt-5.3-codex",
  spark: "gpt-5.3-codex-spark",
  gpt5: "gpt-5.4",
};

export default function Models() {
  const [models, setModels] = useState<Model[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        setLoading(true);
        setError(null);
        const data = await getModels();
        setModels(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <Card className="shadow-sm border-destructive/30">
          <CardContent className="py-8 flex flex-col items-center gap-3 text-center">
            <AlertCircle className="w-8 h-8 text-destructive" />
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
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold flex items-center gap-2 text-foreground">
          <Cpu className="w-5 h-5" />
          Available Models
        </h2>
        <Badge variant="secondary" className="text-xs font-mono">
          {models.length} models
        </Badge>
      </div>

      {models.length === 0 ? (
        <Card className="shadow-sm">
          <CardContent className="py-16 flex flex-col items-center gap-3 text-center">
            <Boxes className="w-10 h-10 text-muted-foreground/30" />
            <p className="text-sm text-muted-foreground">No models available</p>
          </CardContent>
        </Card>
      ) : (
        <div className="grid grid-cols-1 gap-3">
          {models.map((model) => (
            <Card key={model.id} className="shadow-sm">
              <CardContent className="p-4 flex items-center justify-between">
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-9 h-9 rounded-lg bg-primary/10 flex items-center justify-center shrink-0">
                    <Cpu className="w-4 h-4 text-primary" />
                  </div>
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-sm text-foreground">{model.id}</span>
                      {ALIASES[model.id] && (
                        <span className="text-[11px] text-muted-foreground truncate">
                          → {ALIASES[model.id]}
                        </span>
                      )}
                    </div>
                    <div className="text-xs text-muted-foreground mt-0.5">
                      {Math.round(model.context_length / 1000)}K context
                    </div>
                  </div>
                </div>

                <div className="flex items-center gap-2 shrink-0">
                  {model.supports_vision && (
                    <Badge variant="outline" className="text-[10px] gap-1 border-blue-500/20 bg-blue-500/5 text-blue-600 dark:text-blue-400">
                      <Eye className="w-3 h-3" />
                      Vision
                    </Badge>
                  )}
                  <Badge
                    variant="outline"
                    className={model.visible
                      ? "text-[10px] gap-1 border-emerald-500/20 bg-emerald-500/5 text-emerald-600 dark:text-emerald-400"
                      : "text-[10px] gap-1 text-muted-foreground/60"
                    }
                  >
                    {model.visible ? <Eye className="w-3 h-3" /> : <EyeOff className="w-3 h-3" />}
                    {model.visible ? "Visible" : "Hidden"}
                  </Badge>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}
