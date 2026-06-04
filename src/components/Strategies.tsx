import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StrategyProfileInfo, ThreadValidationResult } from "../types";
import { logError } from "../utils/logging";

export default function Strategies() {
  const [profiles, setProfiles] = useState<StrategyProfileInfo[]>([]);
  const [results, setResults] = useState<ThreadValidationResult[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<StrategyProfileInfo[]>("get_strategy_profiles")
      .then((result) => {
        setProfiles(result);
        setError("");
      })
      .catch((err) => {
        logError("get_strategy_profiles failed", err);
        setProfiles([]);
        setError(String(err));
      });
    invoke<ThreadValidationResult[]>("get_thread_validation_results")
      .then(setResults)
      .catch((err) => {
        logError("get_thread_validation_results failed", err);
        setResults([]);
      });
  }, []);

  return (
    <div className="w-full max-w-6xl">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-white">전략 프로필</h2>
          <p className="mt-1 text-sm text-slate-400">전략 기준은 구현 기준선이며, 백테스트/운영 증거에 따라 변경될 수 있습니다.</p>
        </div>
        <span className="rounded bg-blue-500/10 px-3 py-1.5 text-xs text-blue-300">최근 1년 백테스트 연결됨</span>
      </div>

      {error && <p className="mb-4 rounded bg-red-500/10 px-3 py-2 text-sm text-red-300">전략 정보를 불러오지 못했습니다: {error}</p>}

      {profiles.length === 0 && !error ? (
        <div className="rounded-xl border border-dashed border-slate-600 bg-slate-800/80 p-8 text-center text-sm text-slate-500">전략 프로필을 불러오는 중입니다.</div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-3">
          {profiles.map((profile) => (
            <article key={profile.profile} className="rounded-xl border border-slate-700 bg-slate-800/80 p-5">
              <div className="mb-4 flex items-start justify-between gap-3">
                <div>
                  <h3 className="text-base font-semibold text-white">{profile.title}</h3>
                  <p className="mt-1 text-xs text-slate-400">{profile.riskLabel}</p>
                </div>
                <span className="rounded bg-slate-900 px-2 py-1 text-[11px] text-slate-300">{profile.tradeFrequency}</span>
              </div>
              <p className="text-sm leading-6 text-slate-300">{profile.summary}</p>
              <div className="mt-4 flex flex-wrap gap-2">
                {profile.indicators.map((indicator) => (
                  <span key={indicator} className="rounded bg-slate-900/80 px-2 py-1 text-[11px] text-slate-300">{indicator}</span>
                ))}
              </div>
            </article>
          ))}
        </div>
      )}

      <section className="mt-5 rounded-xl border border-slate-700 bg-slate-800/80 p-5">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div>
            <h3 className="text-base font-semibold text-white">최근 검증 결과</h3>
            <p className="mt-1 text-xs text-slate-400">스레드 화면에서 실행한 백테스트 결과입니다.</p>
          </div>
          <span className="rounded bg-slate-900 px-2 py-1 text-[11px] text-slate-400">주문 전송 없음</span>
        </div>
        {results.length === 0 ? (
          <p className="rounded-lg border border-dashed border-slate-600 p-4 text-sm text-slate-500">아직 저장된 백테스트 결과가 없습니다.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[760px] text-left text-xs">
              <thead className="text-slate-500">
                <tr>
                  <th className="py-2 pr-3">시장</th>
                  <th className="py-2 pr-3">전략</th>
                  <th className="py-2 pr-3">상태</th>
                  <th className="py-2 pr-3">수익률</th>
                  <th className="py-2 pr-3">DCA</th>
                  <th className="py-2 pr-3">Buy/Hold</th>
                  <th className="py-2 pr-3">MDD</th>
                  <th className="py-2 pr-3">거래</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-700 text-slate-300">
                {results.slice(0, 8).map((result) => (
                  <tr key={result.id}>
                    <td className="py-2 pr-3">{result.market}</td>
                    <td className="py-2 pr-3">{profileTitle(result.strategyProfile)}</td>
                    <td className={`py-2 pr-3 ${result.status === "pass" ? "text-green-300" : "text-red-300"}`}>{result.status === "pass" ? "통과" : "실패"}</td>
                    <td className="py-2 pr-3">{formatPercent(result.returnPercent)}</td>
                    <td className="py-2 pr-3">{formatPercent(result.baselineDcaReturnPercent)}</td>
                    <td className="py-2 pr-3">{formatPercent(result.baselineBuyHoldReturnPercent)}</td>
                    <td className="py-2 pr-3">{formatPercent(result.maxDrawdownPercent)}</td>
                    <td className="py-2 pr-3">{result.simulatedTrades}건</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <div className="mt-5 rounded-xl border border-yellow-500/30 bg-yellow-500/10 p-4 text-sm text-yellow-100">
        이 화면은 전략 분류와 검증 기준을 설명합니다. 수익을 보장하거나 매수/매도를 추천하지 않습니다. 실거래는 백테스트/Paper 검증과 최종 확인 전까지 비활성화됩니다.
      </div>
    </div>
  );
}

function profileTitle(profile: ThreadValidationResult["strategyProfile"]): string {
  return profile === "stable" ? "안정적" : profile === "conservative" ? "보수적" : "공격적";
}

function formatPercent(value: number): string {
  return `${value.toFixed(2)}%`;
}
