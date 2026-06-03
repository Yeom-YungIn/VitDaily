import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PurchaseLog, SafetyEvent } from "../types";
import PurchaseLogs from "./PurchaseLogs";

export default function Logs() {
  const [logs, setLogs] = useState<PurchaseLog[]>([]);
  const [events, setEvents] = useState<SafetyEvent[]>([]);
  const [logsError, setLogsError] = useState("");
  const [eventsError, setEventsError] = useState("");

  useEffect(() => {
    invoke<PurchaseLog[]>("get_purchase_logs")
      .then((result) => {
        setLogs(result);
        setLogsError("");
      })
      .catch((err) => {
        setLogs([]);
        setLogsError(String(err));
      });
    invoke<SafetyEvent[]>("get_safety_events")
      .then((result) => {
        setEvents(result);
        setEventsError("");
      })
      .catch((err) => {
        setEvents([]);
        setEventsError(String(err));
      });
  }, []);

  return (
    <div className="grid w-full max-w-6xl gap-5 lg:grid-cols-[1fr_420px]">
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        {logsError && (
          <div className="mb-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
            주문 로그를 불러오지 못했습니다: {logsError}
          </div>
        )}
        <PurchaseLogs logs={logs} title="주문/스케줄 로그" />
      </section>
      <section className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        <h2 className="mb-3 text-sm font-semibold text-slate-300">안전 이벤트</h2>
        {eventsError && (
          <div className="mb-3 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-xs text-red-200">
            안전 이벤트를 불러오지 못했습니다: {eventsError}
          </div>
        )}
        {events.length === 0 ? (
          <div className="rounded-lg bg-slate-900/60 p-6 text-center text-sm text-slate-500">안전 이벤트가 없습니다</div>
        ) : (
          <ul className="flex flex-col gap-2">
            {events.map((event) => (
              <li key={event.id} className="rounded-lg bg-slate-900/60 px-4 py-3">
                <div className="flex items-center justify-between gap-2">
                  <span className="rounded bg-red-500/10 px-2 py-0.5 text-[11px] text-red-300">{event.eventType}</span>
                  <span className="text-[11px] text-slate-500">{new Date(event.createdAt).toLocaleString("ko-KR")}</span>
                </div>
                <p className="mt-2 text-sm text-slate-300">{event.message}</p>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
