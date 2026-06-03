import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { StrategyProfileInfo } from "../types";

export default function Strategies() {
  const [profiles, setProfiles] = useState<StrategyProfileInfo[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    invoke<StrategyProfileInfo[]>("get_strategy_profiles")
      .then((result) => {
        setProfiles(result);
        setError("");
      })
      .catch((err) => {
        setProfiles([]);
        setError(String(err));
      });
  }, []);

  return (
    <div className="w-full max-w-6xl">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold text-white">전략 프로필</h2>
          <p className="mt-1 text-sm text-slate-400">전략 기준은 구현 기준선이며, 백테스트/운영 증거에 따라 변경될 수 있습니다.</p>
        </div>
        <span className="rounded bg-blue-500/10 px-3 py-1.5 text-xs text-blue-300">최근 1년 백테스트 예정</span>
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

      <div className="mt-5 rounded-xl border border-yellow-500/30 bg-yellow-500/10 p-4 text-sm text-yellow-100">
        이 화면은 전략 분류와 검증 기준을 설명합니다. 수익을 보장하거나 매수/매도를 추천하지 않습니다. 실거래는 백테스트/Paper 검증과 최종 확인 전까지 비활성화됩니다.
      </div>
    </div>
  );
}
