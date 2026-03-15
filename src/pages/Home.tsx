import { useEffect, useState } from "react";
import { getServerStatus, startServer, stopServer } from "@/lib/tauri";
import { Card, CardContent } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Play, Square, Server, Key, Clock, Activity, AlertCircle } from "lucide-react";

interface ServerStatus {
  running: boolean;
  port: number;
  uptime_secs: number;
  auth_mode: string;
  error?: string;
}

function formatUptime(secs: number): string {
  if (secs === 0) return "0s";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  
  const parts = [];
  if (h > 0) parts.push(`${h}h`);
  if (m > 0) parts.push(`${m}m`);
  if (s > 0 || parts.length === 0) parts.push(`${s}s`);
  
  return parts.join(" ");
}

export default function Home() {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [loading, setLoading] = useState(false);

  const fetchStatus = async () => {
    try {
      const res = await getServerStatus();
      setStatus(res);
    } catch {}
  };

  useEffect(() => {
    fetchStatus();
    const interval = setInterval(fetchStatus, 2000);
    return () => clearInterval(interval);
  }, []);

  const handleStart = async () => {
    setLoading(true);
    try {
      await startServer();
      await fetchStatus();
    } catch {} finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true);
    try {
      await stopServer();
      await fetchStatus();
    } catch {} finally {
      setLoading(false);
    }
  };

  if (!status) {
    return (
      <div className="p-6 space-y-6">
        <Card className="shadow-sm">
          <CardContent className="p-6 flex flex-col items-center justify-center space-y-4">
            <div className="w-16 h-16 rounded-full bg-muted animate-pulse" />
            <div className="h-6 w-32 bg-muted rounded animate-pulse" />
            <div className="h-4 w-24 bg-muted rounded animate-pulse" />
          </CardContent>
        </Card>
        <div className="grid grid-cols-3 gap-4">
          {[1, 2, 3].map((i) => (
            <Card key={i} className="shadow-sm">
              <CardContent className="p-4 space-y-2">
                <div className="h-4 w-16 bg-muted rounded animate-pulse" />
                <div className="h-6 w-24 bg-muted rounded animate-pulse" />
              </CardContent>
            </Card>
          ))}
        </div>
        <div className="h-10 w-full bg-muted rounded animate-pulse" />
      </div>
    );
  }

  return (
    <div className="p-6 space-y-6">
      <Card className="shadow-sm overflow-hidden">
        <div className={`h-1 w-full ${status.error ? 'bg-destructive' : status.running ? 'bg-emerald-500 dark:bg-emerald-400' : 'bg-muted'}`} />
        <CardContent className="p-8 flex flex-col items-center justify-center text-center space-y-4">
          <div className="relative">
            <div className={`absolute inset-0 rounded-full blur-xl opacity-20 ${status.error ? 'bg-destructive' : status.running ? 'bg-emerald-500 dark:bg-emerald-400' : 'bg-muted'}`} />
            <div className={`relative w-20 h-20 rounded-full flex items-center justify-center border-4 ${status.error ? 'border-destructive/20 bg-destructive/10 text-destructive' : status.running ? 'border-emerald-500/20 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400' : 'border-muted bg-muted/50 text-muted-foreground'}`}>
              <Activity className="w-10 h-10" />
            </div>
          </div>
          <div className="space-y-1">
            <h2 className="text-2xl font-semibold tracking-tight text-foreground">
              {status.error ? "Error" : status.running ? "System Online" : "System Offline"}
            </h2>
            <p className="text-sm text-muted-foreground">
              {status.error ? "The server encountered an error" : status.running ? "The proxy server is currently running and accepting requests" : "The proxy server is stopped"}
            </p>
          </div>
          {status.error && (
            <div className="flex items-center gap-2 px-4 py-2 mt-2 text-sm font-medium text-destructive bg-destructive/10 rounded-md">
              <AlertCircle className="w-4 h-4" />
              {status.error}
            </div>
          )}
        </CardContent>
      </Card>

      <div className="grid grid-cols-3 gap-4">
        <Card className="shadow-sm">
          <CardContent className="p-4 flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Server className="w-4 h-4" />
              Port
            </div>
            <div className="text-2xl font-semibold font-mono text-foreground">
              {status.port}
            </div>
          </CardContent>
        </Card>
        <Card className="shadow-sm">
          <CardContent className="p-4 flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Key className="w-4 h-4" />
              Auth Mode
            </div>
            <div className="text-2xl font-semibold font-mono text-foreground">
              {status.auth_mode}
            </div>
          </CardContent>
        </Card>
        <Card className="shadow-sm">
          <CardContent className="p-4 flex flex-col gap-1">
            <div className="flex items-center gap-2 text-sm font-medium text-muted-foreground">
              <Clock className="w-4 h-4" />
              Uptime
            </div>
            <div className="text-2xl font-semibold font-mono text-foreground">
              {formatUptime(status.uptime_secs)}
            </div>
          </CardContent>
        </Card>
      </div>

      {status.running ? (
        <Button 
          variant="destructive" 
          size="lg"
          className="w-full text-base font-medium shadow-sm" 
          onClick={handleStop}
          disabled={loading}
        >
          <Square className="w-5 h-5 mr-2 fill-current" />
          {loading ? "Stopping Server..." : "Stop Server"}
        </Button>
      ) : (
        <Button 
          size="lg"
          className="w-full text-base font-medium shadow-sm bg-emerald-500 hover:bg-emerald-600 dark:bg-emerald-600 dark:hover:bg-emerald-700 text-white" 
          onClick={handleStart}
          disabled={loading}
        >
          <Play className="w-5 h-5 mr-2 fill-current" />
          {loading ? "Starting Server..." : "Start Server"}
        </Button>
      )}
    </div>
  );
}
