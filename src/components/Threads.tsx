import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InvestmentThread, PaperExecutionResult, StrategyProfile, SupportedMarket, ThreadStatus, ThreadValidationResult, ValidationStatus } from "../types";

const markets: SupportedMarket[] = ["KRW-BTC", "KRW-ETH", "KRW-XRP"];
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
  const [paperResult, setPaperResult] = useState<PaperExecutionResult | null>(null);

  useEffect(() => {
    loadThreads();
  }, []);

  async function loadThreads() {
    setError("");
    try {
      const result = await invoke<InvestmentThread[]>("get_investment_threads");
      setThreads(result);
      setSelectedId((current) => current ?? result[0]?.id ?? null);
      setValidationResults(await invoke<ThreadValidationResult[]>("get_thread_validation_results"));
    } catch (err) {
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
      setError(String(err));
    } finally {
      setPaperRunningThreadId(null);
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
        onRunBacktest={handleRunBacktest}
        onRunPaper={handleRunPaper}
        onEdit={(thread) => { setEditTarget(thread); setShowForm(true); }}
        onDelete={handleDelete}
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
          실거래 활성화는 백테스트/모의투자, 글로벌 Live Lock, 최종 확인 단계가 구현된 뒤에만 가능합니다.
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
  onRunBacktest,
  onRunPaper,
  onEdit,
  onDelete,
}: {
  thread: InvestmentThread | null;
  validationResult: ThreadValidationResult | null;
  paperResult: PaperExecutionResult | null;
  isRunningBacktest: boolean;
  isRunningPaper: boolean;
  onRunBacktest: (thread: InvestmentThread) => void;
  onRunPaper: (thread: InvestmentThread) => void;
  onEdit: (thread: InvestmentThread) => void;
  onDelete: (id: string) => void;
}) {
  if (!thread) {
    return (
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-6">
        <p className="text-sm text-slate-300">스레드를 선택하거나 새로 생성하세요.</p>
        <p className="mt-2 text-xs text-slate-500">Product Foundation Sprint에서는 실거래 없이 데이터 모델과 UI 흐름을 먼저 구축합니다.</p>
      </section>
    );
  }

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
            disabled={isRunningPaper || thread.status === "live" || thread.status === "armed"}
            className="mt-3 w-full rounded bg-cyan-500 px-4 py-2 text-sm font-medium text-white hover:bg-cyan-400 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
          >
            {isRunningPaper ? "Paper 실행 중..." : "전략 신호 Paper 실행"}
          </button>
          <p className="mt-2 text-xs text-slate-500">API 키 없이 공개 캔들을 평가하고 모의 주문 로그만 남깁니다.</p>
        </div>
      </div>

      <div className="mt-5 rounded-lg bg-slate-900/60 p-4">
        <h3 className="text-sm font-semibold text-slate-200">Live 활성화</h3>
        <button disabled className="mt-3 w-full rounded bg-slate-700 px-4 py-2 text-sm text-slate-400 opacity-70">
            실거래 활성화 비활성화
        </button>
        <p className="mt-2 text-xs text-slate-500">백테스트 통과, 글로벌 Live Lock, 최종 확인 전에는 활성화할 수 없습니다.</p>
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
