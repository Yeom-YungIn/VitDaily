import ScheduleList from "./ScheduleList";

export default function Schedules() {
  return (
    <div className="w-full max-w-5xl">
      <div className="mb-4 rounded-xl border border-yellow-500/30 bg-yellow-500/10 p-4 text-sm text-yellow-100">
        기존 정기 매수 스케줄은 보존됩니다. 새 실거래 흐름은 투자 만들기에서 전략 등록, 과거 테스트, 모의 실행을 거친 뒤 진행합니다.
      </div>
      <div className="rounded-xl border border-slate-700 bg-slate-800/80 p-4">
        <ScheduleList />
      </div>
    </div>
  );
}
