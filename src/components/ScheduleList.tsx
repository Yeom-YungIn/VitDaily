import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { LegacyScheduleLivePolicyStatus, Schedule } from "../types";
import ScheduleForm from "./ScheduleForm";

export default function ScheduleList() {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [policyStatuses, setPolicyStatuses] = useState<LegacyScheduleLivePolicyStatus[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [editTarget, setEditTarget] = useState<Schedule | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    loadScheduleState();
  }, []);

  async function loadScheduleState() {
    setError("");
    try {
      const [nextSchedules, nextPolicies] = await Promise.all([
        invoke<Schedule[]>("get_schedules"),
        invoke<LegacyScheduleLivePolicyStatus[]>("get_legacy_schedule_live_policy_statuses"),
      ]);
      setSchedules(nextSchedules);
      setPolicyStatuses(nextPolicies);
    } catch (err) {
      setError(String(err));
    }
  }

  async function toggleEnabled(id: string) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("toggle_schedule", { id }));
      setPolicyStatuses(await invoke<LegacyScheduleLivePolicyStatus[]>("get_legacy_schedule_live_policy_statuses"));
    } catch (err) {
      setError(String(err));
    }
  }

  async function deleteSchedule(id: string) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("delete_schedule", { id }));
      setPolicyStatuses(await invoke<LegacyScheduleLivePolicyStatus[]>("get_legacy_schedule_live_policy_statuses"));
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleSave(schedule: Schedule) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("save_schedule", { schedule }));
      setPolicyStatuses(await invoke<LegacyScheduleLivePolicyStatus[]>("get_legacy_schedule_live_policy_statuses"));
      setShowForm(false);
      setEditTarget(null);
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <section>
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-sm font-semibold text-slate-300">스케줄</h2>
        <button
          onClick={() => { setEditTarget(null); setShowForm(true); }}
          className="text-xs px-3 py-1.5 bg-orange-500 hover:bg-orange-400 text-white rounded transition-colors"
        >
          + 추가
        </button>
      </div>

      <div className="mb-3 rounded-lg border border-yellow-500/20 bg-yellow-500/10 px-4 py-3 text-xs text-yellow-100">
        레거시 DCA 스케줄은 제품 정책상 실거래 주문을 직접 제출하지 않습니다. 실행 시 shared Live Order Gate에서 차단 감사 로그만 남기며, 실거래는 투자 스레드 Live 경로를 사용합니다.
      </div>

      {schedules.length === 0 ? (
        <div className="bg-slate-800 rounded-lg p-6 text-center text-slate-500 text-sm">
          등록된 스케줄이 없습니다
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {schedules.map((s) => (
            <ScheduleRow
              key={s.id}
              schedule={s}
              policyStatus={policyStatuses.find((status) => status.scheduleId === s.id)}
              onToggle={toggleEnabled}
              onEdit={(schedule) => { setEditTarget(schedule); setShowForm(true); }}
              onDelete={deleteSchedule}
            />
          ))}
        </ul>
      )}

      {error && <p className="mt-3 text-xs text-red-400">{error}</p>}

      {showForm && (
        <ScheduleForm
          key={editTarget?.id ?? "new"}
          initial={editTarget}
          onSave={handleSave}
          onCancel={() => { setShowForm(false); setEditTarget(null); }}
        />
      )}
    </section>
  );
}

function ScheduleRow({
  schedule,
  policyStatus,
  onToggle,
  onEdit,
  onDelete,
}: {
  schedule: Schedule;
  policyStatus?: LegacyScheduleLivePolicyStatus;
  onToggle: (id: string) => void;
  onEdit: (schedule: Schedule) => void;
  onDelete: (id: string) => void;
}) {
  return (
    <li className="bg-slate-800 rounded-lg px-4 py-3 flex items-center justify-between">
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-base font-mono font-semibold text-white">
          {schedule.time}
        </span>
        <span className="text-slate-300 text-sm">
          {schedule.amount.toLocaleString()}원
        </span>
        <span className="text-xs text-slate-500">
          다음 {getNextRunLabel(schedule.time)}
        </span>
        <span className="rounded bg-red-500/10 px-2 py-1 text-xs text-red-200">
          {policyStatus?.title ?? "레거시 실거래 차단"}
        </span>
        {policyStatus && (
          <span className="max-w-xs text-xs text-slate-500">
            {policyStatus.liveOrderGate.reason}
          </span>
        )}
        {schedule.pendingChange && (
          <div className="flex flex-col gap-0.5">
            <span className="text-xs text-yellow-400 bg-yellow-400/10 px-2 py-0.5 rounded">
              익일 반영 대기
            </span>
            <span className="text-xs text-yellow-300">
              {schedule.pendingChange.time} · {schedule.pendingChange.amount.toLocaleString()}원
            </span>
          </div>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={() => onToggle(schedule.id)}
          className={`relative w-9 h-5 rounded-full transition-colors ${
            schedule.enabled ? "bg-orange-500" : "bg-slate-600"
          }`}
        >
          <span
            className={`absolute left-0.5 top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
              schedule.enabled ? "translate-x-4" : "translate-x-0"
            }`}
          />
        </button>
        <button
          onClick={() => onEdit(schedule)}
          className="text-slate-400 hover:text-slate-200 text-xs px-2 py-1"
        >
          편집
        </button>
        <button
          onClick={() => onDelete(schedule.id)}
          className="text-slate-500 hover:text-red-400 text-xs px-2 py-1"
        >
          삭제
        </button>
      </div>
    </li>
  );
}

function getNextRunLabel(time: string): string {
  const [hours, minutes] = time.split(":").map(Number);
  const nextRun = new Date();
  nextRun.setHours(hours, minutes, 0, 0);

  if (nextRun <= new Date()) {
    nextRun.setDate(nextRun.getDate() + 1);
  }

  return nextRun.toLocaleString("ko-KR", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}
