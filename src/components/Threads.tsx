import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InvestmentThread, LiveMarketSellRequest, PaperExecutionResult, StrategyProfile, SupportedMarket, ThreadAutoLoopResult, ThreadStatus, ThreadValidationResult, ValidationStatus } from "../types";
import { friendlySystemText } from "../utils/copy";
import { logError } from "../utils/logging";

const markets: SupportedMarket[] = ["KRW-BTC", "KRW-ETH", "KRW-XRP"];
const fallbackLiveConfirmationPhrase = "실거래 위험을 이해하고 실제 주문을 활성화합니다";
const strategies: Array<{ value: StrategyProfile; label: string; description: string }> = [
  { value: "stable", label: "안정형", description: "천천히 사고 손실 방어를 우선합니다" },
  { value: "conservative", label: "균형형", description: "추세와 되돌림을 함께 봅니다" },
  { value: "aggressive", label: "공격형", description: "기회가 보이면 더 자주 움직입니다" },
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
            <h2 className="text-base font-semibold text-white">투자 만들기</h2>
            <p className="mt-1 text-xs text-slate-400">원하는 전략을 등록하고 테스트한 뒤 실행합니다.</p>
          </div>
          <button
            onClick={() => { setEditTarget(null); setShowForm(true); }}
            className="rounded bg-orange-500 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-orange-400"
          >
            + 새 전략
          </button>
          <button
            onClick={handleRunAllAutoLoopTicks}
            disabled={allAutoLoopRunning}
            className="rounded bg-cyan-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-cyan-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {allAutoLoopRunning ? "점검 중..." : "전체 점검"}
          </button>
        </div>

        {error && <p className="mb-3 rounded bg-red-500/10 px-3 py-2 text-xs text-red-300">{error}</p>}

        <div className="mb-4 rounded-lg border border-slate-700 bg-slate-900/50 p-3">
          <p className="text-xs font-semibold text-slate-200">사용 순서</p>
          <div className="mt-3 grid gap-2 text-[11px] text-slate-400 sm:grid-cols-2">
            <FlowStep number="1" title="전략 등록" detail="코인, 금액, 기간, 투자 성향을 정합니다." />
            <FlowStep number="2" title="과거 테스트" detail="최근 데이터로 이 전략이 견딜 만한지 확인합니다." />
            <FlowStep number="3" title="모의 실행" detail="실제 주문 없이 오늘 신호와 기록 방식을 봅니다." />
            <FlowStep number="4" title="실거래 준비" detail="API, 잠금 해제, 최종 문구 확인 뒤에만 주문합니다." />
          </div>
        </div>

        {threads.length === 0 ? (
          <div className="rounded-lg border border-dashed border-slate-600 p-6 text-center">
            <p className="text-sm text-slate-300">아직 등록한 전략이 없습니다.</p>
            <p className="mt-1 text-xs text-slate-500">새 전략을 누르면 테스트부터 시작할 수 있습니다.</p>
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
                      <p className="mt-1 text-[11px] text-slate-500">{nextStepText(thread)}</p>
                    </div>
                    <StatusBadge status={thread.status} />
                  </div>
                  <div className="mt-3 grid grid-cols-3 gap-2 text-xs text-slate-400">
                    <span>{thread.durationDays}일</span>
                    <span>멈춤 {thread.maxLossPercent}%</span>
                    <span>하루 {thread.dailyTradeCap}회</span>
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
  const [name, setName] = useState(initial?.name ?? "비트코인 균형 투자");
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
          <h3 className="text-lg font-semibold text-white">{initial ? "전략 편집" : "새 전략 등록"}</h3>
          <p className="mt-1 text-xs text-slate-400">처음 저장하면 실제 주문은 하지 않고 과거 테스트부터 진행합니다.</p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2">
          <Field label="이 전략의 이름" className="sm:col-span-2">
            <input value={name} onChange={(e) => setName(e.target.value)} className="input" />
          </Field>
          <Field label="투자할 코인">
            <select value={market} onChange={(e) => setMarket(e.target.value as SupportedMarket)} className="input">
              {markets.map((value) => <option key={value}>{value}</option>)}
            </select>
          </Field>
          <Field label="처음 배정할 금액">
            <input type="number" min={5000} step={1000} value={budget} onChange={(e) => setBudget(e.target.value)} className="input" />
          </Field>
          <Field label="운영 기간">
            <input type="number" min={1} value={durationDays} onChange={(e) => setDurationDays(e.target.value)} className="input" />
          </Field>
          <Field label="자동으로 멈출 손실 기준">
            <input type="number" min={1} max={100} value={maxLossPercent} onChange={(e) => setMaxLossPercent(e.target.value)} className="input" />
          </Field>
          <Field label="하루 최대 매매 횟수">
            <input type="number" min={1} max={10} value={dailyTradeCap} onChange={(e) => setDailyTradeCap(e.target.value)} className="input" />
          </Field>
          <Field label="투자 성향" className="sm:col-span-2">
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
          저장 직후에는 실제 주문이 나가지 않습니다. 과거 테스트와 모의 실행을 거친 뒤, 설정에서 실거래 잠금을 해제하고 이 전략에서 최종 확인을 해야 주문할 수 있습니다.
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
        <p className="mt-2 text-xs text-slate-500">전략을 만들면 과거 테스트, 모의 실행, 실거래 준비 순서로 진행됩니다.</p>
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
  const guide = nextActionGuide(thread, validationResult, finalConfirmationSaved);

  return (
    <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-semibold text-white">{thread.name}</h2>
            <StatusBadge status={thread.status} />
          </div>
          <p className="mt-1 text-sm text-slate-400">{thread.market} · {strategyLabel(thread.strategyProfile)}</p>
          <p className="mt-1 text-xs text-slate-500">{guide.description}</p>
        </div>
        <div className="flex gap-2">
          <button onClick={() => onEdit(thread)} className="rounded px-3 py-1.5 text-xs text-slate-300 hover:bg-slate-700">편집</button>
          <button onClick={() => onDelete(thread.id)} className="rounded px-3 py-1.5 text-xs text-slate-400 hover:bg-red-500/10 hover:text-red-300">삭제</button>
        </div>
      </div>

      <div className="mt-5 grid gap-3 md:grid-cols-4">
        <Metric label="초기 자금" value={`${thread.initialBudgetKrw.toLocaleString()}원`} />
        <Metric label="기간" value={`${thread.durationDays}일`} />
        <Metric label="멈출 손실 기준" value={`${thread.maxLossPercent}%`} tone="danger" />
        <Metric label="하루 매매 제한" value={`${thread.dailyTradeCap}회`} />
      </div>

      <div className="mt-5 rounded-lg border border-slate-700 bg-slate-900/60 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <p className="text-xs font-semibold text-orange-200">다음에 할 일</p>
            <h3 className="mt-1 text-base font-semibold text-white">{guide.title}</h3>
            <p className="mt-1 text-sm text-slate-400">{guide.help}</p>
          </div>
          <span className="rounded bg-slate-800 px-2 py-1 text-[11px] text-slate-300">{guide.stepLabel}</span>
        </div>
        <div className="mt-4 grid gap-2 sm:grid-cols-4">
          <ProgressPill active={guide.step === 1} done={thread.validationStatus !== "missing"} label="1 전략 등록" />
          <ProgressPill active={guide.step === 2} done={thread.validationStatus === "pass"} label="2 과거 테스트" />
          <ProgressPill active={guide.step === 3} done={thread.status !== "draft"} label="3 모의 실행" />
          <ProgressPill active={guide.step === 4} done={["armed", "live"].includes(thread.status)} label="4 실거래 준비" />
        </div>
      </div>

      <div className="mt-5 grid gap-4 lg:grid-cols-2">
        <div className="rounded-lg bg-slate-900/60 p-4">
          <h3 className="text-sm font-semibold text-slate-200">2단계 · 과거 테스트</h3>
          <ValidationBadge status={thread.validationStatus} />
          <button
            onClick={() => onRunBacktest(thread)}
            disabled={isRunningBacktest}
            className="mt-4 w-full rounded bg-blue-500 px-4 py-2 text-sm font-medium text-white hover:bg-blue-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningBacktest ? "테스트 실행 중..." : "최근 1년으로 테스트"}
          </button>
          <p className="mt-2 text-xs text-slate-500">업비트 공개 가격 데이터로 계산하며 실제 주문은 보내지 않습니다.</p>
        </div>
        <div className="rounded-lg bg-slate-900/60 p-4">
          <h3 className="text-sm font-semibold text-slate-200">3단계 · 모의 실행</h3>
          <button
            onClick={() => onRunPaper(thread)}
            disabled={isRunningPaper || !canRunPaper}
            className="mt-3 w-full rounded bg-cyan-500 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningPaper ? "모의 실행 중..." : "오늘 신호를 모의 실행"}
          </button>
          <p className="mt-2 text-xs text-slate-500">API 키 없이 오늘 신호를 평가하고 가짜 주문 기록만 남깁니다.</p>
        </div>
        <div className="rounded-lg bg-slate-900/60 p-4 lg:col-span-2">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-slate-200">자동 점검</h3>
              <p className="mt-1 text-xs text-slate-500">모의 실행 상태에서는 가짜 주문만 기록하고, 실거래 상태에서는 모든 보호 조건을 통과해야 주문합니다.</p>
            </div>
            <button
              onClick={() => onRunAutoLoopTick(thread)}
              disabled={!canRunAutoLoop || isRunningAutoLoop}
              className="rounded bg-cyan-600 px-4 py-2 text-xs font-semibold text-white hover:bg-cyan-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
            >
              {isRunningAutoLoop ? "점검 중..." : "지금 한 번 점검"}
            </button>
          </div>
          {autoLoopResult && (
            <div className="mt-3 rounded border border-slate-700 bg-slate-950/50 px-3 py-2 text-xs">
              <p className="text-slate-300">{autoLoopActionLabel(autoLoopResult.action)} · {autoLoopResult.message}</p>
              <p className="mt-1 break-all text-[11px] text-slate-500">
                방식={autoLoopResult.mode === "paper" ? "모의 실행" : "실거래"} · 재시도={autoLoopResult.retryCount} · 기록키={autoLoopResult.idempotencyKey ?? "없음"}
              </p>
              {autoLoopResult.liveOrderGate && (
                <p className="mt-1 text-[11px] text-yellow-200">{friendlySystemText(autoLoopResult.liveOrderGate.reason)}</p>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="mt-5 rounded-lg bg-slate-900/60 p-4">
        <h3 className="text-sm font-semibold text-slate-200">4단계 · 실거래 준비</h3>
        <div className="mt-3 rounded border border-red-500/20 bg-red-500/5 px-3 py-3 text-xs text-red-100">
          <p>{thread.market} · {thread.initialBudgetKrw.toLocaleString()}원 · {strategyLabel(thread.strategyProfile)} · 손실 {thread.maxLossPercent}%부터 멈춤 · 하루 {thread.dailyTradeCap}회까지</p>
          <p className="mt-3 text-slate-300">아래 문구를 정확히 입력하면 이 전략만 실거래 준비 상태로 전환됩니다.</p>
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
          <p className="mt-3 text-[11px] text-slate-400">설정의 실거래 잠금, API 권한, 잔고, 최소 주문금액은 주문 직전에 다시 확인합니다.</p>
        </div>
        <button
          onClick={() => onActivate(thread, confirmationText)}
          disabled={!canRequestActivation || !confirmationMatches}
          className="mt-3 w-full rounded bg-red-500 px-4 py-2 text-sm font-medium text-white hover:bg-red-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          이 전략 실거래 준비
        </button>
        <button
          onClick={() => onStartLive(thread)}
          disabled={!canStartLive}
          className="mt-2 w-full rounded bg-rose-600 px-4 py-2 text-sm font-semibold text-white hover:bg-rose-500 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          실거래 시작
        </button>
        <button
          onClick={() => onSubmitLiveMarketBuy(thread)}
          disabled={!canSubmitLiveBuy || isRunningLiveBuy}
          className="mt-2 w-full rounded bg-red-700 px-4 py-2 text-sm font-semibold text-white hover:bg-red-600 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
        >
          {isRunningLiveBuy ? "실제 매수 주문 제출 중..." : "실제 매수 주문 제출"}
        </button>
        <div className="mt-2 grid gap-2 sm:grid-cols-[1fr_auto]">
          <input
            value={sellVolume}
            onChange={(event) => setSellVolume(event.target.value)}
            placeholder="팔 BTC 수량"
            className="rounded border border-slate-700 bg-slate-950 px-3 py-2 text-xs text-slate-100 outline-none focus:border-red-400"
          />
          <button
            onClick={() => onSubmitLiveMarketSell(thread, sellVolume)}
            disabled={!canSubmitLiveSell || isRunningLiveSell}
            className="rounded bg-red-800 px-4 py-2 text-xs font-semibold text-white hover:bg-red-700 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningLiveSell ? "매도 제출 중..." : "실제 매도 주문"}
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
        <p className="mt-2 text-xs text-slate-500">실제 주문은 보호장치와 업비트 주문 미리보기 확인을 통과해야 하며 기본값은 차단입니다.</p>
      </div>

      {paperResult && (
        <div className="mt-5 rounded-lg border border-cyan-500/30 bg-cyan-500/10 p-4">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-cyan-100">마지막 모의 실행</h3>
              <p className="mt-1 text-xs text-cyan-200/80">{friendlySystemText(paperResult.message)}</p>
            </div>
            <span className={`rounded px-2 py-1 text-xs ${paperResult.duplicate ? "bg-yellow-500/10 text-yellow-200" : "bg-blue-500/10 text-blue-200"}`}>
              {paperResult.duplicate ? "이미 기록됨" : "새 기록"}
            </span>
          </div>
          <div className="mt-4 grid gap-3 md:grid-cols-4">
            <Metric label="신호" value={paperActionLabel(paperResult.signal.action)} />
            <Metric label="평가 가격" value={`${Math.round(paperResult.signal.priceKrw).toLocaleString()}원`} />
            <Metric label="모의 로그" value={paperResult.log ? "기록됨" : "없음"} />
            <Metric label="실거래 보호" value={paperResult.liveOrderGate.allowed ? "통과" : "차단 확인"} tone={paperResult.liveOrderGate.allowed ? "default" : "danger"} />
          </div>
          <p className="mt-3 text-xs text-cyan-100/80">{friendlySystemText(paperResult.signal.reason)}</p>
          <p className="mt-1 break-all text-[11px] text-slate-400">기록키: {paperResult.idempotencyKey}</p>
          <p className="mt-1 text-[11px] text-yellow-200">{friendlySystemText(paperResult.liveOrderGate.reason)}</p>
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
            <Metric label="나눠 사기 기준" value={formatPercent(validationResult.baselineDcaReturnPercent)} />
            <Metric label="처음에 전부 사기" value={formatPercent(validationResult.baselineBuyHoldReturnPercent)} />
            <Metric label="거래 횟수" value={`${validationResult.simulatedTrades}건`} />
            <Metric label="수수료" value={`${validationResult.feesKrw.toLocaleString()}원`} />
            <Metric label="체결 비용 가정" value={`${validationResult.slippagePercent}%`} />
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
    blockers.push("종료된 전략은 실거래 차단");
  }
  if (!["draft", "paper", "paused", "armed", "live"].includes(thread.status)) {
    blockers.push("지원하지 않는 스레드 상태");
  }
  if (!finalConfirmationSaved) {
    blockers.push("최종 확인 저장 필요");
  }
  if (thread.status !== "armed" && thread.status !== "live") {
    blockers.push("주문 전 실거래 준비/시작 필요");
  }
  return blockers;
}

function FlowStep({ number, title, detail }: { number: string; title: string; detail: string }) {
  return (
    <div className="flex gap-2 rounded bg-slate-950/40 p-2">
      <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-orange-500/20 text-[10px] font-semibold text-orange-200">{number}</span>
      <span>
        <span className="block font-semibold text-slate-200">{title}</span>
        <span className="mt-0.5 block leading-4">{detail}</span>
      </span>
    </div>
  );
}

function ProgressPill({ label, active, done }: { label: string; active: boolean; done: boolean }) {
  const color = done
    ? "border-green-500/30 bg-green-500/10 text-green-200"
    : active
      ? "border-orange-500/40 bg-orange-500/10 text-orange-200"
      : "border-slate-700 bg-slate-950/40 text-slate-500";
  return <span className={`rounded border px-3 py-2 text-center text-[11px] ${color}`}>{label}</span>;
}

function nextStepText(thread: InvestmentThread): string {
  if (thread.validationStatus === "missing") return "다음: 과거 테스트 실행";
  if (thread.validationStatus === "fail") return "다음: 조건을 조정하고 다시 테스트";
  if (thread.status === "draft") return "다음: 모의 실행으로 신호 확인";
  if (thread.status === "paper") return "다음: 실거래 준비 여부 결정";
  if (thread.status === "armed") return "다음: 실거래 시작";
  if (thread.status === "live") return "현재: 실거래 점검 중";
  if (thread.status === "paused") return "현재: 일시정지";
  if (thread.status === "stopped") return "현재: 중지됨";
  return "현재: 완료됨";
}

function nextActionGuide(thread: InvestmentThread, validationResult: ThreadValidationResult | null, finalConfirmationSaved: boolean) {
  if (thread.validationStatus === "missing" || !validationResult) {
    return {
      step: 2,
      stepLabel: "과거 테스트 필요",
      title: "먼저 최근 1년 데이터로 테스트하세요",
      description: "전략은 등록됐지만 아직 과거 가격으로 검증되지 않았습니다.",
      help: "테스트는 실제 주문을 보내지 않고 수익률, 손실 폭, 거래 횟수를 계산합니다.",
    };
  }
  if (thread.validationStatus === "fail" || validationResult.status === "fail") {
    return {
      step: 2,
      stepLabel: "조건 조정 필요",
      title: "전략 조건을 낮추거나 기간을 바꿔 다시 테스트하세요",
      description: "현재 조건은 앱의 손실/성과 기준을 통과하지 못했습니다.",
      help: "투자금, 손실 기준, 하루 매매 횟수, 투자 성향을 조정한 뒤 다시 테스트할 수 있습니다.",
    };
  }
  if (thread.status === "draft") {
    return {
      step: 3,
      stepLabel: "모의 실행 가능",
      title: "오늘 신호를 모의 실행하세요",
      description: "과거 테스트를 통과했고, 이제 실제 주문 없이 오늘의 신호를 확인할 수 있습니다.",
      help: "모의 실행은 주문 로그처럼 기록되지만 돈은 움직이지 않습니다.",
    };
  }
  if (thread.status === "paper" && !finalConfirmationSaved) {
    return {
      step: 4,
      stepLabel: "실거래 준비 선택",
      title: "실제 주문을 원하면 준비 단계를 진행하세요",
      description: "모의 실행 상태입니다. 실거래는 잠금 해제, API 확인, 최종 문구 입력을 요구합니다.",
      help: "지금은 모의 점검을 반복하거나, 아래 실거래 준비 문구를 입력할 수 있습니다.",
    };
  }
  if (thread.status === "armed") {
    return {
      step: 4,
      stepLabel: "시작 대기",
      title: "실거래 시작 버튼을 누르면 이 전략이 실행 상태가 됩니다",
      description: "최종 확인은 저장됐지만 주문 전 보호장치는 매번 다시 검사됩니다.",
      help: "설정의 실거래 잠금이 켜져 있거나 API 권한이 부족하면 실제 주문은 차단됩니다.",
    };
  }
  if (thread.status === "live") {
    return {
      step: 4,
      stepLabel: "실거래 중",
      title: "자동 점검으로 주문 가능 여부를 확인하세요",
      description: "이 전략은 실거래 상태입니다. 보호 조건을 통과할 때만 주문됩니다.",
      help: "긴급 중지나 일시정지 버튼으로 언제든 멈출 수 있습니다.",
    };
  }
  return {
    step: 4,
    stepLabel: "관리 중",
    title: "상태를 확인하고 필요한 관리 작업을 선택하세요",
    description: "이 전략은 일반 진행 흐름을 벗어난 상태입니다.",
    help: "필요하면 편집, 일시정지, 중지, 완료 처리를 사용할 수 있습니다.",
  };
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
    draft: "등록됨",
    paper: "모의 실행",
    armed: "실거래 준비",
    live: "실거래 중",
    paused: "일시정지",
    stopped: "중지",
    completed: "완료",
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
    paper_tick: "모의 점검",
    live_market_buy_submitted: "실제 매수 제출",
    live_gate_blocked: "보호장치 차단",
    duplicate_tick: "중복 점검",
    retry_limited: "재시도 제한",
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
          <li key={item}>- {friendlySystemText(item)}</li>
        ))}
      </ul>
    </div>
  );
}

function ValidationChart({ result }: { result: ThreadValidationResult }) {
  const rows = [
    { label: "내 전략", value: result.returnPercent, color: "bg-orange-400" },
    { label: "나눠 사기", value: result.baselineDcaReturnPercent, color: "bg-blue-400" },
    { label: "한 번에 사기", value: result.baselineBuyHoldReturnPercent, color: "bg-green-400" },
    { label: "비용 2배", value: result.doubledSlippageReturnPercent, color: "bg-yellow-400" },
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
