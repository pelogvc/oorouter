import { useEffect, useState } from "react";
import { getModels } from "@/lib/tauri";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Check, Minus, Loader2 } from "lucide-react";

type Model = {
  slug: string;
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
    async function fetchModels() {
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
    }

    fetchModels();
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
      <div className="p-4 text-sm text-destructive">
        Error loading models: {error}
      </div>
    );
  }

  return (
    <div className="p-4">
      <div className="rounded-md border">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Name</TableHead>
              <TableHead className="text-right">Context</TableHead>
              <TableHead className="text-center">Vision</TableHead>
              <TableHead className="text-center">Visible</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {models.length === 0 ? (
              <TableRow>
                <TableCell colSpan={4} className="text-center text-muted-foreground">
                  No models found.
                </TableCell>
              </TableRow>
            ) : (
              models.map((model) => (
                <TableRow key={model.slug}>
                  <TableCell>
                    <div className="flex items-center gap-1.5">
                      <span className="font-bold">{model.slug}</span>
                      {ALIASES[model.slug] && (
                        <span className="text-[11px] text-muted-foreground">
                          → {ALIASES[model.slug]}
                        </span>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-right text-muted-foreground">
                    {Math.round(model.context_length / 1000)}K
                  </TableCell>
                  <TableCell className="text-center">
                    {model.supports_vision ? (
                      <Check className="mx-auto h-4 w-4 text-green-500" />
                    ) : (
                      <Minus className="mx-auto h-4 w-4 text-muted-foreground/50" />
                    )}
                  </TableCell>
                  <TableCell className="text-center">
                    {model.visible ? (
                      <Check className="mx-auto h-4 w-4 text-green-500" />
                    ) : (
                      <Minus className="mx-auto h-4 w-4 text-muted-foreground/50" />
                    )}
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
