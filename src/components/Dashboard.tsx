import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import ScheduleList from "./ScheduleList";
import PurchaseLogs from "./PurchaseLogs";
import type { ApiStatus, PortfolioSnapshot, PurchaseLog } from "../types";

export default function Dashboard() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [portfolio, setPortfolio] = useState<PortfolioSnapshot | null>(null);
  const [portfolioError, setPortfolioError] = useState("");
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

    invoke<PortfolioSnapshot>("get_portfolio_snapshot")
      .then((snapshot) => {
        setPortfolio(snapshot);
        setPortfolioError("");
      })
      .catch((err) => {
        setPortfolio(null);
        setPortfolioError(String(err));
      });
  }, []);

  const todayPurchaseKrw = useMemo(() => {
    const today = new Date().toLocaleDateString("ko-KR");
    return logs
      .filter(
        (log) =>
          log.status === "success" &&
          new Date(log.executedAt).toLocaleDateString("ko-KR") === today,
      )
      .reduce((total, log) => total + log.amountKrw, 0);
  }, [logs]);

  const connectionLabel = apiStatus.connected
    ? "업비트 연결됨"
    : apiStatus.hasCredentials
      ? "API 키 저장됨"
      : "API 키 미설정";

  return (
    <div className="flex w-full max-w-[390px] flex-col gap-5">
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
            {todayPurchaseKrw.toLocaleString()}원
          </p>
        </div>
        <div className="bg-slate-800 rounded-lg p-4">
          <p className="text-slate-400 text-xs mb-1">현재 보유량</p>
          <p className="text-xl font-semibold text-orange-400">
            {formatBtc(portfolio?.btcTotal ?? 0)} BTC
          </p>
          <p className="mt-1 text-xs text-slate-500">
            {portfolio
              ? `약 ${Math.round(portfolio.btcValueKrw).toLocaleString()}원`
              : portfolioError
                ? "보유량 조회 실패"
                : "새로고침 시 갱신"}
          </p>
        </div>
      </div>

      <div className="bg-slate-800 rounded-lg px-4 py-3">
        <p className="text-xs text-slate-400">KRW-BTC 현재가</p>
        <p className="mt-1 text-sm text-slate-200">
          1 BTC ={" "}
          <span className="font-semibold text-white">
            {portfolio ? Math.round(portfolio.btcPriceKrw).toLocaleString() : 0}원
          </span>
        </p>
        {portfolioError && (
          <p className="mt-1 text-xs text-red-300">{portfolioError}</p>
        )}
      </div>

      <ScheduleList />
      <PurchaseLogs logs={logs} />
    </div>
  );
}

function formatBtc(value: number): string {
  return Number.isFinite(value) ? value.toFixed(8) : "0.00000000";
}
