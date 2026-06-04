import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Dashboard from "./components/Dashboard";
import Logs from "./components/Logs";
import Schedules from "./components/Schedules";
import Settings from "./components/Settings";
import Strategies from "./components/Strategies";
import Threads from "./components/Threads";
import type { AppSettings } from "./types";
import { logError } from "./utils/logging";

type Tab = "overview" | "threads" | "strategies" | "schedules" | "logs" | "settings";

const tabs: Array<{ id: Tab; label: string }> = [
  { id: "overview", label: "홈" },
  { id: "threads", label: "투자 만들기" },
  { id: "strategies", label: "전략" },
  { id: "schedules", label: "정기 매수" },
  { id: "logs", label: "기록" },
  { id: "settings", label: "설정" },
];

export default function App() {
  const [activeTab, setActiveTab] = useState<Tab>("overview");
  const [globalLiveLocked, setGlobalLiveLocked] = useState(true);

  useEffect(() => {
    invoke<AppSettings>("get_app_settings")
      .then((settings) => setGlobalLiveLocked(settings.globalLiveLocked))
      .catch((err) => {
        logError("get_app_settings failed", err);
        setGlobalLiveLocked(true);
      });
  }, []);

  return (
    <div className="flex h-screen flex-col bg-slate-900 text-slate-100">
      <header className="border-b border-slate-700 bg-slate-900/95 px-5 py-3">
        <div className="mx-auto flex w-full max-w-7xl flex-wrap items-center justify-between gap-3">
          <div className="flex items-center gap-3">
            <img src="/vitdaily-icon.png" alt="" className="h-7 w-7 rounded-md" />
            <div>
              <span className="block font-semibold tracking-tight text-slate-100">VitDaily</span>
              <span className="text-[11px] text-slate-500">전략을 등록하고 검증한 뒤 실행하는 자동 투자 앱</span>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <div className="hidden rounded-full border border-slate-700 bg-slate-800 px-3 py-1 text-xs text-slate-300 sm:block">
              <span className={`mr-1 inline-block h-2 w-2 rounded-full ${globalLiveLocked ? "bg-slate-500" : "bg-red-400"}`} />
              실거래 잠금: {globalLiveLocked ? "켜짐" : "꺼짐"}
            </div>
            <nav className="flex flex-wrap gap-1">
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  onClick={() => setActiveTab(tab.id)}
                  className={`rounded px-3 py-1.5 text-sm transition-colors ${
                    activeTab === tab.id
                      ? "bg-slate-700 text-white"
                      : "text-slate-400 hover:bg-slate-800 hover:text-slate-200"
                  }`}
                >
                  {tab.label}
                </button>
              ))}
            </nav>
          </div>
        </div>
      </header>

      <main className="box-border flex flex-1 justify-center overflow-auto px-5 py-5">
        {activeTab === "overview" && <Dashboard />}
        {activeTab === "threads" && <Threads />}
        {activeTab === "strategies" && <Strategies />}
        {activeTab === "schedules" && <Schedules />}
        {activeTab === "logs" && <Logs />}
        {activeTab === "settings" && <Settings />}
      </main>
    </div>
  );
}
