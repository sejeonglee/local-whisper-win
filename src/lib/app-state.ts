export type AppPhase =
  | "starting"
  | "downloading_model"
  | "loading_model"
  | "ready"
  | "listening_requested"
  | "listening"
  | "transcribing"
  | "error";

export interface DownloadProgress {
  model: string | null;
  receivedBytes: number;
  totalBytes: number;
  percent: number | null;
}

export interface AppSnapshot {
  phase: AppPhase;
  hotkey: string;
  engine: string | null;
  model: string | null;
  backend: string | null;
  message: string;
  lastError: string | null;
  downloadProgress: DownloadProgress | null;
  isStubBootstrap: boolean;
  updatedAt: number;
}

export const defaultAppState: AppSnapshot = {
  phase: "starting",
  hotkey: "Ctrl+H",
  engine: "whisper",
  model: null,
  backend: null,
  message: "Starting WhisperWindows...",
  lastError: null,
  downloadProgress: null,
  isStubBootstrap: true,
  updatedAt: 0,
};

export function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let index = 0;

  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }

  return `${value.toFixed(value >= 100 ? 0 : 1)} ${units[index]}`;
}
