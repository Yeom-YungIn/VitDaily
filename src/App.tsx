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
          <span className="text-orange-400 text-xl font-bold">₿</span>
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

      <main className="flex-1 overflow-auto">
        {activeTab === "settings" ? (
          <Settings />
        ) : (
          <Dashboard />
        )}
      </main>
    </div>
  );
}
