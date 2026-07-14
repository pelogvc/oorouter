import { useState, useEffect } from "react";
import { listen } from "@/lib/tauri";
import { Layout, type Tab } from "@/components/Layout";
import Home from "@/pages/Home";
import Auth from "@/pages/Auth";
import Logs from "@/pages/Logs";
import Models from "@/pages/Models";
import Settings from "@/pages/Settings";
import TokenUsage from "@/pages/TokenUsage";

function App() {
  const [activeTab, setActiveTab] = useState<Tab>("home");

  useEffect(() => {
    const unlisten = listen("navigate-to-settings", () => {
      setActiveTab("settings");
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);
  const renderContent = () => {
    switch (activeTab) {
      case "home":
        return <Home />;
      case "logs":
        return <Logs />;
      case "models":
        return <Models />;
      case "settings":
        return <Settings />;
      case "usage":
        return <TokenUsage />;
      case "auth":
        return <Auth />;
      default:
        return <Home />;
    }
  };

  return (
    <Layout activeTab={activeTab} onTabChange={setActiveTab}>
      {renderContent()}
    </Layout>
  );
}

export default App;
