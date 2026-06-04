import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import type { ApiStatus, AppSettings, CredentialReadinessStatus, LiveOrderChanceStatus, LiveOrderGateBlockReason } from "../types";

type ConnectionStatus = "idle" | "testing" | "ok" | "error";

export default function Settings() {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [status, setStatus] = useState<ConnectionStatus>("idle");
  const [notificationsEnabled, setNotificationsEnabled] = useState(true);
  const [globalLiveLocked, setGlobalLiveLocked] = useState(true);
  const [strategyLogicApproved, setStrategyLogicApproved] = useState(false);
  const [showSecret, setShowSecret] = useState(false);
  const [hasCredentials, setHasCredentials] = useState(false);
  const [credentialReadiness, setCredentialReadiness] = useState<CredentialReadinessStatus>("missing");
  const [message, setMessage] = useState("");
  const [chanceStatus, setChanceStatus] = useState<LiveOrderChanceStatus | null>(null);
  const [chanceLoading, setChanceLoading] = useState(false);

  useEffect(() => {
    invoke<ApiStatus>("get_api_status")
      .then((apiStatus) => {
        setHasCredentials(apiStatus.hasCredentials);
        setCredentialReadiness(apiStatus.credentialReadiness);
        setStatus(apiStatus.connected ? "ok" : "idle");
      })
      .catch(() => setStatus("error"));

    loadAppSettings();

    loadLiveOrderChanceStatus();
  }, []);

  async function loadAppSettings() {
    try {
      const settings = await invoke<AppSettings>("get_app_settings");
      setNotificationsEnabled(settings.notificationsEnabled);
      setGlobalLiveLocked(settings.globalLiveLocked);
      setStrategyLogicApproved(settings.strategyLogicApproved);
    } catch {
      setNotificationsEnabled(false);
      setGlobalLiveLocked(true);
      setStrategyLogicApproved(false);
    }
  }

  async function loadLiveOrderChanceStatus() {
    setChanceLoading(true);
    try {
      const status = await invoke<LiveOrderChanceStatus>("get_live_order_chance_status");
      setChanceStatus(status);
      setCredentialReadiness(status.credentialReadiness);
    } catch (error) {
      setChanceStatus(null);
      setMessage(String(error));
    } finally {
      setChanceLoading(false);
    }
  }

  async function handleTest() {
    if (!accessKey || !secretKey) return;
    setStatus("testing");
    setMessage("");

    try {
      await invoke("save_api_credentials", { accessKey, secretKey });
      const apiStatus = await invoke<ApiStatus>("test_api_connection");
      setHasCredentials(apiStatus.hasCredentials);
      setCredentialReadiness(apiStatus.credentialReadiness);
      setStatus(apiStatus.connected ? "ok" : "error");
      setMessage(apiStatus.error ?? "업비트 API 연결을 확인했습니다. API 키 변경으로 Live readiness와 최종 확인은 해제되었습니다.");
      await loadAppSettings();
      await loadLiveOrderChanceStatus();
    } catch (error) {
      setStatus("error");
      setMessage(String(error));
    }
  }

  async function handleSave() {
    if (!accessKey || !secretKey) return;
    setMessage("");

    try {
      await invoke("save_api_credentials", { accessKey, secretKey });
      setHasCredentials(true);
      setCredentialReadiness("stored_unchecked");
      setStatus("idle");
      setMessage("API 키를 OS 키체인에 저장했고 Live readiness와 최종 확인을 해제했습니다");
      await loadAppSettings();
      await loadLiveOrderChanceStatus();
    } catch (error) {
      setStatus("error");
      setMessage(String(error));
    }
  }

  async function handleDelete() {
    try {
      await invoke("delete_api_credentials");
      setAccessKey("");
      setSecretKey("");
      setStatus("idle");
      setHasCredentials(false);
      setCredentialReadiness("missing");
      setChanceStatus(null);
      setMessage("저장된 API 키를 삭제했고 Live readiness와 최종 확인을 해제했습니다");
      await loadAppSettings();
    } catch (error) {
      setStatus("error");
      setMessage(String(error));
    }
  }

  async function toggleNotifications() {
    const nextEnabled = !notificationsEnabled;
    setMessage("");

    try {
      let permissionRequested = false;

      if (nextEnabled) {
        let permissionGranted = await isPermissionGranted();
        if (!permissionGranted) {
          permissionRequested = true;
          permissionGranted = (await requestPermission()) === "granted";
        }

        if (!permissionGranted) {
          await invoke<AppSettings>("set_notifications_enabled", {
            enabled: false,
            permissionRequested,
          });
          setMessage("시스템 알림 권한이 허용되지 않았습니다");
          return;
        }

        permissionRequested = true;
      }

      const settings = await invoke<AppSettings>("set_notifications_enabled", {
        enabled: nextEnabled,
        permissionRequested,
      });
      setNotificationsEnabled(settings.notificationsEnabled);
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function updateLiveTradingSettings(nextLocked: boolean, nextStrategyApproved: boolean) {
    setMessage("");
    try {
      const settings = await invoke<AppSettings>("set_live_trading_settings", {
        globalLiveLocked: nextLocked,
        strategyLogicApproved: nextStrategyApproved,
      });
      setGlobalLiveLocked(settings.globalLiveLocked);
      setStrategyLogicApproved(settings.strategyLogicApproved);
    } catch (error) {
      setMessage(String(error));
    }
  }

  const statusLabel: Record<ConnectionStatus, { text: string; color: string }> = {
    idle: { text: "미확인", color: "text-slate-400" },
    testing: { text: "확인 중...", color: "text-yellow-400" },
    ok: { text: "연결됨", color: "text-green-400" },
    error: { text: "연결 실패", color: "text-red-400" },
  };
  const credentialReadinessLabel: Record<CredentialReadinessStatus, { text: string; color: string }> = {
    missing: { text: "API 키 없음", color: "text-slate-400" },
    stored_unchecked: { text: "저장됨 · 미검증", color: "text-yellow-300" },
    connected: { text: "계정 조회 가능", color: "text-green-300" },
    invalid_key: { text: "Invalid key", color: "text-red-300" },
    revoked_key: { text: "Revoked key", color: "text-red-300" },
    order_permission_missing: { text: "주문 권한 없음", color: "text-red-300" },
    network_error: { text: "확인 실패", color: "text-yellow-300" },
  };
  const chanceReasonLabel: Record<LiveOrderGateBlockReason, string> = {
    global_live_locked: "Global Live Lock",
    credentials_missing: "API 키 없음",
    strategy_logic_not_approved: "전략 승인 필요",
    final_confirmation_missing: "최종 확인 필요",
    live_mode_not_enabled: "Live 상태 아님",
    daily_trade_cap_exceeded: "일일 한도 초과",
    max_loss_exceeded: "손실 기준 초과",
    supported_market_required: "지원 마켓 필요",
    validation_missing: "검증 없음",
    validation_not_passed: "검증 실패",
    legacy_schedule_not_migrated: "레거시 스케줄 차단",
    settings_unavailable: "설정 로드 실패",
    audit_data_unavailable: "감사 데이터 실패",
    invalid_api_key: "Invalid API key",
    revoked_api_key: "Revoked API key",
    insufficient_balance: "잔고 부족",
    minimum_order_amount_not_met: "최소 주문금액 미달",
    market_order_unavailable: "시장가 주문 불가",
    order_permission_denied: "주문 권한 실패",
    order_chance_unavailable: "주문 가능 정보 없음",
  };
  const formatKrw = (value?: number | null) =>
    value == null ? "확인 필요" : `${Math.round(value).toLocaleString()}원`;
  const formatBalance = (value: number, currency: string) =>
    currency === "KRW" ? `${Math.floor(value).toLocaleString()} KRW` : `${value.toFixed(8)} ${currency}`;

  return (
    <div className="flex w-full max-w-[390px] flex-col gap-6">
      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3">업비트 API 키</h2>
        <div className="bg-slate-800 rounded-lg p-4 flex flex-col gap-3">
          {hasCredentials && (
            <p className="rounded bg-green-500/10 px-3 py-2 text-xs text-green-400">
              저장된 API 키가 있습니다. 보안을 위해 기존 키 값은 표시하지 않습니다.
            </p>
          )}
          <div className="rounded border border-slate-700 bg-slate-900/40 px-3 py-2 text-xs">
            <p className="text-slate-500">Credential readiness</p>
            <p className={`mt-1 ${credentialReadinessLabel[credentialReadiness].color}`}>
              {credentialReadinessLabel[credentialReadiness].text}
            </p>
            {["invalid_key", "revoked_key", "order_permission_missing"].includes(credentialReadiness) && (
              <p className="mt-1 text-red-200/80">
                이 상태에서는 Live Order Gate가 fail-closed로 차단되며 API 키를 재발급하거나 주문 권한을 추가해야 합니다.
              </p>
            )}
          </div>
          <div className="flex flex-col gap-1.5">
            <label className="text-xs text-slate-400">Access Key</label>
            <input
              type="text"
              value={accessKey}
              onChange={(e) => setAccessKey(e.target.value)}
              placeholder="업비트 Access Key"
              className="bg-slate-700 text-white rounded px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-orange-500 font-mono"
            />
          </div>

          <div className="flex flex-col gap-1.5">
            <label className="text-xs text-slate-400">Secret Key</label>
            <div className="relative">
              <input
                type={showSecret ? "text" : "password"}
                value={secretKey}
                onChange={(e) => setSecretKey(e.target.value)}
                placeholder="업비트 Secret Key"
                className="w-full bg-slate-700 text-white rounded px-3 py-2 text-sm outline-none focus:ring-1 focus:ring-orange-500 font-mono pr-16"
              />
              <button
                onClick={() => setShowSecret((v) => !v)}
                className="absolute right-2 top-1/2 -translate-y-1/2 text-xs text-slate-400 hover:text-slate-200"
              >
                {showSecret ? "숨기기" : "보기"}
              </button>
            </div>
          </div>

          <div className="flex items-center justify-between pt-1">
            <span className={`text-xs ${statusLabel[status].color}`}>
              {statusLabel[status].text}
            </span>
            <div className="flex gap-2">
              <button
                onClick={handleDelete}
                disabled={!hasCredentials && !accessKey && !secretKey}
                className="text-xs px-3 py-1.5 text-slate-400 hover:text-red-400 transition-colors"
              >
                초기화
              </button>
              <button
                onClick={handleTest}
                disabled={!accessKey || !secretKey || status === "testing"}
                className="text-xs px-3 py-1.5 bg-slate-600 hover:bg-slate-500 text-white rounded disabled:opacity-40 transition-colors"
              >
                연결 테스트
              </button>
              <button
                onClick={handleSave}
                disabled={!accessKey || !secretKey}
                className="text-xs px-3 py-1.5 bg-orange-500 hover:bg-orange-400 text-white rounded disabled:opacity-40 transition-colors"
              >
                저장
              </button>
            </div>
          </div>
          {message && <p className="text-xs text-slate-400">{message}</p>}
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3">알림</h2>
        <div className="bg-slate-800 rounded-lg p-4 flex items-center justify-between">
          <div>
            <p className="text-sm text-slate-200">매수 알림</p>
            <p className="text-xs text-slate-400 mt-0.5">매수 성공/실패 시 시스템 알림 발송</p>
          </div>
          <button
            onClick={toggleNotifications}
            className={`relative w-9 h-5 rounded-full transition-colors ${
              notificationsEnabled ? "bg-orange-500" : "bg-slate-600"
            }`}
          >
            <span
              className={`absolute left-0.5 top-0.5 w-4 h-4 bg-white rounded-full shadow transition-transform ${
                notificationsEnabled ? "translate-x-4" : "translate-x-0"
              }`}
            />
          </button>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3">Live Trading</h2>
        <div className="bg-slate-800 rounded-lg p-4 space-y-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-sm text-slate-200">Global Live Lock</p>
              <p className="mt-0.5 text-xs text-slate-400">
                {globalLiveLocked
                  ? "Global Live Lock이 활성화되어 실거래가 잠겨 있습니다."
                  : "잠금이 해제되어도 스레드별 검증, 최종 확인, API 키, 전략 승인이 모두 필요합니다."}
              </p>
            </div>
            <button
              onClick={() => updateLiveTradingSettings(!globalLiveLocked, strategyLogicApproved)}
              className={`rounded px-3 py-1.5 text-xs ${
                globalLiveLocked ? "bg-slate-700 text-slate-300" : "bg-red-500/10 text-red-300"
              }`}
            >
              {globalLiveLocked ? "Locked" : "Unlocked"}
            </button>
          </div>
          <div className="flex items-center justify-between gap-3 border-t border-slate-700 pt-4">
            <div>
              <p className="text-sm text-slate-200">Strategy Logic Approval</p>
              <p className="mt-0.5 text-xs text-slate-400">백테스트/Paper 로직을 실거래 후보로 승인해야 Live Order Gate가 통과됩니다.</p>
            </div>
            <button
              onClick={() => updateLiveTradingSettings(globalLiveLocked, !strategyLogicApproved)}
              className={`rounded px-3 py-1.5 text-xs ${
                strategyLogicApproved ? "bg-red-500/10 text-red-300" : "bg-slate-700 text-slate-300"
              }`}
            >
              {strategyLogicApproved ? "Approved" : "Not Approved"}
            </button>
          </div>
          <div className="border-t border-slate-700 pt-4">
            <div className="flex items-center justify-between gap-3">
              <div>
                <p className="text-sm text-slate-200">Upbit 주문 가능성</p>
                <p className="mt-0.5 text-xs text-slate-400">
                  /orders/chance로 잔고, 최소 주문금액, 시장가 주문 지원, 주문 권한을 확인합니다.
                </p>
              </div>
              <button
                onClick={loadLiveOrderChanceStatus}
                disabled={chanceLoading}
                className="rounded px-3 py-1.5 text-xs bg-slate-700 text-slate-300 disabled:opacity-40"
              >
                {chanceLoading ? "확인 중" : "다시 확인"}
              </button>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 text-xs">
              <StatusCell label="상태" value={chanceStatus?.allowed ? "통과" : "차단"} danger={!chanceStatus?.allowed} />
              <StatusCell label="마켓" value={chanceStatus?.market ?? "KRW-BTC"} />
              <StatusCell
                label="Credential"
                value={credentialReadinessLabel[chanceStatus?.credentialReadiness ?? credentialReadiness].text}
                danger={["invalid_key", "revoked_key", "order_permission_missing", "missing"].includes(chanceStatus?.credentialReadiness ?? credentialReadiness)}
              />
              <StatusCell
                label="매수 잔고"
                value={chanceStatus ? formatBalance(chanceStatus.bidBalance, chanceStatus.bidCurrency) : "확인 필요"}
              />
              <StatusCell
                label="매도 잔고"
                value={chanceStatus ? formatBalance(chanceStatus.askBalance, chanceStatus.askCurrency) : "확인 필요"}
              />
              <StatusCell label="최소 매수" value={formatKrw(chanceStatus?.minimumBidTotalKrw)} />
              <StatusCell label="최소 매도" value={formatKrw(chanceStatus?.minimumAskTotalKrw)} />
              <StatusCell label="시장가 매수" value={chanceStatus?.marketBuySupported ? "가능" : "차단"} danger={!chanceStatus?.marketBuySupported} />
              <StatusCell label="시장가 매도" value={chanceStatus?.marketSellSupported ? "가능" : "차단"} danger={!chanceStatus?.marketSellSupported} />
            </div>
            {chanceStatus && !chanceStatus.allowed && (
              <div className="mt-3 rounded border border-red-500/20 bg-red-500/5 px-3 py-2 text-xs text-red-100">
                <div className="flex flex-wrap gap-1.5">
                  {chanceStatus.blockReasons.map((reason) => (
                    <span key={reason} className="rounded bg-red-500/10 px-2 py-1">
                      {chanceReasonLabel[reason]}
                    </span>
                  ))}
                </div>
                <p className="mt-2 text-red-100/80">{chanceStatus.reason}</p>
              </div>
            )}
          </div>
        </div>
      </section>

      <section>
        <h2 className="text-sm font-semibold text-slate-300 mb-3">앱 정보</h2>
        <div className="bg-slate-800 rounded-lg p-4">
          <p className="text-sm text-slate-400">VitDaily <span className="text-slate-500">v0.1.0</span></p>
        </div>
      </section>
    </div>
  );
}

function StatusCell({ label, value, danger = false }: { label: string; value: string; danger?: boolean }) {
  return (
    <div className="rounded border border-slate-700 bg-slate-900/40 px-3 py-2">
      <p className="text-[11px] text-slate-500">{label}</p>
      <p className={`mt-1 truncate ${danger ? "text-red-300" : "text-slate-200"}`}>{value}</p>
    </div>
  );
}
