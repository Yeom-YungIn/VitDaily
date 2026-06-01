import { useState } from "react";
import type { Schedule } from "../types";

interface Props {
  initial: Schedule | null;
  onSave: (schedule: Schedule) => void;
  onCancel: () => void;
}

export default function ScheduleForm({ initial, onSave, onCancel }: Props) {
  const [time, setTime] = useState(initial?.time ?? "09:00");
  const [amount, setAmount] = useState(initial?.amount?.toString() ?? "5000");
  const [error, setError] = useState("");

  function handleSubmit() {
    const amountNum = parseInt(amount, 10);
    if (isNaN(amountNum) || amountNum < 5000) {
      setError("최소 5,000원 이상 입력해주세요");
      return;
    }

    const now = new Date().toISOString();
    const isEdit = !!initial;

    const schedule: Schedule = isEdit
      ? {
          ...initial!,
          pendingChange: {
            time,
            amount: amountNum,
            applyAt: getTomorrowMidnight(),
          },
          updatedAt: now,
        }
      : {
          id: crypto.randomUUID(),
          time,
          amount: amountNum,
          enabled: true,
          createdAt: now,
          updatedAt: now,
        };

    onSave(schedule);
  }

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
      <div className="bg-slate-800 rounded-xl p-6 w-80 flex flex-col gap-4">
        <h3 className="font-semibold text-slate-100">
          {initial ? "스케줄 편집" : "스케줄 추가"}
        </h3>

        <div className="flex flex-col gap-1.5">
          <label className="text-xs text-slate-400">매수 시간</label>
          <input
            type="time"
            value={time}
            onChange={(e) => setTime(e.target.value)}
            className="bg-slate-700 text-white rounded px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-orange-500"
          />
        </div>

        <div className="flex flex-col gap-1.5">
          <label className="text-xs text-slate-400">매수 금액 (KRW)</label>
          <input
            type="number"
            min={5000}
            step={1000}
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
            className="bg-slate-700 text-white rounded px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-orange-500"
          />
          {error && <p className="text-red-400 text-xs">{error}</p>}
        </div>

        {initial && (
          <p className="text-xs text-yellow-400 bg-yellow-400/10 rounded px-3 py-2">
            변경 사항은 익일부터 적용됩니다
          </p>
        )}

        <div className="flex gap-2 justify-end">
          <button
            onClick={onCancel}
            className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 transition-colors"
          >
            취소
          </button>
          <button
            onClick={handleSubmit}
            className="px-4 py-2 text-sm bg-orange-500 hover:bg-orange-400 text-white rounded transition-colors"
          >
            저장
          </button>
        </div>
      </div>
    </div>
  );
}

function getTomorrowMidnight(): string {
  const d = new Date();
  d.setDate(d.getDate() + 1);
  d.setHours(0, 0, 0, 0);
  return d.toISOString();
}
