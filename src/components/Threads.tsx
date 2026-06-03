import { useEffect, useMemo, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { InvestmentThread, StrategyProfile, SupportedMarket, ThreadStatus, ValidationStatus } from "../types";

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
  const [error, setError] = useState("");

  useEffect(() => {
    loadThreads();
  }, []);

  async function loadThreads() {
    setError("");
    try {
      const result = await invoke<InvestmentThread[]>("get_investment_threads");
      setThreads(result);
      setSelectedId((current) => current ?? result[0]?.id ?? null);
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

      <ThreadDetail thread={selected} onEdit={(thread) => { setEditTarget(thread); setShowForm(true); }} onDelete={handleDelete} />

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

function ThreadDetail({ thread, onEdit, onDelete }: { thread: InvestmentThread | null; onEdit: (thread: InvestmentThread) => void; onDelete: (id: string) => void }) {
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
          <p className="mt-3 text-xs text-slate-400">최근 1년 백테스트와 Paper 검증 패널은 다음 마일스톤에서 연결됩니다.</p>
        </div>
        <div className="rounded-lg bg-slate-900/60 p-4">
          <h3 className="text-sm font-semibold text-slate-200">Live 활성화</h3>
          <button disabled className="mt-3 w-full rounded bg-slate-700 px-4 py-2 text-sm text-slate-400 opacity-70">
            실거래 활성화 비활성화
          </button>
          <p className="mt-2 text-xs text-slate-500">백테스트 통과, 글로벌 Live Lock, 최종 확인 전에는 활성화할 수 없습니다.</p>
        </div>
      </div>
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
  return <span className="mt-3 inline-flex rounded bg-yellow-500/10 px-2 py-1 text-xs text-yellow-300">{labels[status]}</span>;
}

function strategyLabel(profile: StrategyProfile): string {
  return strategies.find((strategy) => strategy.value === profile)?.label ?? profile;
}
