import { useState } from "react";
import Dashboard from "./components/Dashboard";
import Settings from "./components/Settings";

type Tab = "dashboard" | "settings";

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("dashboard");

  return (
    <div className="flex flex-col h-screen bg-slate-900 text-slate-100">
      <header className="flex items-center justify-between px-5 py-3 border-b border-slate-700 bg-slate-900">
        <div className="flex items-center gap-2">
          <img
            src="/vitdaily-icon.png"
            alt=""
            className="h-6 w-6 rounded-md"
          />
          <span className="font-semibold text-slate-100 tracking-tight">VitDaily</span>
        </div>
        <nav className="flex gap-1">
          <button
            onClick={() => setActiveTab("dashboard")}
            className={`px-3 py-1.5 rounded text-sm transition-colors ${
              activeTab === "dashboard"
                ? "bg-slate-700 text-white"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            대시보드
          </button>
          <button
            onClick={() => setActiveTab("settings")}
            className={`px-3 py-1.5 rounded text-sm transition-colors ${
              activeTab === "settings"
                ? "bg-slate-700 text-white"
                : "text-slate-400 hover:text-slate-200"
            }`}
          >
            설정
          </button>
        </nav>
      </header>

      <main className="box-border flex flex-1 justify-center overflow-auto px-6 py-5">
        {activeTab === "settings" ? (
          <Settings />
        ) : (
          <Dashboard />
        )}
      </main>
    </div>
  );
}
