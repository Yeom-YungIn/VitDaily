import type { PurchaseLog } from "../types";

interface Props {
  logs?: PurchaseLog[];
}

export default function PurchaseLogs({ logs = [] }: Props) {
  return (
    <section>
      <h2 className="text-sm font-semibold text-slate-300 mb-3">최근 매수 내역</h2>

      {logs.length === 0 ? (
        <div className="bg-slate-800 rounded-lg p-6 text-center text-slate-500 text-sm">
          매수 내역이 없습니다
        </div>
      ) : (
        <ul className="flex flex-col gap-2">
          {logs.map((log) => (
            <li
              key={log.id}
              className="bg-slate-800 rounded-lg px-4 py-3 flex items-center justify-between"
            >
              <div>
                <p className="text-sm text-slate-200">
                  {new Date(log.executedAt).toLocaleString("ko-KR")}
                </p>
                <p className="text-xs text-slate-400 mt-0.5">
                  {log.amountKrw.toLocaleString()}원 · {log.volumeBtc.toFixed(8)} BTC
                </p>
                {log.errorMessage && (
                  <p className="mt-1 max-w-xs truncate text-xs text-red-300">
                    {log.errorMessage}
                  </p>
                )}
              </div>
              <span
                className={`text-xs px-2 py-0.5 rounded ${
                  log.status === "success"
                    ? "bg-green-500/10 text-green-400"
                    : "bg-red-500/10 text-red-400"
                }`}
              >
                {log.status === "success" ? "성공" : "실패"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
