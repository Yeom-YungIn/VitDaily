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
  threadId?: string | null;
  executedAt: string;
  amountKrw: number;
  volumeBtc: number;
  status: "success" | "failure" | "blocked";
  errorMessage?: string;
  source?: PurchaseLogSource;
  mode?: ExecutionMode;
  action?: PurchaseLogAction;
  auditCategory?: AuditCategory;
  title?: string | null;
  reason?: string | null;
  safetyEventId?: string | null;
  strategySignalReason?: string | null;
}

export type PurchaseLogSource = "legacy_schedule" | "investment_thread" | "system";

export type ExecutionMode = "live" | "paper" | "system";

export type PurchaseLogAction = "market_buy" | "safety_check";

export type AuditCategory =
  | "trade"
  | "paper_trade"
  | "blocked_order"
  | "api_failure"
  | "safety_gate"
  | "validation"
  | "schedule";

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

export type PortfolioPointSource = "local" | "simulated";

export interface PortfolioTimePoint {
  date: string;
  investedKrw: number;
  estimatedValueKrw: number;
  returnPercent: number;
  drawdownPercent: number;
  source: PortfolioPointSource;
}

export interface PortfolioAllocation {
  market: SupportedMarket;
  budgetKrw: number;
  sharePercent: number;
}

export interface PortfolioSummary {
  totalBudgetKrw: number;
  investedKrw: number;
  currentValueKrw: number;
  returnPercent: number;
  maxDrawdownPercent: number;
  successfulBuys: number;
  blockedOrders: number;
  safetyEvents: number;
  latestPointSource?: PortfolioPointSource | null;
}

export interface ThreadAnalytics {
  threadId: string;
  threadName: string;
  market: SupportedMarket;
  budgetKrw: number;
  validationStatus: ValidationStatus;
  returnPercent?: number | null;
  maxDrawdownPercent?: number | null;
  baselineDcaReturnPercent?: number | null;
  simulatedTrades?: number | null;
  updatedAt: string;
}

export interface PortfolioAnalytics {
  summary: PortfolioSummary;
  timeSeries: PortfolioTimePoint[];
  allocations: PortfolioAllocation[];
  threads: ThreadAnalytics[];
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

export type LiveOrderFinalConfirmationStatus = "missing" | "confirmed";

export type LiveOrderGateSource = "legacy_schedule" | "investment_thread";

export type LiveOrderGateBlockReason =
  | "global_live_locked"
  | "final_confirmation_missing"
  | "live_mode_not_enabled"
  | "daily_trade_cap_exceeded"
  | "max_loss_exceeded"
  | "supported_market_required"
  | "validation_missing"
  | "validation_not_passed"
  | "legacy_schedule_not_migrated"
  | "settings_unavailable"
  | "audit_data_unavailable";

export interface LiveOrderGateCheck {
  source: LiveOrderGateSource;
  threadId?: string | null;
  relatedScheduleId?: string | null;
  market: SupportedMarket;
  amountKrw: number;
  finalConfirmationStatus: LiveOrderFinalConfirmationStatus;
  dailyTradeCount: number;
  dailyTradeCap: number;
  maxLossPercent?: number | null;
  latestMaxDrawdownPercent?: number | null;
  checkedAt: string;
}

export interface LiveOrderGateDecision {
  allowed: boolean;
  check: LiveOrderGateCheck;
  blockReasons: LiveOrderGateBlockReason[];
  reason: string;
}

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
  finalConfirmationStatus?: LiveOrderFinalConfirmationStatus;
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
  category?: AuditCategory;
  source?: string | null;
  relatedScheduleId?: string | null;
  reason?: string | null;
}

export interface StrategyProfileInfo {
  profile: StrategyProfile;
  title: string;
  riskLabel: string;
  tradeFrequency: string;
  indicators: string[];
  summary: string;
}
