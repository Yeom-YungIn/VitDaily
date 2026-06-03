import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import type { ApiStatus, AppSettings } from "../types";

type ConnectionStatus = "idle" | "testing" | "ok" | "error";

export default function Settings() {
  const [accessKey, setAccessKey] = useState("");
  const [secretKey, setSecretKey] = useState("");
  const [status, setStatus] = useState<ConnectionStatus>("idle");
  const [notificationsEnabled, setNotificationsEnabled] = useState(true);
  const [globalLiveLocked, setGlobalLiveLocked] = useState(true);
  const [showSecret, setShowSecret] = useState(false);
  const [hasCredentials, setHasCredentials] = useState(false);
  const [message, setMessage] = useState("");

  useEffect(() => {
    invoke<ApiStatus>("get_api_status")
      .then((apiStatus) => {
        setHasCredentials(apiStatus.hasCredentials);
        setStatus(apiStatus.connected ? "ok" : "idle");
      })
      .catch(() => setStatus("error"));

    invoke<AppSettings>("get_app_settings")
      .then((settings) => {
        setNotificationsEnabled(settings.notificationsEnabled);
        setGlobalLiveLocked(settings.globalLiveLocked);
      })
      .catch(() => {
        setNotificationsEnabled(false);
        setGlobalLiveLocked(true);
      });
  }, []);

  async function handleTest() {
    if (!accessKey || !secretKey) return;
    setStatus("testing");
    setMessage("");

    try {
      await invoke("save_api_credentials", { accessKey, secretKey });
      const apiStatus = await invoke<ApiStatus>("test_api_connection");
      setHasCredentials(apiStatus.hasCredentials);
      setStatus(apiStatus.connected ? "ok" : "error");
      setMessage(apiStatus.error ?? "업비트 API 연결을 확인했습니다");
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
      setStatus("idle");
      setMessage("API 키를 OS 키체인에 저장했습니다");
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
      setMessage("저장된 API 키를 삭제했습니다");
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

  const statusLabel: Record<ConnectionStatus, { text: string; color: string }> = {
    idle: { text: "미확인", color: "text-slate-400" },
    testing: { text: "확인 중...", color: "text-yellow-400" },
    ok: { text: "연결됨", color: "text-green-400" },
    error: { text: "연결 실패", color: "text-red-400" },
  };

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
        <div className="bg-slate-800 rounded-lg p-4">
          <div className="flex items-center justify-between gap-3">
            <div>
              <p className="text-sm text-slate-200">Global Live Lock</p>
              <p className="mt-0.5 text-xs text-slate-400">
                {globalLiveLocked
                  ? "Global Live Lock이 활성화되어 실거래가 잠겨 있습니다."
                  : "잠금은 해제되어 있지만 Product Foundation 단계에서는 별도 안전 게이트가 실주문을 차단합니다."}
              </p>
            </div>
            <span
              className={`rounded px-2 py-1 text-xs ${
                globalLiveLocked ? "bg-slate-700 text-slate-300" : "bg-red-500/10 text-red-300"
              }`}
            >
              {globalLiveLocked ? "Locked" : "Unlocked"}
            </span>
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
