import { useEffect, useState } from "react";
import { getSettings, updateSetting } from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { AlertCircle } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

interface SettingsState {
  port: string;
  auth_path: string;
  auto_start: string;
  log_level: string;
}

const DEFAULT_SETTINGS: SettingsState = {
  port: "11434",
  auth_path: "~/.codex/auth.json",
  auto_start: "true",
  log_level: "info",
};

export default function Settings() {
  const [settings, setSettings] = useState<SettingsState>(DEFAULT_SETTINGS);
  const [portChanged, setPortChanged] = useState(false);
  useEffect(() => {
    (async () => {
      try {
        const rows = await getSettings();
        const map: Partial<SettingsState> = {};
        for (const row of rows) {
          if (row.key in DEFAULT_SETTINGS) {
            (map as Record<string, string>)[row.key] = row.value;
          }
        }
        setSettings((prev) => ({ ...prev, ...map }));
      } catch {}
    })();
  }, []);

  const save = async (key: keyof SettingsState, value: string) => {
    try {
      await updateSetting(key, value);
    } catch {}
  };

  return (
    <div className="p-6 space-y-6 max-w-2xl">
      <Card className="shadow-sm">
        <CardContent className="p-0 divide-y divide-border">
          <div className="p-5 space-y-2">
            <label className="text-sm font-medium text-foreground">Port</label>
            <p className="text-xs text-muted-foreground">The port the proxy server listens on</p>
            <Input
              type="number"
              min={1}
              max={65535}
              value={settings.port}
              onChange={(e) => {
                setSettings((prev) => ({ ...prev, port: e.target.value }));
                setPortChanged(true);
              }}
              onBlur={() => save("port", settings.port)}
              className="w-28 font-mono"
            />
            {portChanged && (
              <p className="text-xs text-amber-600 dark:text-amber-400 flex items-center gap-1.5 mt-1">
                <AlertCircle className="w-3 h-3 shrink-0" />
                Restart required for port change
              </p>
            )}
          </div>

          <div className="p-5 space-y-2">
            <label className="text-sm font-medium text-foreground">Auth File</label>
            <p className="text-xs text-muted-foreground">Path to the Codex authentication file</p>
            <Input
              type="text"
              value={settings.auth_path}
              onChange={(e) =>
                setSettings((prev) => ({ ...prev, auth_path: e.target.value }))
              }
              onBlur={() => save("auth_path", settings.auth_path)}
              className="font-mono text-xs"
              placeholder="~/.codex/auth.json"
            />
          </div>

          <div className="p-5 flex items-center justify-between">
            <div className="space-y-1">
              <label className="text-sm font-medium text-foreground">Auto Start</label>
              <p className="text-xs text-muted-foreground">
                Launch oorouter automatically when you log in to your Mac
              </p>
            </div>
            <Switch
              checked={settings.auto_start === "true"}
              onCheckedChange={(checked) => {
                const value = checked ? "true" : "false";
                setSettings((prev) => ({ ...prev, auto_start: value }));
                save("auto_start", value);
              }}
            />
          </div>

          <div className="p-5 space-y-2">
            <label className="text-sm font-medium text-foreground">Log Level</label>
            <p className="text-xs text-muted-foreground">Controls the verbosity of server logs</p>
            <Select
              value={settings.log_level}
              onValueChange={(value) => {
                setSettings((prev) => ({ ...prev, log_level: value }));
                save("log_level", value);
              }}
            >
              <SelectTrigger className="w-28">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="debug">debug</SelectItem>
                <SelectItem value="info">info</SelectItem>
                <SelectItem value="warn">warn</SelectItem>
                <SelectItem value="error">error</SelectItem>
              </SelectContent>
            </Select>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
