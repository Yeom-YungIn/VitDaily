import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InvestmentThread, LiveMarketSellRequest, PaperExecutionResult, StrategyProfile, SupportedMarket, ThreadAutoLoopResult, ThreadStatus, ThreadValidationResult, ValidationStatus } from "../types";
import { logError } from "../utils/logging";

const markets: SupportedMarket[] = ["KRW-BTC", "KRW-ETH", "KRW-XRP"];
const fallbackLiveConfirmationPhrase = "실거래 위험을 이해하고 Live 주문을 활성화합니다";
const strategies: Array<{ value: StrategyProfile; label: string; description: string }> = [
  { value: "stable", label: "안정적", description: "저빈도 · 손실 제한 우선" },
  { value: "conservative", label: "보수적", description: "추세 + 평균회귀 균형" },
  { value: "aggressive", label: "공격적", description: "모멘텀/돌파 · 고위험" },
];

export default function Threads() {
  const [threads, setThreads] = useState<InvestmentThread[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [editTarget, setEditTarget] = useState<InvestmentThread | null>(null);
  const [validationResults, setValidationResults] = useState<ThreadValidationResult[]>([]);
  const [error, setError] = useState("");
  const [runningThreadId, setRunningThreadId] = useState<string | null>(null);
  const [paperRunningThreadId, setPaperRunningThreadId] = useState<string | null>(null);
  const [liveBuyRunningThreadId, setLiveBuyRunningThreadId] = useState<string | null>(null);
  const [liveSellRunningThreadId, setLiveSellRunningThreadId] = useState<string | null>(null);
  const [autoLoopRunningThreadId, setAutoLoopRunningThreadId] = useState<string | null>(null);
  const [allAutoLoopRunning, setAllAutoLoopRunning] = useState(false);
  const [paperResult, setPaperResult] = useState<PaperExecutionResult | null>(null);
  const [autoLoopResult, setAutoLoopResult] = useState<ThreadAutoLoopResult | null>(null);
  const [liveConfirmationPhrase, setLiveConfirmationPhrase] = useState(fallbackLiveConfirmationPhrase);

  useEffect(() => {
    loadThreads();
    invoke<string>("get_live_activation_confirmation_phrase")
      .then(setLiveConfirmationPhrase)
      .catch((err) => {
        logError("get_live_activation_confirmation_phrase failed", err);
        setLiveConfirmationPhrase(fallbackLiveConfirmationPhrase);
      });
  }, []);

  async function loadThreads() {
    setError("");
    try {
      const result = await invoke<InvestmentThread[]>("get_investment_threads");
      setThreads(result);
      setSelectedId((current) => current ?? result[0]?.id ?? null);
      setValidationResults(await invoke<ThreadValidationResult[]>("get_thread_validation_results"));
    } catch (err) {
      logError("load investment threads failed", err);
      setError(String(err));
    }
  }

  async function handleSave(thread: InvestmentThread) {
    setError("");
    try {
      const result = await invoke<InvestmentThread[]>("save_investment_thread", { thread });
      setThreads(result);
      setSelectedId(thread.id);
      setShowForm(false);
    } catch (err) {
      logError("save_investment_thread failed", err);
      setError(String(err));
    }
  }

  async function handleDelete(id: string) {
    setError("");
    try {
      const result = await invoke<InvestmentThread[]>("delete_investment_thread", { id });
      setThreads(result);
      setSelectedId(result[0]?.id ?? null);
    } catch (err) {
      logError("delete_investment_thread failed", err);
      setError(String(err));
    }
  }

  async function handleRunBacktest(thread: InvestmentThread) {
    setError("");
    setRunningThreadId(thread.id);
    try {
      const result = await invoke<ThreadValidationResult>("run_thread_backtest", { threadId: thread.id });
      setValidationResults((current) => [result, ...current.filter((item) => item.threadId !== thread.id)]);
      setThreads((current) =>
        current.map((item) => item.id === thread.id ? { ...item, validationStatus: result.status, updatedAt: new Date().toISOString() } : item),
      );
    } catch (err) {
      logError("run_thread_backtest failed", err);
      setError(String(err));
    } finally {
      setRunningThreadId(null);
    }
  }

  async function handleRunPaper(thread: InvestmentThread) {
    setError("");
    setPaperRunningThreadId(thread.id);
    try {
      const result = await invoke<PaperExecutionResult>("run_thread_paper_execution", { threadId: thread.id });
      setPaperResult(result);
      setThreads((current) =>
        current.map((item) => item.id === thread.id ? { ...item, status: item.status === "draft" ? "paper" : item.status, updatedAt: new Date().toISOString() } : item),
      );
    } catch (err) {
      logError("run_thread_paper_execution failed", err);
      setError(String(err));
    } finally {
      setPaperRunningThreadId(null);
    }
  }

  async function handleRunAutoLoopTick(thread: InvestmentThread) {
    setError("");
    setAutoLoopRunningThreadId(thread.id);
    try {
      const result = await invoke<ThreadAutoLoopResult>("run_thread_auto_loop_tick", { threadId: thread.id });
      setAutoLoopResult(result);
      if (result.paperResult) {
        setPaperResult(result.paperResult);
      }
      await loadThreads();
    } catch (err) {
      logError("run_thread_auto_loop_tick failed", err);
      setError(String(err));
    } finally {
      setAutoLoopRunningThreadId(null);
    }
  }

  async function handleRunAllAutoLoopTicks() {
    setError("");
    setAllAutoLoopRunning(true);
    try {
      const results = await invoke<ThreadAutoLoopResult[]>("run_all_thread_auto_loop_ticks");
      const selectedResult = results.find((result) => result.threadId === selectedId) ?? results[0] ?? null;
      setAutoLoopResult(selectedResult);
      if (selectedResult?.paperResult) {
        setPaperResult(selectedResult.paperResult);
      }
      await loadThreads();
    } catch (err) {
      logError("run_all_thread_auto_loop_ticks failed", err);
      setError(String(err));
    } finally {
      setAllAutoLoopRunning(false);
    }
  }

  async function handleActivate(thread: InvestmentThread, confirmationText: string) {
    setError("");
    try {
      const updated = await invoke<InvestmentThread>("activate_thread_live", {
        request: { threadId: thread.id, confirmationText },
      });
      setThreads((current) => current.map((item) => item.id === updated.id ? updated : item));
      setSelectedId(updated.id);
    } catch (err) {
      logError("activate_thread_live failed", err);
      setError(String(err));
    }
  }

  async function handleStartLive(thread: InvestmentThread) {
    setError("");
    try {
      const updated = await invoke<InvestmentThread>("start_thread_live", { threadId: thread.id });
      setThreads((current) => current.map((item) => item.id === updated.id ? updated : item));
      setSelectedId(updated.id);
    } catch (err) {
      logError("start_thread_live failed", err);
      setError(String(err));
    }
  }

  async function handleSubmitLiveMarketBuy(thread: InvestmentThread) {
    setError("");
    setLiveBuyRunningThreadId(thread.id);
    try {
      await invoke("submit_thread_live_market_buy", { threadId: thread.id, amountKrw: null });
    } catch (err) {
      logError("submit_thread_live_market_buy failed", err);
      setError(String(err));
    } finally {
      setLiveBuyRunningThreadId(null);
    }
  }

  async function handleSubmitLiveMarketSell(thread: InvestmentThread, volume: string) {
    setError("");
    setLiveSellRunningThreadId(thread.id);
    try {
      const request: LiveMarketSellRequest = {
        threadId: thread.id,
        volume,
        estimatedAmountKrw: null,
        policyReason: "manual_pause_stop_policy",
      };
      await invoke("submit_thread_live_market_sell", { request });
    } catch (err) {
      logError("submit_thread_live_market_sell failed", err);
      setError(String(err));
    } finally {
      setLiveSellRunningThreadId(null);
    }
  }

  async function handlePause(thread: InvestmentThread) {
    setError("");
    try {
      const updated = await invoke<InvestmentThread>("pause_thread", { threadId: thread.id });
      setThreads((current) => current.map((item) => item.id === updated.id ? updated : item));
    } catch (err) {
      logError("pause_thread failed", err);
      setError(String(err));
    }
  }

  async function handleStop(thread: InvestmentThread) {
    setError("");
    try {
      const updated = await invoke<InvestmentThread>("stop_thread", { threadId: thread.id });
      setThreads((current) => current.map((item) => item.id === updated.id ? updated : item));
    } catch (err) {
      logError("stop_thread failed", err);
      setError(String(err));
    }
  }

  async function handleComplete(thread: InvestmentThread) {
    setError("");
    try {
      const updated = await invoke<InvestmentThread>("complete_thread", { threadId: thread.id });
      setThreads((current) => current.map((item) => item.id === updated.id ? updated : item));
    } catch (err) {
      logError("complete_thread failed", err);
      setError(String(err));
    }
  }

  const selected = useMemo(
    () => threads.find((thread) => thread.id === selectedId) ?? null,
    [threads, selectedId],
  );

  return (
    <div className="grid w-full max-w-6xl gap-5 lg:grid-cols-[380px_1fr]">
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        <div className="mb-4 flex items-start justify-between gap-3">
          <div>
            <h2 className="text-base font-semibold text-white">투자 스레드</h2>
            <p className="mt-1 text-xs text-slate-400">스레드는 예산, 기간, 전략을 가진 독립 투자 단위입니다.</p>
          </div>
          <button
            onClick={() => { setEditTarget(null); setShowForm(true); }}
            className="rounded bg-orange-500 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-orange-400"
          >
            + 생성
          </button>
          <button
            onClick={handleRunAllAutoLoopTicks}
            disabled={allAutoLoopRunning}
            className="rounded bg-cyan-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-cyan-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {allAutoLoopRunning ? "Loop..." : "전체 tick"}
          </button>
        </div>

        {error && <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-300">{error}</p>}

        {threads.length === 0 ? (
          <div className="rounded-lg border border-dashed border-slate-600 p-6 text-center">
            <p className="text-sm text-slate-300">아직 생성된 스레드가 없습니다.</p>
            <p className="mt-1 text-xs text-slate-500">첫 버전에서는 Paper/Draft 상태만 생성됩니다.</p>
          </div>
        ) : (
          <ul className="flex flex-col gap-2">
            {threads.map((thread) => (
              <li key={thread.id}>
                <button
                  onClick={() => setSelectedId(thread.id)}
                  className={`w-full rounded-lg border px-4 py-3 text-left transition-colors ${
                    selectedId === thread.id
                      ? "border-orange-400 bg-orange-500/10"
                      : "border-slate-700 bg-slate-900/50 hover:border-slate-500"
                  }`}
                >
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <p className="font-medium text-slate-100">{thread.name}</p>
                      <p className="mt-1 text-xs text-slate-400">
                        {thread.market} · {strategyLabel(thread.strategyProfile)} · {thread.initialBudgetKrw.toLocaleString()}원
                      </p>
                    </div>
                    <StatusBadge status={thread.status} />
                  </div>
                  <div className="mt-3 grid grid-cols-3 gap-2 text-xs text-slate-400">
                    <span>{thread.durationDays}일</span>
                    <span>손실 {thread.maxLossPercent}%</span>
                    <span>{thread.dailyTradeCap}회/일</span>
                  </div>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <ThreadDetail
        thread={selected}
        validationResult={validationResults.find((result) => result.threadId === selected?.id) ?? null}
        paperResult={paperResult?.threadId === selected?.id ? paperResult : null}
        isRunningBacktest={runningThreadId === selected?.id}
        isRunningPaper={paperRunningThreadId === selected?.id}
        isRunningLiveBuy={liveBuyRunningThreadId === selected?.id}
        isRunningLiveSell={liveSellRunningThreadId === selected?.id}
        isRunningAutoLoop={autoLoopRunningThreadId === selected?.id}
        autoLoopResult={autoLoopResult?.threadId === selected?.id ? autoLoopResult : null}
        onRunBacktest={handleRunBacktest}
        onRunPaper={handleRunPaper}
        onRunAutoLoopTick={handleRunAutoLoopTick}
        onActivate={handleActivate}
        onStartLive={handleStartLive}
        onSubmitLiveMarketBuy={handleSubmitLiveMarketBuy}
        onSubmitLiveMarketSell={handleSubmitLiveMarketSell}
        onPause={handlePause}
        onStop={handleStop}
        onComplete={handleComplete}
        onEdit={(thread) => { setEditTarget(thread); setShowForm(true); }}
        onDelete={handleDelete}
        liveConfirmationPhrase={liveConfirmationPhrase}
      />

      {showForm && <ThreadForm initial={editTarget} onSave={handleSave} onCancel={() => { setShowForm(false); setEditTarget(null); }} />}
    </div>
  );
}

function ThreadForm({ initial, onSave, onCancel }: { initial: InvestmentThread | null; onSave: (thread: InvestmentThread) => void; onCancel: () => void }) {
  const now = new Date().toISOString();
  const [name, setName] = useState(initial?.name ?? "새 투자 스레드");
  const [market, setMarket] = useState<SupportedMarket>(initial?.market ?? "KRW-BTC");
  const [budget, setBudget] = useState((initial?.initialBudgetKrw ?? 100000).toString());
  const [durationDays, setDurationDays] = useState((initial?.durationDays ?? 30).toString());
  const [strategyProfile, setStrategyProfile] = useState<StrategyProfile>(initial?.strategyProfile ?? "conservative");
  const [maxLossPercent, setMaxLossPercent] = useState((initial?.maxLossPercent ?? 50).toString());
  const [dailyTradeCap, setDailyTradeCap] = useState((initial?.dailyTradeCap ?? 10).toString());
  const [error, setError] = useState("");

  function handleSubmit() {
    const initialBudgetKrw = Number(budget);
    const duration = Number(durationDays);
    const maxLoss = Number(maxLossPercent);
    const cap = Number(dailyTradeCap);

    if (!name.trim()) return setError("스레드 이름을 입력해주세요");
    if (!Number.isFinite(initialBudgetKrw) || initialBudgetKrw < 5000) return setError("투자금은 최소 5,000원 이상이어야 합니다");
    if (!Number.isFinite(duration) || duration < 1) return setError("투자 기간은 1일 이상이어야 합니다");
    if (!Number.isFinite(maxLoss) || maxLoss <= 0 || maxLoss > 100) return setError("최대 손실률은 0% 초과 100% 이하로 입력해주세요");
    if (!Number.isFinite(cap) || cap < 1 || cap > 10) return setError("일일 거래 횟수는 1~10회 사이여야 합니다");

    onSave({
      id: initial?.id ?? crypto.randomUUID(),
      name: name.trim(),
      market,
      initialBudgetKrw,
      durationDays: duration,
      strategyProfile,
      maxLossPercent: maxLoss,
      dailyTradeCap: cap,
      status: initial?.status ?? "draft",
      validationStatus: initial?.validationStatus ?? "missing",
      finalConfirmationStatus: initial?.finalConfirmationStatus ?? "missing",
      createdAt: initial?.createdAt ?? now,
      updatedAt: now,
    });
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 px-4">
      <div className="max-h-[90vh] w-full max-w-xl overflow-auto rounded-xl bg-slate-800 p-6 shadow-xl">
        <div className="mb-5">
          <h3 className="text-lg font-semibold text-white">{initial ? "스레드 편집" : "스레드 생성"}</h3>
          <p className="mt-1 text-xs text-slate-400">현재 단계에서는 실거래가 아닌 Draft/Paper 준비 상태로만 생성됩니다.</p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="스레드 이름" className="sm:col-span-2">
            <input value={name} onChange={(e) => setName(e.target.value)} className="input" />
          </Field>
          <Field label="마켓">
            <select value={market} onChange={(e) => setMarket(e.target.value as SupportedMarket)} className="input">
              {markets.map((value) => <option key={value}>{value}</option>)}
            </select>
          </Field>
          <Field label="투자금액 (KRW)">
            <input type="number" min={5000} step={1000} value={budget} onChange={(e) => setBudget(e.target.value)} className="input" />
          </Field>
          <Field label="투자기간 (일)">
            <input type="number" min={1} value={durationDays} onChange={(e) => setDurationDays(e.target.value)} className="input" />
          </Field>
          <Field label="최대 손실률 (%)">
            <input type="number" min={1} max={100} value={maxLossPercent} onChange={(e) => setMaxLossPercent(e.target.value)} className="input" />
          </Field>
          <Field label="일일 거래 횟수 제한">
            <input type="number" min={1} max={10} value={dailyTradeCap} onChange={(e) => setDailyTradeCap(e.target.value)} className="input" />
          </Field>
          <Field label="전략" className="sm:col-span-2">
            <div className="grid gap-2 sm:grid-cols-3">
              {strategies.map((strategy) => (
                <button
                  key={strategy.value}
                  type="button"
                  onClick={() => setStrategyProfile(strategy.value)}
                  className={`rounded-lg border px-3 py-3 text-left text-xs transition-colors ${
                    strategyProfile === strategy.value
                      ? "border-orange-400 bg-orange-500/10 text-orange-200"
                      : "border-slate-600 bg-slate-900/40 text-slate-300 hover:border-slate-500"
                  }`}
                >
                  <span className="block font-semibold">{strategy.label}</span>
                  <span className="mt-1 block text-slate-400">{strategy.description}</span>
                </button>
              ))}
            </div>
          </Field>
        </div>

        <div className="mt-5 rounded-lg border border-yellow-500/30 bg-yellow-500/10 px-4 py-3 text-xs text-yellow-100">
          실거래 준비 전환은 백테스트 통과, 글로벌 Live Lock 해제, API 키, 전략 승인, 최종 확인을 모두 요구합니다.
        </div>
        {error && <p className="mt-3 text-xs text-red-300">{error}</p>}

        <div className="mt-6 flex justify-end gap-2">
          <button onClick={onCancel} className="rounded px-4 py-2 text-sm text-slate-400 hover:text-slate-200">취소</button>
          <button onClick={handleSubmit} className="rounded bg-orange-500 px-4 py-2 text-sm font-medium text-white hover:bg-orange-400">저장</button>
        </div>
      </div>
    </div>
  );
}

function ThreadDetail({
  thread,
  validationResult,
  paperResult,
  isRunningBacktest,
  isRunningPaper,
  isRunningLiveBuy,
  isRunningLiveSell,
  isRunningAutoLoop,
  autoLoopResult,
  onRunBacktest,
  onRunPaper,
  onRunAutoLoopTick,
  onActivate,
  onStartLive,
  onSubmitLiveMarketBuy,
  onSubmitLiveMarketSell,
  onPause,
  onStop,
  onComplete,
  onEdit,
  onDelete,
  liveConfirmationPhrase,
}: {
  thread: InvestmentThread | null;
  validationResult: ThreadValidationResult | null;
  paperResult: PaperExecutionResult | null;
  isRunningBacktest: boolean;
  isRunningPaper: boolean;
  isRunningLiveBuy: boolean;
  isRunningLiveSell: boolean;
  isRunningAutoLoop: boolean;
  autoLoopResult: ThreadAutoLoopResult | null;
  onRunBacktest: (thread: InvestmentThread) => void;
  onRunPaper: (thread: InvestmentThread) => void;
  onRunAutoLoopTick: (thread: InvestmentThread) => void;
  onActivate: (thread: InvestmentThread, confirmationText: string) => void;
  onStartLive: (thread: InvestmentThread) => void;
  onSubmitLiveMarketBuy: (thread: InvestmentThread) => void;
  onSubmitLiveMarketSell: (thread: InvestmentThread, volume: string) => void;
  onPause: (thread: InvestmentThread) => void;
  onStop: (thread: InvestmentThread) => void;
  onComplete: (thread: InvestmentThread) => void;
  onEdit: (thread: InvestmentThread) => void;
  onDelete: (id: string) => void;
  liveConfirmationPhrase: string;
}) {
  const [confirmationText, setConfirmationText] = useState("");
  const [sellVolume, setSellVolume] = useState("");
  useEffect(() => {
    setConfirmationText("");
    setSellVolume("");
  }, [thread?.id, thread?.finalConfirmedAt]);

  if (!thread) {
    return (
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-6">
        <p className="text-sm text-slate-300">스레드를 선택하거나 새로 생성하세요.</p>
        <p className="mt-2 text-xs text-slate-500">Live readiness 단계에서는 기본 차단 상태에서 검증과 확인을 통과한 스레드만 준비 상태로 전환합니다.</p>
      </section>
    );
  }

  const confirmationMatches = confirmationText.trim() === liveConfirmationPhrase;
  const finalConfirmationSaved = thread.finalConfirmationStatus === "confirmed"
    && thread.finalConfirmationText === liveConfirmationPhrase
    && Boolean(thread.finalConfirmedAt);
  const canRequestActivation = ["draft", "paper", "paused", "armed"].includes(thread.status) && thread.validationStatus === "pass";
  const canStartLive = thread.status === "armed" && thread.validationStatus === "pass" && finalConfirmationSaved;
  const canSubmitLiveBuy = thread.status === "live" && finalConfirmationSaved;
  const canSubmitLiveSell = canSubmitLiveBuy && Number(sellVolume) > 0;
  const canPause = thread.status === "paper" || thread.status === "armed" || thread.status === "live";
  const canStop = !["stopped", "completed"].includes(thread.status);
  const canComplete = ["paper", "paused", "armed", "live"].includes(thread.status);
  const canRunPaper = thread.status === "draft" || thread.status === "paper";
  const canRunAutoLoop = thread.status === "paper" || thread.status === "live";
  const readinessBlockers = liveReadinessBlockers(thread, validationResult, finalConfirmationSaved);

  return (
    <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold text-white">{thread.name}</h2>
            <StatusBadge status={thread.status} />
          </div>
          <p className="mt-1 text-sm text-slate-400">{thread.market} · {strategyLabel(thread.strategyProfile)}</p>
        </div>
        <div className="flex gap-2">
          <button onClick={() => onEdit(thread)} className="rounded px-3 py-1.5 text-xs text-slate-300 hover:bg-slate-700">편집</button>
          <button onClick={() => onDelete(thread.id)} className="rounded px-3 py-1.5 text-xs text-slate-400 hover:bg-red-500/10 hover:text-red-300">삭제</button>
        </div>
      </div>

      <div className="mt-5 grid gap-3 md:grid-cols-4">
        <Metric label="초기 자금" value={`${thread.initialBudgetKrw.toLocaleString()}원`} />
        <Metric label="기간" value={`${thread.durationDays}일`} />
        <Metric label="최대 손실률" value={`${thread.maxLossPercent}%`} tone="danger" />
        <Metric label="일일 거래 제한" value={`${thread.dailyTradeCap}회`} />
      </div>

      <div className="mt-5 grid gap-4 lg:grid-cols-2">
        <div className="rounded-lg bg-slate-900/60 p-4">
          <h3 className="text-sm font-semibold text-slate-200">검증 상태</h3>
          <ValidationBadge status={thread.validationStatus} />
          <button
            onClick={() => onRunBacktest(thread)}
            disabled={isRunningBacktest}
            className="mt-4 w-full rounded bg-blue-500 px-4 py-2 text-sm font-medium text-white hover:bg-blue-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningBacktest ? "백테스트 실행 중..." : "최근 1년 백테스트 실행"}
          </button>
          <p className="mt-2 text-xs text-slate-500">Upbit 공개 60분 캔들로 검증하며 실주문은 전송하지 않습니다.</p>
        </div>
        <div className="rounded-lg bg-slate-900/60 p-4">
          <h3 className="text-sm font-semibold text-slate-200">Paper 실행</h3>
          <button
            onClick={() => onRunPaper(thread)}
            disabled={isRunningPaper || !canRunPaper}
            className="mt-3 w-full rounded bg-cyan-500 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningPaper ? "Paper 실행 중..." : "전략 신호 Paper 실행"}
          </button>
          <p className="mt-2 text-xs text-slate-500">API 키 없이 공개 캔들을 평가하고 모의 주문 로그만 남깁니다.</p>
        </div>
        <div className="rounded-lg bg-slate-900/60 p-4 lg:col-span-2">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-slate-200">자동 실행 loop</h3>
              <p className="mt-1 text-xs text-slate-500">Paper 상태는 credential 없이 모의 tick만 실행하고, Live 상태는 모든 Gate 통과 후에만 주문을 제출합니다.</p>
            </div>
            <button
              onClick={() => onRunAutoLoopTick(thread)}
              disabled={!canRunAutoLoop || isRunningAutoLoop}
              className="rounded bg-cyan-600 px-4 py-2 text-xs font-semibold text-white hover:bg-cyan-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
            >
              {isRunningAutoLoop ? "Tick 실행 중..." : "선택 tick 실행"}
            </button>
          </div>
          {autoLoopResult && (
            <div className="mt-3 rounded border border-slate-700 bg-slate-950/50 px-3 py-2 text-xs">
              <p className="text-slate-300">{autoLoopActionLabel(autoLoopResult.action)} · {autoLoopResult.message}</p>
              <p className="mt-1 break-all text-[11px] text-slate-500">
                mode={autoLoopResult.mode} · retry={autoLoopResult.retryCount} · key={autoLoopResult.idempotencyKey ?? "none"}
              </p>
              {autoLoopResult.liveOrderGate && (
                <p className="mt-1 text-[11px] text-yellow-200">{autoLoopResult.liveOrderGate.reason}</p>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="mt-5 rounded-lg bg-slate-900/60 p-4">
        <h3 className="text-sm font-semibold text-slate-200">Live 활성화</h3>
        <div className="mt-3 rounded border border-red-500/20 bg-red-500/5 px-3 py-3 text-xs text-red-100">
          <p>{thread.market} · {thread.initialBudgetKrw.toLocaleString()}원 · {strategyLabel(thread.strategyProfile)} · 최대 손실 {thread.maxLossPercent}% · 일일 {thread.dailyTradeCap}회</p>
          <p className="mt-3 text-slate-300">아래 문구를 정확히 입력하면 이 스레드에 최종 확인 증거가 저장되고 Armed 상태로 전환됩니다.</p>
          <code className="mt-2 block select-all rounded bg-slate-950/70 px-3 py-2 text-[11px] text-red-100">{liveConfirmationPhrase}</code>
          <input
            value={confirmationText}
            onChange={(event) => setConfirmationText(event.target.value)}
            placeholder={liveConfirmationPhrase}
            className="mt-3 w-full rounded border border-slate-700 bg-slate-950 px-3 py-2 text-xs text-slate-100 outline-none focus:border-red-400"
          />
          <div className="mt-2 flex flex-wrap gap-2 text-[11px]">
            <span className={`rounded px-2 py-1 ${confirmationMatches ? "bg-green-500/10 text-green-200" : "bg-slate-800 text-slate-400"}`}>
              입력 문구 {confirmationMatches ? "일치" : "대기"}
            </span>
            <span className={`rounded px-2 py-1 ${finalConfirmationSaved ? "bg-green-500/10 text-green-200" : "bg-slate-800 text-slate-400"}`}>
              저장 확인 {finalConfirmationSaved ? "완료" : "없음"}
            </span>
          </div>
          {readinessBlockers.length > 0 && (
            <div className="mt-3 flex flex-wrap gap-1.5">
              {readinessBlockers.map((blocker) => (
                <span key={blocker} className="rounded bg-red-500/10 px-2 py-1 text-[11px] text-red-100">
                  {blocker}
                </span>
              ))}
            </div>
          )}
          <p className="mt-3 text-[11px] text-slate-400">Global Lock, API 권한, /orders/chance 잔고와 주문 가능성은 제출 시 Gate에서 다시 확인합니다.</p>
        </div>
        <button
          onClick={() => onActivate(thread, confirmationText)}
          disabled={!canRequestActivation || !confirmationMatches}
          className="mt-3 w-full rounded bg-red-500 px-4 py-2 text-sm font-medium text-white hover:bg-red-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          Armed 준비
        </button>
        <button
          onClick={() => onStartLive(thread)}
          disabled={!canStartLive}
          className="mt-2 w-full rounded bg-rose-600 px-4 py-2 text-sm font-semibold text-white hover:bg-rose-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          Live 시작
        </button>
        <button
          onClick={() => onSubmitLiveMarketBuy(thread)}
          disabled={!canSubmitLiveBuy || isRunningLiveBuy}
          className="mt-2 w-full rounded bg-red-700 px-4 py-2 text-sm font-semibold text-white hover:bg-red-600 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          {isRunningLiveBuy ? "Live 시장가 매수 제출 중..." : "Live 시장가 매수 제출"}
        </button>
        <div className="mt-2 grid gap-2 sm:grid-cols-[1fr_auto]">
          <input
            value={sellVolume}
            onChange={(event) => setSellVolume(event.target.value)}
            placeholder="매도 BTC volume"
            className="rounded border border-slate-700 bg-slate-950 px-3 py-2 text-xs text-slate-100 outline-none focus:border-red-400"
          />
          <button
            onClick={() => onSubmitLiveMarketSell(thread, sellVolume)}
            disabled={!canSubmitLiveSell || isRunningLiveSell}
            className="rounded bg-red-800 px-4 py-2 text-xs font-semibold text-white hover:bg-red-700 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningLiveSell ? "매도 제출 중..." : "Live 시장가 매도"}
          </button>
        </div>
        <div className="mt-2 flex gap-2">
          <button
            onClick={() => onPause(thread)}
            disabled={!canPause}
            className="flex-1 rounded bg-slate-700 px-3 py-2 text-xs text-slate-200 hover:bg-slate-600 disabled:cursor-not-allowed disabled:text-slate-500"
          >
            일시정지
          </button>
          <button
            onClick={() => onStop(thread)}
            disabled={!canStop}
            className="flex-1 rounded bg-slate-700 px-3 py-2 text-xs text-red-200 hover:bg-red-500/20 disabled:cursor-not-allowed disabled:text-slate-500"
          >
            긴급 중지
          </button>
          <button
            onClick={() => onComplete(thread)}
            disabled={!canComplete}
            className="flex-1 rounded bg-slate-700 px-3 py-2 text-xs text-green-200 hover:bg-green-500/20 disabled:cursor-not-allowed disabled:text-slate-500"
          >
            완료
          </button>
        </div>
        <p className="mt-2 text-xs text-slate-500">최종 주문 제출은 별도 Live Order Gate와 Upbit payload preview 테스트 뒤에만 가능하며 기본값은 차단입니다.</p>
      </div>

      {paperResult && (
        <div className="mt-5 rounded-lg border border-cyan-500/30 bg-cyan-500/10 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-cyan-100">마지막 Paper 실행</h3>
              <p className="mt-1 text-xs text-cyan-200/80">{paperResult.message}</p>
            </div>
            <span className={`rounded px-2 py-1 text-xs ${paperResult.duplicate ? "bg-yellow-500/10 text-yellow-200" : "bg-blue-500/10 text-blue-200"}`}>
              {paperResult.duplicate ? "idempotent" : "new tick"}
            </span>
          </div>
          <div className="mt-4 grid gap-3 md:grid-cols-4">
            <Metric label="신호" value={paperActionLabel(paperResult.signal.action)} />
            <Metric label="평가 가격" value={`${Math.round(paperResult.signal.priceKrw).toLocaleString()}원`} />
            <Metric label="모의 로그" value={paperResult.log ? "기록됨" : "없음"} />
            <Metric label="Live Gate" value={paperResult.liveOrderGate.allowed ? "통과" : "차단 확인"} tone={paperResult.liveOrderGate.allowed ? "default" : "danger"} />
          </div>
          <p className="mt-3 text-xs text-cyan-100/80">{paperResult.signal.reason}</p>
          <p className="mt-1 break-all text-[11px] text-slate-400">key: {paperResult.idempotencyKey}</p>
          <p className="mt-1 text-[11px] text-yellow-200">{paperResult.liveOrderGate.reason}</p>
        </div>
      )}

      {validationResult && (
        <div className="mt-5 rounded-lg bg-slate-900/60 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-slate-200">백테스트 결과</h3>
              <p className="mt-1 text-xs text-slate-500">
                {formatDate(validationResult.periodStart)} - {formatDate(validationResult.periodEnd)}
              </p>
            </div>
            <ValidationBadge status={validationResult.status} />
          </div>
          <ValidationChart result={validationResult} />
          <div className="mt-4 grid gap-3 md:grid-cols-4">
            <Metric label="전략 수익률" value={formatPercent(validationResult.returnPercent)} />
            <Metric label="최대 낙폭" value={formatPercent(validationResult.maxDrawdownPercent)} tone="danger" />
            <Metric label="DCA 기준선" value={formatPercent(validationResult.baselineDcaReturnPercent)} />
            <Metric label="Buy/Hold" value={formatPercent(validationResult.baselineBuyHoldReturnPercent)} />
            <Metric label="거래 횟수" value={`${validationResult.simulatedTrades}건`} />
            <Metric label="수수료" value={`${validationResult.feesKrw.toLocaleString()}원`} />
            <Metric label="슬리피지" value={`${validationResult.slippagePercent}%`} />
            <Metric label="최근 90일" value={formatPercent(validationResult.recent90dReturnPercent)} />
          </div>
          <div className="mt-4 grid gap-3 lg:grid-cols-2">
            <ResultList title="판정 사유" items={validationResult.reasons} />
            <ResultList title="가정" items={validationResult.assumptions} />
          </div>
        </div>
      )}
    </section>
  );
}

function liveReadinessBlockers(thread: InvestmentThread, validationResult: ThreadValidationResult | null, finalConfirmationSaved: boolean): string[] {
  const blockers: string[] = [];
  if (thread.validationStatus !== "pass" || validationResult?.status !== "pass") {
    blockers.push("백테스트 통과 필요");
  }
  if (["stopped", "completed"].includes(thread.status)) {
    blockers.push("종료된 스레드는 Live 차단");
  }
  if (!["draft", "paper", "paused", "armed", "live"].includes(thread.status)) {
    blockers.push("지원하지 않는 스레드 상태");
  }
  if (!finalConfirmationSaved) {
    blockers.push("최종 확인 저장 필요");
  }
  if (thread.status !== "armed" && thread.status !== "live") {
    blockers.push("주문 전 Armed/Live 전환 필요");
  }
  return blockers;
}

function Field({ label, className = "", children }: { label: string; className?: string; children: ReactNode }) {
  return <label className={`flex flex-col gap-1.5 text-xs text-slate-400 ${className}`}>{label}{children}</label>;
}

function Metric({ label, value, tone = "default" }: { label: string; value: string; tone?: "default" | "danger" }) {
  return (
    <div className="rounded-lg bg-slate-900/60 p-4">
      <p className="text-xs text-slate-500">{label}</p>
      <p className={`mt-1 text-lg font-semibold ${tone === "danger" ? "text-red-300" : "text-white"}`}>{value}</p>
    </div>
  );
}

function StatusBadge({ status }: { status: ThreadStatus }) {
  const labels: Record<ThreadStatus, string> = {
    draft: "Draft",
    paper: "Paper",
    armed: "Armed",
    live: "Live",
    paused: "Paused",
    stopped: "Stopped",
    completed: "Done",
  };
  const color = status === "live" ? "bg-red-500/10 text-red-300" : status === "draft" ? "bg-slate-500/10 text-slate-300" : "bg-blue-500/10 text-blue-300";
  return <span className={`rounded px-2 py-0.5 text-[11px] font-medium ${color}`}>{labels[status]}</span>;
}

function ValidationBadge({ status }: { status: ValidationStatus }) {
  const labels: Record<ValidationStatus, string> = {
    missing: "백테스트 미실행",
    running: "실행 중",
    pass: "통과",
    fail: "실패",
    stale: "재검증 필요",
  };
  const color = status === "pass" ? "bg-green-500/10 text-green-300" : status === "fail" ? "bg-red-500/10 text-red-300" : "bg-yellow-500/10 text-yellow-300";
  return <span className={`mt-3 inline-flex rounded px-2 py-1 text-xs ${color}`}>{labels[status]}</span>;
}

function strategyLabel(profile: StrategyProfile): string {
  return strategies.find((strategy) => strategy.value === profile)?.label ?? profile;
}

function paperActionLabel(action: PaperExecutionResult["signal"]["action"]): string {
  if (action === "buy") return "모의 매수";
  if (action === "sell") return "모의 청산";
  return "대기";
}

function autoLoopActionLabel(action: ThreadAutoLoopResult["action"]): string {
  const labels: Record<ThreadAutoLoopResult["action"], string> = {
    paper_tick: "Paper tick",
    live_market_buy_submitted: "Live 매수 제출",
    live_gate_blocked: "Gate 차단",
    duplicate_tick: "중복 tick",
    retry_limited: "Retry 제한",
    hold: "대기",
    sell_skipped: "자동 매도 차단",
    skipped: "건너뜀",
  };
  return labels[action];
}

function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}

function formatDate(value: string): string {
  return new Date(value).toLocaleDateString("ko-KR");
}

function ResultList({ title, items }: { title: string; items: string[] }) {
  return (
    <div className="rounded-lg border border-slate-700 bg-slate-950/40 p-3">
      <h4 className="text-xs font-semibold text-slate-300">{title}</h4>
      <ul className="mt-2 space-y-1 text-xs text-slate-400">
        {items.map((item) => (
          <li key={item}>- {item}</li>
        ))}
      </ul>
    </div>
  );
}

function ValidationChart({ result }: { result: ThreadValidationResult }) {
  const rows = [
    { label: "Strategy", value: result.returnPercent, color: "bg-orange-400" },
    { label: "DCA", value: result.baselineDcaReturnPercent, color: "bg-blue-400" },
    { label: "Buy/Hold", value: result.baselineBuyHoldReturnPercent, color: "bg-green-400" },
    { label: "2x Slippage", value: result.doubledSlippageReturnPercent, color: "bg-yellow-400" },
  ];
  const maxAbs = Math.max(...rows.map((row) => Math.abs(row.value)), 1);

  return (
    <div className="mt-4 rounded-lg border border-slate-700 bg-slate-950/40 p-4">
      <div className="mb-3 flex items-center justify-between gap-3">
        <h4 className="text-xs font-semibold text-slate-300">성과 비교</h4>
        <span className="text-[11px] text-slate-500">낙폭 {formatPercent(result.maxDrawdownPercent)}</span>
      </div>
      <div className="flex flex-col gap-3">
        {rows.map((row) => {
          const width = Math.max((Math.abs(row.value) / maxAbs) * 100, 4);
          return (
            <div key={row.label} className="grid grid-cols-[88px_1fr_64px] items-center gap-3 text-xs">
              <span className="text-slate-400">{row.label}</span>
              <div className="h-2 rounded bg-slate-800">
                <div className={`h-2 rounded ${row.color}`} style={{ width: `${width}%`, opacity: row.value < 0 ? 0.55 : 1 }} />
              </div>
              <span className={row.value >= 0 ? "text-green-300" : "text-red-300"}>{formatPercent(row.value)}</span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
