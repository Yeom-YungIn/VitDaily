export interface Schedule {
  id: string;
  time: string;
  amount: number;
  enabled: boolean;
  pendingChange?: {
    time: string;
    amount: number;
    applyAt: string;
  };
  createdAt: string;
  updatedAt: string;
}

export interface PurchaseLog {
  id: string;
  scheduleId: string;
  executedAt: string;
  amountKrw: number;
  volumeBtc: number;
  status: "success" | "failure";
  errorMessage?: string;
}

export interface ApiStatus {
  connected: boolean;
  hasCredentials: boolean;
  error?: string;
}

export interface DailySummary {
  totalKrw: number;
  totalBtc: number;
  date: string;
}
