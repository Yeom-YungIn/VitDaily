import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ApiStatus, AppSettings, InvestmentThread, PortfolioSnapshot, PurchaseLog } from "../types";

export default function Dashboard() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [threads, setThreads] = useState<InvestmentThread[]>([]);
  const [logsError, setLogsError] = useState("");
  const [threadsError, setThreadsError] = useState("");
  const [portfolio, setPortfolio] = useState<PortfolioSnapshot | null>(null);
  const [portfolioError, setPortfolioError] = useState("");
  const [globalLiveLocked, setGlobalLiveLocked] = useState(true);
  const [apiStatus, setApiStatus] = useState<ApiStatus>({
    connected: false,
    hasCredentials: false,
  });

  useEffect(() => {
    invoke<ApiStatus>("get_api_status")
      .then(setApiStatus)
      .catch(() => setApiStatus({ connected: false, hasCredentials: false }));

    invoke<PurchaseLog[]>("get_purchase_logs")
      .then((result) => {
        setLogs(result);
        setLogsError("");
      })
      .catch((err) => {
        setLogs([]);
        setLogsError(String(err));
      });

    invoke<InvestmentThread[]>("get_investment_threads")
      .then((result) => {
        setThreads(result);
        setThreadsError("");
      })
      .catch((err) => {
        setThreads([]);
        setThreadsError(String(err));
      });

    invoke<AppSettings>("get_app_settings")
      .then((settings) => setGlobalLiveLocked(settings.globalLiveLocked))
      .catch(() => setGlobalLiveLocked(true));

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

  const activeThreads = threads.filter((thread) => !["stopped", "completed"].includes(thread.status));
  const connectionLabel = apiStatus.connected
    ? "업비트 연결됨"
    : apiStatus.hasCredentials
      ? "API 키 저장됨"
      : "API 키 미설정";

  return (
    <div className="w-full max-w-6xl">
      <div className="mb-5 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold text-white">Overview</h1>
          <p className="mt-1 text-sm text-slate-400">포트폴리오, 스레드, 안전 상태를 한 화면에서 확인합니다.</p>
        </div>
        <div className="flex items-center gap-2 rounded-full border border-slate-700 bg-slate-800 px-3 py-1.5 text-sm">
          <span className={`h-2 w-2 rounded-full ${apiStatus.connected ? "bg-green-400" : "bg-slate-500"}`} />
          <span className={apiStatus.connected ? "text-green-400" : "text-slate-400"}>{connectionLabel}</span>
        </div>
      </div>

      <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
        <SummaryCard label="오늘 매수 (KRW)" value={`${todayPurchaseKrw.toLocaleString()}원`} />
        <SummaryCard label="BTC 보유량" value={`${formatBtc(portfolio?.btcTotal ?? 0)} BTC`} detail={portfolio ? `약 ${Math.round(portfolio.btcValueKrw).toLocaleString()}원` : portfolioError || "조회 대기"} tone="orange" />
        <SummaryCard label="활성 스레드" value={`${activeThreads.length}개`} detail={`전체 ${threads.length}개`} />
        <SummaryCard
          label="Live Lock"
          value={globalLiveLocked ? "Locked" : "Unlocked"}
          detail={globalLiveLocked ? "실거래 잠금" : "잠금 해제됨 · v1 실거래는 별도 안전 게이트로 차단"}
          tone={globalLiveLocked ? "danger" : "orange"}
        />
      </div>

      <div className="mt-5 grid gap-5 lg:grid-cols-[1.2fr_0.8fr]">
        <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
          <div className="mb-4 flex items-center justify-between">
            <div>
              <h2 className="text-base font-semibold text-white">자산 성장 차트</h2>
              <p className="mt-1 text-xs text-slate-400">백테스트/스레드 데이터 연결 전까지는 준비 상태를 표시합니다.</p>
            </div>
            <span className="rounded bg-blue-500/10 px-2 py-1 text-[11px] text-blue-300">Paper first</span>
          </div>
          <div className="flex h-56 items-center justify-center rounded-lg border border-dashed border-slate-600 bg-slate-900/50 text-sm text-slate-500">
            포트폴리오 차트 준비 중
          </div>
        </section>

        <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
          <h2 className="text-base font-semibold text-white">최근 활동</h2>
          {(logsError || threadsError) && (
            <div className="mt-4 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
              {threadsError && <p>스레드 데이터를 불러오지 못했습니다: {threadsError}</p>}
              {logsError && <p>주문 로그를 불러오지 못했습니다: {logsError}</p>}
            </div>
          )}
          {logs.length === 0 && threads.length === 0 ? (
            <div className="mt-4 rounded-lg bg-slate-900/60 p-6 text-center text-sm text-slate-500">아직 활동이 없습니다</div>
          ) : (
            <ul className="mt-4 flex flex-col gap-2">
              {threads.slice(0, 3).map((thread) => (
                <li key={thread.id} className="rounded-lg bg-slate-900/60 px-4 py-3 text-sm text-slate-300">
                  {thread.name} 생성됨 · {thread.market}
                </li>
              ))}
              {logs.slice(0, 3).map((log) => {
                const statusLabel = log.status === "success" ? "성공" : log.status === "blocked" ? "차단" : "실패";
                return (
                  <li key={log.id} className="rounded-lg bg-slate-900/60 px-4 py-3 text-sm text-slate-300">
                    스케줄 주문 {statusLabel} · {log.amountKrw.toLocaleString()}원
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}

function SummaryCard({ label, value, detail, tone = "default" }: { label: string; value: string; detail?: string; tone?: "default" | "orange" | "danger" }) {
  const valueClass = tone === "orange" ? "text-orange-400" : tone === "danger" ? "text-red-300" : "text-white";
  return (
    <div className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
      <p className="text-xs text-slate-400">{label}</p>
      <p className={`mt-2 text-xl font-semibold ${valueClass}`}>{value}</p>
      {detail && <p className="mt-1 text-xs text-slate-500">{detail}</p>}
    </div>
  );
}

function formatBtc(value: number): string {
  return Number.isFinite(value) ? value.toFixed(8) : "0.00000000";
}
