import { error as tauriError } from "@tauri-apps/plugin-log";

export function logError(label: string, error: unknown) {
  console.error(`[VitDaily] ${label}`, error);
  void tauriError(`[VitDaily] ${label}: ${formatError(error)}`).catch(() => {});
}

export function installGlobalErrorLogging() {
  window.addEventListener("error", (event) => {
    logError("window error", event.error ?? event.message);
  });

  window.addEventListener("unhandledrejection", (event) => {
    logError("unhandled rejection", event.reason);
  });
}

function formatError(error: unknown): string {
  if (error instanceof Error) {
    return `${error.name}: ${error.message}${error.stack ? `\n${error.stack}` : ""}`;
  }

  if (typeof error === "string") {
    return error;
  }

  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}
