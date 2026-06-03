import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { ApiStatus, AppSettings, InvestmentThread, PortfolioAnalytics, PortfolioSnapshot, PortfolioTimePoint, PurchaseLog, ThreadAnalytics } from "../types";

export default function Dashboard() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [threads, setThreads] = useState<InvestmentThread[]>([]);
  const [logsError, setLogsError] = useState("");
  const [threadsError, setThreadsError] = useState("");
  const [portfolio, setPortfolio] = useState<PortfolioSnapshot | null>(null);
  const [portfolioError, setPortfolioError] = useState("");
  const [analytics, setAnalytics] = useState<PortfolioAnalytics | null>(null);
  const [analyticsError, setAnalyticsError] = useState("");
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

    invoke<PortfolioAnalytics>("get_portfolio_analytics")
      .then((result) => {
        setAnalytics(result);
        setAnalyticsError("");
      })
      .catch((err) => {
        setAnalytics(null);
        setAnalyticsError(String(err));
      });

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
  const summary = analytics?.summary;
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
        <SummaryCard
          label="포트폴리오 추정 가치"
          value={`${(summary?.currentValueKrw ?? 0).toLocaleString()}원`}
          detail={summary?.latestPointSource === "local" ? "로컬 체결 로그 기준" : summary?.latestPointSource === "simulated" ? "백테스트 시뮬레이션 기준" : "스냅샷 대기"}
          tone="orange"
        />
        <SummaryCard
          label="수익률 / 최대 낙폭"
          value={`${formatPercent(summary?.returnPercent ?? 0)} / ${formatPercent(summary?.maxDrawdownPercent ?? 0)}`}
          detail={`투입 ${((summary?.investedKrw || summary?.totalBudgetKrw) ?? 0).toLocaleString()}원`}
        />
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
              <p className="mt-1 text-xs text-slate-400">로컬 체결 로그가 있으면 실제 로그 기준, 없으면 백테스트 결과 기준으로 표시합니다.</p>
            </div>
            <span className="rounded bg-blue-500/10 px-2 py-1 text-[11px] text-blue-300">
              {summary?.latestPointSource === "local" ? "Local" : summary?.latestPointSource === "simulated" ? "Paper" : "Empty"}
            </span>
          </div>
          {analyticsError && <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-300">{analyticsError}</p>}
          <PerformanceChart points={analytics?.timeSeries ?? []} />
          <div className="mt-4 grid gap-3 sm:grid-cols-3">
            <SummaryCard label="활성 스레드" value={`${activeThreads.length}개`} detail={`전체 ${threads.length}개`} />
            <SummaryCard label="차단 주문" value={`${summary?.blockedOrders ?? 0}건`} detail="실거래 보호 로그" tone="danger" />
            <SummaryCard label="BTC 보유량" value={`${formatBtc(portfolio?.btcTotal ?? 0)} BTC`} detail={portfolio ? `약 ${Math.round(portfolio.btcValueKrw).toLocaleString()}원` : portfolioError || "API 조회 없음"} />
          </div>
          <AllocationList allocations={analytics?.allocations ?? []} />
        </section>

        <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
          <h2 className="text-base font-semibold text-white">스레드 성과</h2>
          {(logsError || threadsError) && (
            <div className="mt-4 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
              {threadsError && <p>스레드 데이터를 불러오지 못했습니다: {threadsError}</p>}
              {logsError && <p>주문 로그를 불러오지 못했습니다: {logsError}</p>}
            </div>
          )}
          {(!analytics || analytics.threads.length === 0) ? (
            <div className="mt-4 rounded-lg bg-slate-900/60 p-6 text-center text-sm text-slate-500">백테스트가 실행된 스레드가 없습니다</div>
          ) : (
            <ul className="mt-4 flex flex-col gap-2">
              {analytics.threads.slice(0, 5).map((thread) => <ThreadAnalyticsRow key={thread.threadId} thread={thread} />)}
            </ul>
          )}
          <div className="mt-5">
            <h3 className="text-sm font-semibold text-slate-300">최근 활동</h3>
            {logs.length === 0 && threads.length === 0 ? (
              <div className="mt-3 rounded-lg bg-slate-900/60 p-6 text-center text-sm text-slate-500">아직 활동이 없습니다</div>
            ) : (
              <ul className="mt-3 flex flex-col gap-2">
                {threads.slice(0, 2).map((thread) => (
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
          </div>
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

function formatPercent(value: number): string {
  return `${value >= 0 ? "+" : ""}${value.toFixed(2)}%`;
}

function PerformanceChart({ points }: { points: PortfolioTimePoint[] }) {
  if (points.length === 0) {
    return (
      <div className="flex h-56 items-center justify-center rounded-lg border border-dashed border-slate-600 bg-slate-900/50 text-sm text-slate-500">
        체결 로그 또는 백테스트 결과가 생기면 포트폴리오 시계열이 표시됩니다
      </div>
    );
  }

  const width = 680;
  const height = 220;
  const padding = 18;
  const values = points.map((point) => point.estimatedValueKrw);
  const min = Math.min(...values, ...points.map((point) => point.investedKrw));
  const max = Math.max(...values, ...points.map((point) => point.investedKrw));
  const range = Math.max(max - min, 1);
  const xFor = (index: number) => padding + (index / Math.max(points.length - 1, 1)) * (width - padding * 2);
  const yFor = (value: number) => height - padding - ((value - min) / range) * (height - padding * 2);
  const valuePath = points.map((point, index) => `${index === 0 ? "M" : "L"} ${xFor(index)} ${yFor(point.estimatedValueKrw)}`).join(" ");
  const investedPath = points.map((point, index) => `${index === 0 ? "M" : "L"} ${xFor(index)} ${yFor(point.investedKrw)}`).join(" ");
  const latest = points[points.length - 1];

  return (
    <div className="rounded-lg border border-slate-700 bg-slate-900/60 p-4">
      <svg viewBox={`0 0 ${width} ${height}`} className="h-56 w-full" role="img" aria-label="포트폴리오 가치 시계열">
        <path d={investedPath} fill="none" stroke="#64748b" strokeDasharray="6 6" strokeWidth="2" />
        <path d={valuePath} fill="none" stroke="#fb923c" strokeLinecap="round" strokeLinejoin="round" strokeWidth="3" />
        {points.map((point, index) => (
          <circle key={`${point.date}-${index}`} cx={xFor(index)} cy={yFor(point.estimatedValueKrw)} r="3" fill={point.source === "local" ? "#22c55e" : "#38bdf8"} />
        ))}
      </svg>
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-slate-400">
        <span>{points[0].date} - {latest.date}</span>
        <span>현재 {latest.estimatedValueKrw.toLocaleString()}원 · {formatPercent(latest.returnPercent)}</span>
      </div>
    </div>
  );
}

function AllocationList({ allocations }: { allocations: PortfolioAnalytics["allocations"] }) {
  if (allocations.length === 0) return null;

  return (
    <div className="mt-4 rounded-lg border border-slate-700 bg-slate-900/60 p-4">
      <h3 className="text-sm font-semibold text-slate-300">마켓별 예산 배분</h3>
      <div className="mt-3 flex flex-col gap-3">
        {allocations.map((allocation) => (
          <div key={allocation.market}>
            <div className="flex justify-between text-xs text-slate-400">
              <span>{allocation.market}</span>
              <span>{allocation.budgetKrw.toLocaleString()}원 · {allocation.sharePercent.toFixed(1)}%</span>
            </div>
            <div className="mt-1 h-2 rounded bg-slate-800">
              <div className="h-2 rounded bg-orange-400" style={{ width: `${Math.min(Math.max(allocation.sharePercent, 4), 100)}%` }} />
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ThreadAnalyticsRow({ thread }: { thread: ThreadAnalytics }) {
  const hasReturn = typeof thread.returnPercent === "number";
  return (
    <li className="rounded-lg bg-slate-900/60 px-4 py-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <p className="text-sm font-medium text-slate-200">{thread.threadName}</p>
          <p className="mt-1 text-xs text-slate-500">{thread.market} · {thread.budgetKrw.toLocaleString()}원 · {thread.validationStatus}</p>
        </div>
        <span className={`text-sm font-semibold ${hasReturn && (thread.returnPercent ?? 0) >= 0 ? "text-green-300" : "text-red-300"}`}>
          {hasReturn ? formatPercent(thread.returnPercent ?? 0) : "대기"}
        </span>
      </div>
      <div className="mt-2 grid grid-cols-3 gap-2 text-[11px] text-slate-500">
        <span>낙폭 {typeof thread.maxDrawdownPercent === "number" ? formatPercent(thread.maxDrawdownPercent) : "-"}</span>
        <span>DCA {typeof thread.baselineDcaReturnPercent === "number" ? formatPercent(thread.baselineDcaReturnPercent) : "-"}</span>
        <span>거래 {thread.simulatedTrades ?? 0}건</span>
      </div>
    </li>
  );
}
