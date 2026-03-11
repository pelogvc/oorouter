import { useEffect, useState } from "react";
import { getSettings, updateSetting } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { AlertCircle, CheckCircle, Settings as SettingsIcon } from "lucide-react";
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
  const [saveStatus, setSaveStatus] = useState<{ key: string; ok: boolean } | null>(null);

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
      } catch {
        // running outside Tauri
      }
    })();
  }, []);

  const save = async (key: keyof SettingsState, value: string) => {
    try {
      await updateSetting(key, value);
      setSaveStatus({ key, ok: true });
      setTimeout(() => setSaveStatus(null), 2000);
    } catch {
      setSaveStatus({ key, ok: false });
      setTimeout(() => setSaveStatus(null), 3000);
    }
  };

  const StatusIcon = ({ settingKey }: { settingKey: string }) => {
    if (!saveStatus || saveStatus.key !== settingKey) return null;
    return saveStatus.ok ? (
      <CheckCircle className="w-4 h-4 text-emerald-500 shrink-0" />
    ) : (
      <AlertCircle className="w-4 h-4 text-destructive shrink-0" />
    );
  };

  return (
    <div className="p-4 space-y-4">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-lg flex items-center gap-2">
            <SettingsIcon className="w-5 h-5" />
            Settings
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-5">
          <div className="space-y-1.5">
            <label className="text-sm font-medium">Port</label>
            <div className="flex items-center gap-2">
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
                className="w-32"
              />
              <StatusIcon settingKey="port" />
            </div>
            {portChanged && (
              <p className="text-xs text-amber-600 flex items-center gap-1">
                <AlertCircle className="w-3 h-3" />
                Server restart required for port change to take effect
              </p>
            )}
          </div>

          <div className="space-y-1.5">
            <label className="text-sm font-medium">Auth File Path</label>
            <div className="flex items-center gap-2">
              <Input
                type="text"
                value={settings.auth_path}
                onChange={(e) =>
                  setSettings((prev) => ({ ...prev, auth_path: e.target.value }))
                }
                onBlur={() => save("auth_path", settings.auth_path)}
                className="flex-1 font-mono text-xs"
                placeholder="~/.codex/auth.json"
              />
              <StatusIcon settingKey="auth_path" />
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div className="space-y-0.5">
              <label className="text-sm font-medium">Auto Start</label>
              <p className="text-xs text-muted-foreground">
                Start server automatically on app launch
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Switch
                checked={settings.auto_start === "true"}
                onCheckedChange={(checked) => {
                  const value = checked ? "true" : "false";
                  setSettings((prev) => ({ ...prev, auto_start: value }));
                  save("auto_start", value);
                }}
              />
              <StatusIcon settingKey="auto_start" />
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-sm font-medium">Log Level</label>
            <div className="flex items-center gap-2">
              <Select
                value={settings.log_level}
                onValueChange={(value) => {
                  setSettings((prev) => ({ ...prev, log_level: value }));
                  save("log_level", value);
                }}
              >
                <SelectTrigger className="w-32">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="debug">debug</SelectItem>
                  <SelectItem value="info">info</SelectItem>
                  <SelectItem value="warn">warn</SelectItem>
                  <SelectItem value="error">error</SelectItem>
                </SelectContent>
              </Select>
              <StatusIcon settingKey="log_level" />
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
