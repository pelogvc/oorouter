import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { getServerStatus } from "@/lib/tauri";
import { Badge } from "@/components/ui/badge";
import { Home, ScrollText, Cpu, Settings, BarChart3 } from "lucide-react";

export type Tab = "home" | "logs" | "models" | "settings" | "usage";

interface LayoutProps {
  activeTab: Tab;
  onTabChange: (tab: Tab) => void;
  children: React.ReactNode;
}

export function Layout({ activeTab, onTabChange, children }: LayoutProps) {
  const [status, setStatus] = useState<{ running: boolean; error?: string }>({
    running: false,
  });

  useEffect(() => {
    const checkStatus = async () => {
      try {
        const res = await getServerStatus();
        setStatus({ running: res.running, error: res.error });
      } catch (err) {
        console.error("Failed to get server status:", err);
        setStatus({ running: false, error: String(err) });
      }
    };

    checkStatus();
    const interval = setInterval(checkStatus, 2000);
    return () => clearInterval(interval);
  }, []);

  const tabs: { id: Tab; label: string; icon: React.ElementType }[] = [
    { id: "home", label: "Home", icon: Home },
    { id: "logs", label: "Logs", icon: ScrollText },
    { id: "models", label: "Models", icon: Cpu },
    { id: "usage", label: "Token Usage", icon: BarChart3 },
    { id: "settings", label: "Settings", icon: Settings },
  ];

  return (
    <div className="flex h-screen w-full bg-background text-foreground overflow-hidden rounded-lg border shadow-lg">

      <nav className="w-40 border-r bg-muted/30 flex flex-col">
        <div className="p-4 pb-2">
          <h1 className="font-semibold text-sm tracking-tight">Codex Proxy</h1>
        </div>
        
        <div className="flex-1 px-2 py-2 space-y-1 overflow-y-auto">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            return (
              <button
                key={tab.id}
                onClick={() => onTabChange(tab.id)}
                className={cn(
                  "w-full flex items-center gap-2 px-3 py-2 rounded-md text-sm transition-colors",
                  activeTab === tab.id
                    ? "bg-primary text-primary-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-accent hover:text-accent-foreground"
                )}
              >
                <Icon className="w-4 h-4" />
                {tab.label}
              </button>
            );
          })}
        </div>
      </nav>


      <div className="flex-1 flex flex-col min-w-0">

        <header className="h-12 border-b flex items-center justify-between px-4 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium capitalize">
              {tabs.find((t) => t.id === activeTab)?.label}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Badge
              variant={status.running ? "default" : "secondary"}
              className={cn(
                "h-5 px-2 text-[10px] uppercase tracking-wider",
                status.running
                  ? "bg-emerald-500/15 text-emerald-600 hover:bg-emerald-500/25 border-emerald-500/20"
                  : "bg-muted text-muted-foreground"
              )}
            >
              {status.running ? "Running" : "Stopped"}
            </Badge>
          </div>
        </header>


        <main className="flex-1 overflow-auto bg-background">
          {children}
        </main>
      </div>
    </div>
  );
}
