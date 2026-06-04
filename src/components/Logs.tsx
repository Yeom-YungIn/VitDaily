import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { AuditCategory, PurchaseLog, SafetyEvent, SafetyEventType } from "../types";
import PurchaseLogs from "./PurchaseLogs";
import { friendlySystemText } from "../utils/copy";
import { logError } from "../utils/logging";

type EventFilter = "all" | SafetyEventType;
type LogStatusFilter = "all" | PurchaseLog["status"];
type CategoryFilter = "all" | AuditCategory;

export default function Logs() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [events, setEvents] = useState<SafetyEvent[]>([]);
  const [logsError, setLogsError] = useState("");
  const [eventsError, setEventsError] = useState("");
  const [eventFilter, setEventFilter] = useState<EventFilter>("all");
  const [logStatusFilter, setLogStatusFilter] = useState<LogStatusFilter>("all");
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");

  useEffect(() => {
    invoke<PurchaseLog[]>("get_purchase_logs")
      .then((result) => {
        setLogs(result);
        setLogsError("");
      })
      .catch((err) => {
        logError("get_purchase_logs failed", err);
        setLogs([]);
        setLogsError(String(err));
      });
    invoke<SafetyEvent[]>("get_safety_events")
      .then((result) => {
        setEvents(result);
        setEventsError("");
      })
      .catch((err) => {
        logError("get_safety_events failed", err);
        setEvents([]);
        setEventsError(String(err));
      });
  }, []);

  const filteredLogs = logs.filter((log) => {
    const statusMatches = logStatusFilter === "all" || log.status === logStatusFilter;
    const category = log.auditCategory ?? (log.status === "blocked" ? "blocked_order" : ["failure", "failed"].includes(log.status) ? "api_failure" : "trade");
    const categoryMatches = categoryFilter === "all" || category === categoryFilter;
    return statusMatches && categoryMatches;
  });
  const filteredEvents = eventFilter === "all" ? events : events.filter((event) => event.eventType === eventFilter);
  const blockedOrders = logs.filter((log) => log.status === "blocked").length;
  const failedOrders = logs.filter((log) => log.status === "failure" || log.status === "failed").length;
  const safetyGateEvents = events.filter((event) => (event.category ?? "safety_gate") === "safety_gate").length;
  const validationEvents = events.filter((event) => (event.category ?? "") === "validation" || event.message.includes("백테스트") || event.eventType === "info").length;

  return (
    <div className="grid w-full max-w-6xl gap-5 lg:grid-cols-[1fr_420px]">
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        {logsError && (
          <div className="mb-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
            주문 로그를 불러오지 못했습니다: {logsError}
          </div>
        )}
        <div className="mb-4 grid gap-3 sm:grid-cols-4">
          <LogStat label="차단 주문" value={`${blockedOrders}건`} tone="warning" />
          <LogStat label="실패 주문" value={`${failedOrders}건`} tone="danger" />
          <LogStat label="보호장치 이벤트" value={`${safetyGateEvents}건`} />
          <LogStat label="검증/정보 이벤트" value={`${validationEvents}건`} />
        </div>
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-sm font-semibold text-slate-300">주문/스케줄 감사 로그</h2>
          <div className="flex flex-wrap gap-2">
            <select value={logStatusFilter} onChange={(event) => setLogStatusFilter(event.target.value as LogStatusFilter)} className="input py-1 text-xs">
              <option value="all">전체 상태</option>
              <option value="submitted">제출</option>
              <option value="filled">체결</option>
              <option value="success">성공</option>
              <option value="failed">실패</option>
              <option value="blocked">차단</option>
              <option value="failure">실패</option>
            </select>
            <select value={categoryFilter} onChange={(event) => setCategoryFilter(event.target.value as CategoryFilter)} className="input py-1 text-xs">
              <option value="all">전체 분류</option>
              <option value="trade">실제 주문</option>
              <option value="paper_trade">모의 주문</option>
              <option value="blocked_order">막은 주문</option>
              <option value="api_failure">API 실패</option>
              <option value="safety_gate">보호장치</option>
              <option value="validation">검증</option>
              <option value="schedule">정기 매수</option>
            </select>
          </div>
        </div>
        <PurchaseLogs logs={filteredLogs} title="" />
      </section>
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-3">
          <h2 className="text-sm font-semibold text-slate-300">안전/검증 이벤트</h2>
          <select value={eventFilter} onChange={(event) => setEventFilter(event.target.value as EventFilter)} className="input py-1 text-xs">
            <option value="all">전체</option>
            <option value="blocked">차단</option>
            <option value="warning">경고</option>
            <option value="stopped">중지</option>
            <option value="info">정보/검증</option>
          </select>
        </div>
        {eventsError && (
          <div className="mb-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
            안전 이벤트를 불러오지 못했습니다: {eventsError}
          </div>
        )}
        {filteredEvents.length === 0 ? (
          <div className="rounded-lg bg-slate-900/60 p-6 text-center text-sm text-slate-500">선택한 이벤트가 없습니다</div>
        ) : (
          <ul className="flex flex-col gap-2">
            {filteredEvents.map((event) => (
              <li key={event.id} className="rounded-lg bg-slate-900/60 px-4 py-3">
                <div className="flex items-center justify-between gap-2">
                  <div className="flex flex-wrap items-center gap-2">
                    <EventBadge type={event.eventType} />
                    <span className="rounded bg-orange-500/10 px-2 py-0.5 text-[11px] text-orange-300">
                      {(event.category ?? "safety_gate").replace(/_/g, " ")}
                    </span>
                    {event.source && <span className="rounded bg-slate-700 px-2 py-0.5 text-[11px] text-slate-300">{event.source}</span>}
                  </div>
                  <span className="text-[11px] text-slate-500">{new Date(event.createdAt).toLocaleString("ko-KR")}</span>
                </div>
                <p className="mt-2 text-sm text-slate-300">{friendlySystemText(event.message)}</p>
                {event.reason && <p className="mt-1 text-xs text-yellow-200">{friendlySystemText(event.reason)}</p>}
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}

function LogStat({ label, value, tone = "default" }: { label: string; value: string; tone?: "default" | "warning" | "danger" }) {
  const color = tone === "danger" ? "text-red-300" : tone === "warning" ? "text-yellow-300" : "text-slate-100";
  return (
    <div className="rounded-lg bg-slate-900/60 px-4 py-3">
      <p className="text-xs text-slate-500">{label}</p>
      <p className={`mt-1 text-lg font-semibold ${color}`}>{value}</p>
    </div>
  );
}

function EventBadge({ type }: { type: SafetyEventType }) {
  const color = type === "blocked"
    ? "bg-yellow-500/10 text-yellow-300"
    : type === "warning"
      ? "bg-orange-500/10 text-orange-300"
      : type === "stopped"
        ? "bg-red-500/10 text-red-300"
        : "bg-blue-500/10 text-blue-300";
  return <span className={`rounded px-2 py-0.5 text-[11px] ${color}`}>{type}</span>;
}
