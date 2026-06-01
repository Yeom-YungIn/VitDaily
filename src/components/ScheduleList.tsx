import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Schedule } from "../types";
import ScheduleForm from "./ScheduleForm";

export default function ScheduleList() {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [showForm, setShowForm] = useState(false);
  const [editTarget, setEditTarget] = useState<Schedule | null>(null);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<Schedule[]>("get_schedules")
      .then(setSchedules)
      .catch((err) => setError(String(err)));
  }, []);

  async function toggleEnabled(id: string) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("toggle_schedule", { id }));
    } catch (err) {
      setError(String(err));
    }
  }

  async function deleteSchedule(id: string) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("delete_schedule", { id }));
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleSave(schedule: Schedule) {
    setError("");
    try {
      setSchedules(await invoke<Schedule[]>("save_schedule", { schedule }));
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

      {schedules.length === 0 ? (
        <div className="bg-slate-800 rounded-lg p-6 text-center text-slate-500 text-sm">
          등록된 스케줄이 없습니다
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {schedules.map((s) => (
            <li
              key={s.id}
              className="bg-slate-800 rounded-lg px-4 py-3 flex items-center justify-between"
            >
              <div className="flex items-center gap-3">
                <span className="text-base font-mono font-semibold text-white">
                  {s.time}
                </span>
                <span className="text-slate-300 text-sm">
                  {s.amount.toLocaleString()}원
                </span>
                <span className="text-xs text-slate-500">
                  다음 {getNextRunLabel(s.time)}
                </span>
                {s.pendingChange && (
                  <span className="text-xs text-yellow-400 bg-yellow-400/10 px-2 py-0.5 rounded">
                    익일 반영 대기
                  </span>
                )}
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => toggleEnabled(s.id)}
                  className={`relative w-9 h-5 rounded-full transition-colors ${
                    s.enabled ? "bg-orange-500" : "bg-slate-600"
                  }`}
                >
                  <span
                    className={`absolute left-0.5 top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
                      s.enabled ? "translate-x-4" : "translate-x-0"
                    }`}
                  />
                </button>
                <button
                  onClick={() => { setEditTarget(s); setShowForm(true); }}
                  className="text-slate-400 hover:text-slate-200 text-xs px-2 py-1"
                >
                  편집
                </button>
                <button
                  onClick={() => deleteSchedule(s.id)}
                  className="text-slate-500 hover:text-red-400 text-xs px-2 py-1"
                >
                  삭제
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}

      {error && <p className="mt-3 text-xs text-red-400">{error}</p>}

      {showForm && (
        <ScheduleForm
          initial={editTarget}
          onSave={handleSave}
          onCancel={() => { setShowForm(false); setEditTarget(null); }}
        />
      )}
    </section>
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
