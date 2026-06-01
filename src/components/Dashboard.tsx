import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ScheduleList from "./ScheduleList";
import PurchaseLogs from "./PurchaseLogs";
import type { ApiStatus, PurchaseLog } from "../types";

export default function Dashboard() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [apiStatus, setApiStatus] = useState<ApiStatus>({
    connected: false,
    hasCredentials: false,
  });

  useEffect(() => {
    invoke<ApiStatus>("get_api_status")
      .then(setApiStatus)
      .catch(() => setApiStatus({ connected: false, hasCredentials: false }));

    invoke<PurchaseLog[]>("get_purchase_logs")
      .then(setLogs)
      .catch(() => setLogs([]));
  }, []);

  const summary = useMemo(() => {
    const today = new Date().toLocaleDateString("ko-KR");
    return logs
      .filter(
        (log) =>
          log.status === "success" &&
          new Date(log.executedAt).toLocaleDateString("ko-KR") === today,
      )
      .reduce(
        (acc, log) => ({
          totalKrw: acc.totalKrw + log.amountKrw,
          totalBtc: acc.totalBtc + log.volumeBtc,
        }),
        { totalKrw: 0, totalBtc: 0 },
      );
  }, [logs]);

  const connectionLabel = apiStatus.connected
    ? "업비트 연결됨"
    : apiStatus.hasCredentials
      ? "API 키 저장됨"
      : "API 키 미설정";

  return (
    <div className="p-5 flex flex-col gap-5">
      <div className="flex items-center gap-2 text-sm">
        <span
          className={`w-2 h-2 rounded-full ${
            apiStatus.connected ? "bg-green-400" : "bg-slate-500"
          }`}
        />
        <span className={apiStatus.connected ? "text-green-400" : "text-slate-400"}>
          {connectionLabel}
        </span>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="bg-slate-800 rounded-lg p-4">
          <p className="text-slate-400 text-xs mb-1">오늘 매수 (KRW)</p>
          <p className="text-xl font-semibold text-white">
            {summary.totalKrw.toLocaleString()}원
          </p>
        </div>
        <div className="bg-slate-800 rounded-lg p-4">
          <p className="text-slate-400 text-xs mb-1">오늘 매수 (BTC)</p>
          <p className="text-xl font-semibold text-orange-400">
            {summary.totalBtc.toFixed(8)} BTC
          </p>
        </div>
      </div>

      <ScheduleList />
      <PurchaseLogs logs={logs} />
    </div>
  );
}
