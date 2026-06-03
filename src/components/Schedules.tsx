import ScheduleList from "./ScheduleList";

export default function Schedules() {
  return (
    <div className="w-full max-w-5xl">
      <div className="mb-4 rounded-xl border border-yellow-500/30 bg-yellow-500/10 p-4 text-sm text-yellow-100">
        기존 DCA 스케줄은 보존됩니다. 다만 v1 안전 게이트가 완성되기 전까지 실제 주문 실행은 차단되고 로그/안전 이벤트로 기록됩니다.
      </div>
      <div className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        <ScheduleList />
      </div>
    </div>
  );
}
