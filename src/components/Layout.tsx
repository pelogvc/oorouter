import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { useTheme, type Theme } from "@/lib/use-theme";
import { getServerStatus, listen } from "@/lib/tauri";
import { UpdateBanner } from "@/components/UpdateBanner";
import { Badge } from "@/components/ui/badge";
import {
  Home,
  ScrollText,
  Cpu,
  Settings,
  BarChart3,
  Sun,
  Moon,
  Monitor,
  Circle,
} from "lucide-react";

export type Tab = "home" | "logs" | "models" | "settings" | "usage";

interface LayoutProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  children: React.ReactNode;
}

const TABS: { id: Tab; label: string; icon: React.ElementType }[] = [
  { id: "home", label: "Dashboard", icon: Home },
  { id: "logs", label: "Logs", icon: ScrollText },
  { id: "models", label: "Models", icon: Cpu },
  { id: "usage", label: "Usage", icon: BarChart3 },
  { id: "settings", label: "Settings", icon: Settings },
];

const THEME_OPTIONS: { value: Theme; icon: React.ElementType }[] = [
  { value: "light", icon: Sun },
  { value: "dark", icon: Moon },
  { value: "system", icon: Monitor },
];

export function Layout({ activeTab, onTabChange, children }: LayoutProps) {
  const { theme, setTheme } = useTheme();
  const [status, setStatus] = useState<{ running: boolean; error?: string }>({
    running: false,
  });

  useEffect(() => {
    const checkStatus = async () => {
      try {
        const res = await getServerStatus();
        setStatus({ running: res.running, error: res.error });
      } catch {
        setStatus({ running: false });
      }
    };

    checkStatus();
    const interval = setInterval(checkStatus, 2000);
    const unlisten = listen("server-status-changed", checkStatus);
    return () => {
      clearInterval(interval);
      unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);

  const activeLabel = TABS.find((t) => t.id === activeTab)?.label;
  const statusLabel = status.running ? "Online" : status.error ? "Error" : "Offline";

  return (
    <div className="flex h-screen w-full overflow-hidden bg-background text-foreground">
      <nav className="flex w-40 shrink-0 flex-col border-r bg-card">
        <div className="flex h-14 items-center gap-2.5 border-b px-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-foreground text-background">
            <svg viewBox="37 50 175 150" className="h-5 w-5" fill="currentColor" aria-hidden="true">
              <path d="m175.1 87.7c-14.9 0-24.92 5.38-44.96 25.99l-12.16 13.47c-15.92 17.95-26.71 26.02-42.31 26.02-14.9 0-26.99-12.46-26.99-27.45 0-15.35 12.09-27.9 27.19-27.9 11.89 0 19.54 5.89 28.31 17.03 2.69 3.5 9 0.2 7.94-4.66-8.36-12.46-18.38-22.69-35.2-22.69-21.22 0-39.17 17.57-39.17 37.92 0 20.75 15.92 38.32 37.25 38.32h1.06c16.27 0 27.16-8.57 48-30.84l4.84-5.28c17.37-19.41 29.06-29.99 45.33-29.99 15.1 0 27.39 12.65 27.39 27.9 0 14.7-11.9 27.64-27.6 27.64-11.32 0-19.27-5.47-27.42-16.81-3.48-3.56-10.19-0.1-8.74 5.28 9.54 13.34 19.37 22.2 36.16 22.2h1.06c21.03 0 36.53-18.93 36.53-38.14-0.77-19.7-16.69-38.01-36.51-38.01z" />
            </svg>
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-semibold leading-5">oorouter</div>
            <div className="text-[11px] font-medium uppercase text-muted-foreground">
              local proxy
            </div>
          </div>
        </div>

        <div className="flex-1 space-y-1 overflow-y-auto p-2">
          {TABS.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => onTabChange(tab.id)}
                className={cn(
                  "flex h-9 w-full items-center gap-2 rounded-md px-2.5 text-[13px] font-medium transition-colors duration-150",
                  isActive
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                )}
              >
                <Icon className="w-4 h-4 shrink-0" />
                {tab.label}
              </button>
            );
          })}
        </div>

        <div className="space-y-3 border-t p-2">
          <div className="grid grid-cols-3 gap-1 rounded-md bg-muted p-1">
            {THEME_OPTIONS.map(({ value, icon: Icon }) => (
              <button
                key={value}
                onClick={() => setTheme(value)}
                className={cn(
                  "flex h-7 items-center justify-center rounded-sm transition-colors duration-150",
                  theme === value
                    ? "bg-background text-foreground shadow-sm"
                    : "text-muted-foreground hover:text-foreground"
                )}
              >
                <Icon className="w-3.5 h-3.5" />
              </button>
            ))}
          </div>

          <div className="rounded-md border bg-background px-2.5 py-2">
            <div className="flex items-center justify-between gap-2">
              <span className="text-[11px] font-medium uppercase text-muted-foreground">
                Server
              </span>
              <Circle
                className={cn(
                  "h-2.5 w-2.5 shrink-0 fill-current",
                  status.running
                    ? "text-emerald-500"
                    : status.error
                      ? "text-destructive"
                      : "text-muted-foreground"
                )}
              />
            </div>
            <div className="mt-1 truncate text-sm font-semibold">{statusLabel}</div>
          </div>
        </div>
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b bg-background px-6">
          <h2 className="text-sm font-semibold">{activeLabel}</h2>
          <Badge
            variant="outline"
            className={cn(
              "h-6 rounded-md px-2 text-[10px] font-semibold uppercase",
              status.running
                ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300"
                : status.error
                  ? "border-destructive/30 bg-destructive/10 text-destructive"
                  : "text-muted-foreground"
            )}
          >
            <Circle
              className={cn(
                "mr-1.5 h-1.5 w-1.5 fill-current",
                status.running
                  ? "text-emerald-500"
                  : status.error
                    ? "text-destructive"
                    : "text-muted-foreground"
              )}
            />
            {statusLabel}
          </Badge>
        </header>

        <UpdateBanner />
        <main className="flex-1 overflow-auto">{children}</main>
      </div>
    </div>
  );
}
