import { useEffect, useState } from "react";
import { getServerStatus, startServer, stopServer } from "@/lib/tauri";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Play, Square, Server, Key, Clock, Activity } from "lucide-react";

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
    } catch (error) {
      console.error("Failed to fetch server status:", error);
    }
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
    } catch (error) {
      console.error("Failed to start server:", error);
    } finally {
      setLoading(false);
    }
  };

  const handleStop = async () => {
    setLoading(true);
    try {
      await stopServer();
      await fetchStatus();
    } catch (error) {
      console.error("Failed to stop server:", error);
    } finally {
      setLoading(false);
    }
  };

  if (!status) {
    return <div className="p-4 text-sm text-muted-foreground">Loading...</div>;
  }

  return (
    <div className="p-4 space-y-4">
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <CardTitle className="text-lg flex items-center gap-2">
              <Activity className="w-5 h-5" />
              Server Status
            </CardTitle>
            {status.error ? (
              <Badge variant="destructive">Error</Badge>
            ) : status.running ? (
              <Badge className="bg-emerald-500 hover:bg-emerald-600 text-white border-transparent">Running</Badge>
            ) : (
              <Badge variant="secondary">Stopped</Badge>
            )}
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-4 text-sm">
            <div className="space-y-1">
              <div className="text-muted-foreground flex items-center gap-1.5">
                <Server className="w-4 h-4" />
                Port
              </div>
              <div className="font-medium">{status.port}</div>
            </div>
            <div className="space-y-1">
              <div className="text-muted-foreground flex items-center gap-1.5">
                <Key className="w-4 h-4" />
                Auth Mode
              </div>
              <div className="font-medium">{status.auth_mode}</div>
            </div>
            <div className="space-y-1 col-span-2">
              <div className="text-muted-foreground flex items-center gap-1.5">
                <Clock className="w-4 h-4" />
                Uptime
              </div>
              <div className="font-medium">{formatUptime(status.uptime_secs)}</div>
            </div>
          </div>

          {status.error && (
            <div className="p-3 text-sm text-destructive bg-destructive/10 rounded-md">
              {status.error}
            </div>
          )}

          <div className="pt-2">
            {status.running ? (
              <Button 
                variant="destructive" 
                className="w-full" 
                onClick={handleStop}
                disabled={loading}
              >
                <Square className="w-4 h-4 mr-2" />
                {loading ? "Stopping..." : "Stop Server"}
              </Button>
            ) : (
              <Button 
                className="w-full bg-emerald-500 hover:bg-emerald-600 text-white" 
                onClick={handleStart}
                disabled={loading}
              >
                <Play className="w-4 h-4 mr-2" />
                {loading ? "Starting..." : "Start Server"}
              </Button>
            )}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
