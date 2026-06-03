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
  status: "success" | "failure" | "blocked";
  errorMessage?: string;
}

export interface ApiStatus {
  connected: boolean;
  hasCredentials: boolean;
  error?: string;
}

export interface PortfolioSnapshot {
  btcBalance: number;
  btcLocked: number;
  btcTotal: number;
  btcPriceKrw: number;
  btcValueKrw: number;
}

export interface DailySummary {
  totalKrw: number;
  totalBtc: number;
  date: string;
}

export interface AppSettings {
  notificationsEnabled: boolean;
  notificationPermissionRequested: boolean;
  globalLiveLocked: boolean;
}


export type SupportedMarket = "KRW-BTC" | "KRW-ETH" | "KRW-XRP";

export type StrategyProfile = "stable" | "conservative" | "aggressive";

export type ThreadStatus =
  | "draft"
  | "paper"
  | "armed"
  | "live"
  | "paused"
  | "stopped"
  | "completed";

export type ValidationStatus = "missing" | "running" | "pass" | "fail" | "stale";

export interface InvestmentThread {
  id: string;
  name: string;
  market: SupportedMarket;
  initialBudgetKrw: number;
  durationDays: number;
  strategyProfile: StrategyProfile;
  maxLossPercent: number;
  dailyTradeCap: number;
  status: ThreadStatus;
  validationStatus: ValidationStatus;
  createdAt: string;
  updatedAt: string;
}

export interface ThreadValidationResult {
  id: string;
  threadId: string;
  status: ValidationStatus;
  periodDays: number;
  periodStart: string;
  periodEnd: string;
  market: SupportedMarket;
  strategyProfile: StrategyProfile;
  simulatedTrades: number;
  returnPercent: number;
  maxDrawdownPercent: number;
  baselineDcaReturnPercent: number;
  baselineDcaMaxDrawdownPercent: number;
  baselineBuyHoldReturnPercent: number;
  baselineBuyHoldMaxDrawdownPercent: number;
  recent90dReturnPercent: number;
  recent90dDcaReturnPercent: number;
  feesKrw: number;
  feePercent: number;
  slippagePercent: number;
  doubledSlippageReturnPercent: number;
  reasons: string[];
  assumptions: string[];
  createdAt: string;
}

export type SafetyEventType = "blocked" | "warning" | "stopped" | "info";

export interface SafetyEvent {
  id: string;
  threadId?: string | null;
  eventType: SafetyEventType;
  message: string;
  createdAt: string;
}

export interface StrategyProfileInfo {
  profile: StrategyProfile;
  title: string;
  riskLabel: string;
  tradeFrequency: string;
  indicators: string[];
  summary: string;
}
