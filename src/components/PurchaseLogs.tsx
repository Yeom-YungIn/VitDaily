import type { AuditCategory, ExecutionMode, PurchaseLog, PurchaseLogSource } from "../types";

interface Props {
  logs?: PurchaseLog[];
  title?: string;
}

export default function PurchaseLogs({ logs = [], title = "최근 매수 내역" }: Props) {
  return (
    <section>
      {title && <h2 className="text-sm font-semibold text-slate-300 mb-3">{title}</h2>}

      {logs.length === 0 ? (
        <div className="bg-slate-800 rounded-lg p-6 text-center text-slate-500 text-sm">
          기록된 주문/감사 로그가 없습니다
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {logs.map((log) => (
            <li
              key={log.id}
              className="flex items-start justify-between gap-3 rounded-lg bg-slate-800 px-4 py-3"
            >
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-sm text-slate-200">{log.title || fallbackTitle(log)}</p>
                  <ModeBadge mode={log.mode ?? "live"} />
                  <SourceBadge source={log.source ?? "legacy_schedule"} />
                  <CategoryBadge category={log.auditCategory ?? fallbackCategory(log)} />
                </div>
                <p className="mt-1 text-[11px] text-slate-500">
                  {new Date(log.executedAt).toLocaleString("ko-KR")}
                </p>
                <p className="text-xs text-slate-400 mt-0.5">
                  {formatPurchaseDetail(log)}
                </p>
                {(log.reason || log.errorMessage) && (
                  <p className="mt-1 max-w-xl text-xs text-red-300">
                    {log.reason || log.errorMessage}
                  </p>
                )}
                {log.strategySignalReason && (
                  <p className="mt-1 max-w-xl text-xs text-cyan-200">
                    {log.strategySignalReason}
                  </p>
                )}
              </div>
              <span
                className={`text-xs px-2 py-0.5 rounded ${statusColor(log.status)}`}
              >
                {statusLabel(log.status)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function fallbackTitle(log: PurchaseLog): string {
  if (log.status === "blocked") return "주문 차단";
  if (log.status === "failure" || log.status === "failed") return "주문 실패";
  if (log.status === "submitted") return "주문 제출";
  if (log.status === "filled") return "주문 체결";
  return log.action === "market_sell" ? "시장가 매도" : "시장가 매수";
}

function fallbackCategory(log: PurchaseLog): AuditCategory {
  if (log.status === "blocked") return "blocked_order";
  if (log.status === "failure" || log.status === "failed") return "api_failure";
  return "trade";
}

function formatPurchaseDetail(log: PurchaseLog): string {
  const volume = Number.isFinite(log.volumeBtc) ? log.volumeBtc : 0;
  const btcText = `${volume.toFixed(8)} BTC`;
  const actionLabel = actionText(log.action);

  if (!["success", "filled"].includes(log.status) || volume <= 0) {
    return `${actionLabel} · ${log.amountKrw.toLocaleString()}원 · ${btcText}`;
  }

  const unitPrice = Math.round(log.amountKrw / volume);
  if (log.action === "market_sell") {
    return `${actionLabel} · 1 BTC = ${unitPrice.toLocaleString()}원일 때, ${btcText} 매도`;
  }
  return `${actionLabel} · 1 BTC = ${unitPrice.toLocaleString()}원일 때, ${log.amountKrw.toLocaleString()}원치 매수 · ${btcText}`;
}

function statusLabel(status: PurchaseLog["status"]): string {
  if (status === "submitted") return "제출";
  if (status === "filled") return "체결";
  if (status === "success") return "성공";
  if (status === "blocked") return "차단";
  return "실패";
}

function statusColor(status: PurchaseLog["status"]): string {
  if (status === "filled" || status === "success") return "bg-green-500/10 text-green-400";
  if (status === "submitted") return "bg-blue-500/10 text-blue-300";
  if (status === "blocked") return "bg-yellow-500/10 text-yellow-300";
  return "bg-red-500/10 text-red-400";
}

function actionText(action?: PurchaseLog["action"]): string {
  if (action === "safety_check") return "안전 게이트 검사";
  if (action === "market_sell") return "시장가 매도";
  return "시장가 매수";
}

function ModeBadge({ mode }: { mode: ExecutionMode }) {
  const color = mode === "live"
    ? "bg-red-500/10 text-red-300"
    : mode === "paper"
      ? "bg-blue-500/10 text-blue-300"
      : "bg-slate-700 text-slate-300";
  return <span className={`rounded px-2 py-0.5 text-[11px] ${color}`}>{mode}</span>;
}

function SourceBadge({ source }: { source: PurchaseLogSource }) {
  const label = source === "legacy_schedule" ? "Legacy Schedule" : source === "investment_thread" ? "Thread" : "System";
  return <span className="rounded bg-slate-700 px-2 py-0.5 text-[11px] text-slate-300">{label}</span>;
}

function CategoryBadge({ category }: { category: AuditCategory }) {
  const label = category.replace(/_/g, " ");
  return <span className="rounded bg-orange-500/10 px-2 py-0.5 text-[11px] text-orange-300">{label}</span>;
}
