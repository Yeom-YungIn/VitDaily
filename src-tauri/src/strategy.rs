use crate::types::{
    InvestmentThread, PaperSignalAction, StrategyProfile, StrategySignalEvaluation,
    SupportedMarket, ThreadValidationResult, ValidationStatus,
};
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

const DEFAULT_FEE_PERCENT: f64 = 0.05;
const BACKTEST_DAYS: u32 = 365;

#[derive(Debug, Clone)]
pub struct MarketCandle {
    pub timestamp: DateTime<Utc>,
    pub opening_price: f64,
    pub high_price: f64,
    pub low_price: f64,
    pub trade_price: f64,
}

#[derive(Debug, Clone, Copy)]
struct ProfileRules {
    max_trades_per_day: u32,
    slippage_percent: f64,
    atr_limit: f64,
    chandelier_lookback: usize,
    chandelier_multiplier: f64,
}

#[derive(Debug, Clone, Default)]
struct IndicatorRow {
    macd_line: Option<f64>,
    signal_line: Option<f64>,
    histogram: Option<f64>,
    bollinger_middle: Option<f64>,
    bollinger_upper: Option<f64>,
    bollinger_lower: Option<f64>,
    percent_b: Option<f64>,
    bandwidth: Option<f64>,
    atr14: Option<f64>,
    atr22: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TradeSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
struct SimTrade {
    side: TradeSide,
    timestamp: DateTime<Utc>,
    price: f64,
    reason: String,
}

#[derive(Debug, Clone)]
struct Simulation {
    return_percent: f64,
    max_drawdown_percent: f64,
    trades: Vec<SimTrade>,
    fees_krw: f64,
    max_loss_breached: bool,
    daily_cap_breached: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
struct UpbitMinuteCandle {
    candle_date_time_utc: String,
    opening_price: f64,
    high_price: f64,
    low_price: f64,
    trade_price: f64,
}

impl SupportedMarket {
    pub fn as_upbit_market(&self) -> &'static str {
        match self {
            SupportedMarket::KrwBtc => "KRW-BTC",
            SupportedMarket::KrwEth => "KRW-ETH",
            SupportedMarket::KrwXrp => "KRW-XRP",
        }
    }
}

pub async fn fetch_recent_year_hourly_candles(
    market: &SupportedMarket,
) -> Result<Vec<MarketCandle>, String> {
    fetch_hourly_candles(market, BACKTEST_DAYS, 50, 200).await
}

pub async fn fetch_recent_signal_hourly_candles(
    market: &SupportedMarket,
) -> Result<Vec<MarketCandle>, String> {
    fetch_hourly_candles(market, 30, 2, 200).await
}

async fn fetch_hourly_candles(
    market: &SupportedMarket,
    days: u32,
    max_batches: usize,
    count: usize,
) -> Result<Vec<MarketCandle>, String> {
    let client = reqwest::Client::new();
    let mut candles = Vec::new();
    let mut to: Option<String> = None;
    let cutoff = Utc::now() - Duration::days(days as i64);

    for _ in 0..max_batches {
        let mut request = client
            .get("https://api.upbit.com/v1/candles/minutes/60")
            .query(&[
                ("market", market.as_upbit_market().to_string()),
                ("count", count.to_string()),
            ]);
        if let Some(to_value) = to.as_ref() {
            request = request.query(&[("to", to_value.clone())]);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("업비트 캔들 조회 실패: HTTP {status} {body}"));
        }

        let batch = response
            .json::<Vec<UpbitMinuteCandle>>()
            .await
            .map_err(|e| e.to_string())?;
        if batch.is_empty() {
            break;
        }

        let before_len = candles.len();
        for item in batch {
            let candle = item.try_into_market_candle()?;
            if candle.timestamp < cutoff {
                continue;
            }
            candles.push(candle);
        }

        candles.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        candles.dedup_by_key(|candle| candle.timestamp);

        let Some(earliest) = candles.first() else {
            break;
        };
        if earliest.timestamp <= cutoff || candles.len() == before_len {
            break;
        }
        to = Some(earliest.timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }

    candles.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    if candles.len() < 200 {
        return Err(format!(
            "전략 평가에 필요한 캔들 데이터가 부족합니다: {}개",
            candles.len()
        ));
    }
    Ok(candles)
}

pub fn evaluate_latest_signal_for_thread(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
) -> Result<StrategySignalEvaluation, String> {
    validate_candles(candles)?;
    let rules = profile_rules(&thread.strategy_profile);
    let indicators = calculate_indicators(candles);
    let index = candles.len() - 1;
    let latest = &candles[index];
    let evaluated_at = Utc::now();

    let (action, reason) = if should_exit(thread, candles, &indicators, index, rules) {
        (
            PaperSignalAction::Sell,
            format!("Paper 평가: {}", exit_reason(&thread.strategy_profile)),
        )
    } else if should_enter(thread, candles, &indicators, index, rules) {
        (
            PaperSignalAction::Buy,
            format!("Paper 평가: {}", entry_reason(&thread.strategy_profile)),
        )
    } else {
        (
            PaperSignalAction::Hold,
            "Paper 평가: 진입/청산 조건이 충족되지 않아 대기합니다".to_string(),
        )
    };

    Ok(StrategySignalEvaluation {
        thread_id: thread.id,
        market: thread.market.clone(),
        strategy_profile: thread.strategy_profile.clone(),
        action,
        reason,
        evaluated_at,
        candle_timestamp: latest.timestamp,
        price_krw: latest.trade_price,
    })
}

impl UpbitMinuteCandle {
    fn try_into_market_candle(self) -> Result<MarketCandle, String> {
        let naive = NaiveDateTime::parse_from_str(&self.candle_date_time_utc, "%Y-%m-%dT%H:%M:%S")
            .map_err(|e| format!("캔들 시간 파싱 실패: {e}"))?;
        Ok(MarketCandle {
            timestamp: DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc),
            opening_price: self.opening_price,
            high_price: self.high_price,
            low_price: self.low_price,
            trade_price: self.trade_price,
        })
    }
}

pub fn run_backtest_for_thread(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
) -> Result<ThreadValidationResult, String> {
    validate_candles(candles)?;
    let rules = profile_rules(&thread.strategy_profile);
    let indicators = calculate_indicators(candles);
    let simulation = simulate_strategy(thread, candles, &indicators, rules, rules.slippage_percent);
    let doubled = simulate_strategy(
        thread,
        candles,
        &indicators,
        rules,
        rules.slippage_percent * 2.0,
    );
    let dca = simulate_dca(thread, candles, rules.slippage_percent);
    let buy_hold = simulate_buy_and_hold(thread, candles);
    let recent_start = candles.len().saturating_sub(90 * 24);
    let recent_strategy = simulate_strategy(
        thread,
        &candles[recent_start..],
        &calculate_indicators(&candles[recent_start..]),
        rules,
        rules.slippage_percent,
    );
    let recent_dca = simulate_dca(thread, &candles[recent_start..], rules.slippage_percent);
    let (status, reasons) = evaluate_validation(
        thread,
        &simulation,
        &dca,
        &buy_hold,
        &recent_strategy,
        &recent_dca,
        &doubled,
        rules,
    );

    Ok(ThreadValidationResult {
        id: Uuid::new_v4(),
        thread_id: thread.id,
        status,
        period_days: BACKTEST_DAYS,
        period_start: candles.first().expect("validated candles").timestamp,
        period_end: candles.last().expect("validated candles").timestamp,
        market: thread.market.clone(),
        strategy_profile: thread.strategy_profile.clone(),
        simulated_trades: simulation.trades.len() as u32,
        return_percent: round2(simulation.return_percent),
        max_drawdown_percent: round2(simulation.max_drawdown_percent),
        baseline_dca_return_percent: round2(dca.return_percent),
        baseline_dca_max_drawdown_percent: round2(dca.max_drawdown_percent),
        baseline_buy_hold_return_percent: round2(buy_hold.return_percent),
        baseline_buy_hold_max_drawdown_percent: round2(buy_hold.max_drawdown_percent),
        recent_90d_return_percent: round2(recent_strategy.return_percent),
        recent_90d_dca_return_percent: round2(recent_dca.return_percent),
        fees_krw: simulation.fees_krw.round().max(0.0) as u64,
        fee_percent: DEFAULT_FEE_PERCENT,
        slippage_percent: rules.slippage_percent,
        doubled_slippage_return_percent: round2(doubled.return_percent),
        reasons,
        assumptions: vec![
            "Upbit 공개 60분 캔들 기준".to_string(),
            "주문 전송 없이 순수 시뮬레이션만 수행".to_string(),
            format!("수수료 {}%/side fallback 적용", DEFAULT_FEE_PERCENT),
            format!("슬리피지 {}%/fill 적용", rules.slippage_percent),
            "백테스트 통과는 수익 보장이 아닌 실거래 검토 자격입니다".to_string(),
        ],
        created_at: Utc::now(),
    })
}

fn validate_candles(candles: &[MarketCandle]) -> Result<(), String> {
    if candles.len() < 200 {
        return Err("캔들 데이터가 부족합니다".to_string());
    }
    if candles.iter().any(|c| {
        c.opening_price <= 0.0 || c.high_price <= 0.0 || c.low_price <= 0.0 || c.trade_price <= 0.0
    }) {
        return Err("캔들 가격 데이터가 유효하지 않습니다".to_string());
    }
    Ok(())
}

fn profile_rules(profile: &StrategyProfile) -> ProfileRules {
    match profile {
        StrategyProfile::Stable => ProfileRules {
            max_trades_per_day: 2,
            slippage_percent: 0.05,
            atr_limit: 0.06,
            chandelier_lookback: 22,
            chandelier_multiplier: 3.5,
        },
        StrategyProfile::Conservative => ProfileRules {
            max_trades_per_day: 5,
            slippage_percent: 0.07,
            atr_limit: 0.08,
            chandelier_lookback: 22,
            chandelier_multiplier: 3.0,
        },
        StrategyProfile::Aggressive => ProfileRules {
            max_trades_per_day: 10,
            slippage_percent: 0.10,
            atr_limit: 0.12,
            chandelier_lookback: 14,
            chandelier_multiplier: 2.0,
        },
    }
}

fn calculate_indicators(candles: &[MarketCandle]) -> Vec<IndicatorRow> {
    let closes: Vec<f64> = candles.iter().map(|c| c.trade_price).collect();
    let ema12 = ema(&closes, 12);
    let ema26 = ema(&closes, 26);
    let macd_line: Vec<Option<f64>> = ema12
        .iter()
        .zip(ema26.iter())
        .map(|(fast, slow)| match (fast, slow) {
            (Some(f), Some(s)) => Some(f - s),
            _ => None,
        })
        .collect();
    let macd_values: Vec<f64> = macd_line.iter().map(|v| v.unwrap_or(0.0)).collect();
    let macd_signal = ema(&macd_values, 9);
    let atr14 = atr(candles, 14);
    let atr22 = atr(candles, 22);

    candles
        .iter()
        .enumerate()
        .map(|(index, _)| {
            let (middle, upper, lower, percent_b, bandwidth) = bollinger(&closes, index, 20, 2.0);
            let signal_line = if macd_line[index].is_some() {
                macd_signal[index]
            } else {
                None
            };
            IndicatorRow {
                macd_line: macd_line[index],
                signal_line,
                histogram: match (macd_line[index], signal_line) {
                    (Some(macd), Some(signal)) => Some(macd - signal),
                    _ => None,
                },
                bollinger_middle: middle,
                bollinger_upper: upper,
                bollinger_lower: lower,
                percent_b,
                bandwidth,
                atr14: atr14[index],
                atr22: atr22[index],
            }
        })
        .collect()
}

fn ema(values: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; values.len()];
    if values.len() < period {
        return result;
    }

    let mut current = values[..period].iter().sum::<f64>() / period as f64;
    result[period - 1] = Some(current);
    let multiplier = 2.0 / (period as f64 + 1.0);
    for index in period..values.len() {
        current = (values[index] - current) * multiplier + current;
        result[index] = Some(current);
    }
    result
}

fn bollinger(
    closes: &[f64],
    index: usize,
    period: usize,
    multiplier: f64,
) -> (
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
) {
    if index + 1 < period {
        return (None, None, None, None, None);
    }
    let window = &closes[index + 1 - period..=index];
    let mean = window.iter().sum::<f64>() / period as f64;
    let variance = window
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / period as f64;
    let deviation = variance.sqrt();
    let upper = mean + multiplier * deviation;
    let lower = mean - multiplier * deviation;
    let range = upper - lower;
    let percent_b = if range.abs() < f64::EPSILON {
        None
    } else {
        Some((closes[index] - lower) / range)
    };
    let bandwidth = if mean.abs() < f64::EPSILON {
        None
    } else {
        Some(range / mean)
    };
    (Some(mean), Some(upper), Some(lower), percent_b, bandwidth)
}

fn atr(candles: &[MarketCandle], period: usize) -> Vec<Option<f64>> {
    let mut true_ranges = Vec::with_capacity(candles.len());
    for (index, candle) in candles.iter().enumerate() {
        let previous_close = if index == 0 {
            candle.trade_price
        } else {
            candles[index - 1].trade_price
        };
        true_ranges.push(
            (candle.high_price - candle.low_price)
                .max((candle.high_price - previous_close).abs())
                .max((candle.low_price - previous_close).abs()),
        );
    }
    ema(&true_ranges, period)
}

fn simulate_strategy(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    rules: ProfileRules,
    slippage_percent: f64,
) -> Simulation {
    let mut cash = thread.initial_budget_krw as f64;
    let mut units = 0.0;
    let mut entry_value = thread.initial_budget_krw as f64;
    let mut peak_value = thread.initial_budget_krw as f64;
    let mut max_drawdown_percent: f64 = 0.0;
    let mut fees_krw = 0.0;
    let mut trades = Vec::new();
    let mut daily_trades: HashMap<(i32, u32, u32), u32> = HashMap::new();
    let mut max_loss_breached = false;
    let mut daily_cap_breached = false;

    for index in 30..candles.len() {
        let candle = &candles[index];
        let price = candle.trade_price;
        let portfolio_value = cash + units * price;
        peak_value = peak_value.max(portfolio_value);
        max_drawdown_percent =
            max_drawdown_percent.max(percent_loss(peak_value, portfolio_value).max(0.0));
        if percent_loss(entry_value, portfolio_value) >= thread.max_loss_percent {
            max_loss_breached = true;
            if units > 0.0 {
                let (proceeds, fee) = sell_value(units, price, slippage_percent);
                cash += proceeds;
                fees_krw += fee;
                units = 0.0;
                trades.push(SimTrade {
                    side: TradeSide::Sell,
                    timestamp: candle.timestamp,
                    price,
                    reason: "제품 최대 손실률 도달".to_string(),
                });
            }
            continue;
        }

        let key = (
            candle.timestamp.year(),
            candle.timestamp.month(),
            candle.timestamp.day(),
        );
        let today_count = *daily_trades.get(&key).unwrap_or(&0);
        if units <= f64::EPSILON {
            if today_count >= rules.max_trades_per_day || today_count >= thread.daily_trade_cap {
                continue;
            }
            if should_enter(thread, candles, indicators, index, rules) {
                let (bought_units, fee) = buy_units(cash, price, slippage_percent);
                units = bought_units;
                fees_krw += fee;
                cash = 0.0;
                entry_value = thread.initial_budget_krw as f64;
                let count = daily_trades.entry(key).or_insert(0);
                *count += 1;
                if *count > rules.max_trades_per_day || *count > thread.daily_trade_cap {
                    daily_cap_breached = true;
                }
                trades.push(SimTrade {
                    side: TradeSide::Buy,
                    timestamp: candle.timestamp,
                    price,
                    reason: entry_reason(&thread.strategy_profile),
                });
            }
        } else if should_exit(thread, candles, indicators, index, rules) {
            let (proceeds, fee) = sell_value(units, price, slippage_percent);
            cash += proceeds;
            fees_krw += fee;
            units = 0.0;
            let count = daily_trades.entry(key).or_insert(0);
            *count += 1;
            if *count > thread.daily_trade_cap {
                daily_cap_breached = true;
            }
            trades.push(SimTrade {
                side: TradeSide::Sell,
                timestamp: candle.timestamp,
                price,
                reason: exit_reason(&thread.strategy_profile),
            });
        }
    }

    if units > 0.0 {
        let last = candles.last().expect("validated candles");
        let (proceeds, fee) = sell_value(units, last.trade_price, slippage_percent);
        cash += proceeds;
        fees_krw += fee;
        trades.push(SimTrade {
            side: TradeSide::Sell,
            timestamp: last.timestamp,
            price: last.trade_price,
            reason: "백테스트 기간 종료".to_string(),
        });
    }

    let final_value = cash;
    Simulation {
        return_percent: percent_return(thread.initial_budget_krw as f64, final_value),
        max_drawdown_percent,
        trades,
        fees_krw,
        max_loss_breached,
        daily_cap_breached,
    }
}

fn should_enter(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
    rules: ProfileRules,
) -> bool {
    let row = &indicators[index];
    let atr_ratio = row.atr14.map(|atr| atr / candles[index].trade_price);
    if atr_ratio
        .map(|ratio| ratio > rules.atr_limit)
        .unwrap_or(true)
    {
        return false;
    }

    match thread.strategy_profile {
        StrategyProfile::Stable => {
            (histogram_rising(indicators, index, 2) || macd_above_signal(row))
                && (row.percent_b.is_some_and(|v| (0.20..=0.80).contains(&v))
                    || recovered_above_lower_band(candles, indicators, index))
        }
        StrategyProfile::Conservative => {
            (bullish_crossover(indicators, index, 3)
                || positive_histogram_rising(indicators, index))
                && (lower_band_recovery(candles, indicators, index)
                    || middle_cross_after_lower_touch(candles, indicators, index))
        }
        StrategyProfile::Aggressive => {
            macd_above_signal(row)
                && histogram_rising(indicators, index, 2)
                && row
                    .bollinger_upper
                    .is_some_and(|upper| candles[index].trade_price > upper)
                && (bandwidth_squeeze_breakout(indicators, index)
                    || bandwidth_expanding(indicators, index, 2))
        }
    }
}

fn should_exit(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
    rules: ProfileRules,
) -> bool {
    let row = &indicators[index];
    if close_below_chandelier(candles, indicators, index, rules) {
        return true;
    }

    match thread.strategy_profile {
        StrategyProfile::Stable => bearish_crossover_persistent(indicators, index, 2),
        StrategyProfile::Conservative => {
            bearish_crossover(indicators, index) && row.histogram.is_some_and(|h| h < 0.0)
                || row
                    .bollinger_upper
                    .is_some_and(|upper| candles[index].trade_price >= upper)
                || row.percent_b.is_some_and(|v| v >= 0.85)
                    && histogram_decreasing(indicators, index, 1)
        }
        StrategyProfile::Aggressive => {
            bearish_crossover(indicators, index)
                || row
                    .bollinger_upper
                    .is_some_and(|upper| candles[index].trade_price < upper)
                    && histogram_decreasing(indicators, index, 2)
        }
    }
}

fn buy_units(cash: f64, price: f64, slippage_percent: f64) -> (f64, f64) {
    let fee = cash * DEFAULT_FEE_PERCENT / 100.0;
    let fill_price = price * (1.0 + slippage_percent / 100.0);
    ((cash - fee).max(0.0) / fill_price, fee)
}

fn sell_value(units: f64, price: f64, slippage_percent: f64) -> (f64, f64) {
    let gross = units * price * (1.0 - slippage_percent / 100.0);
    let fee = gross * DEFAULT_FEE_PERCENT / 100.0;
    ((gross - fee).max(0.0), fee)
}

fn simulate_dca(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    slippage_percent: f64,
) -> Simulation {
    let days = thread.duration_days.max(1).min(BACKTEST_DAYS) as usize;
    let daily_cash = thread.initial_budget_krw as f64 / days as f64;
    let mut cash_remaining = thread.initial_budget_krw as f64;
    let mut units = 0.0;
    let mut invested_days = 0;
    let mut last_day: Option<(i32, u32, u32)> = None;
    let mut fees_krw = 0.0;
    let mut peak_value = thread.initial_budget_krw as f64;
    let mut max_drawdown_percent: f64 = 0.0;
    let mut trades = Vec::new();

    for candle in candles {
        let day = (
            candle.timestamp.year(),
            candle.timestamp.month(),
            candle.timestamp.day(),
        );
        if Some(day) != last_day && invested_days < days {
            let order_cash = daily_cash.min(cash_remaining);
            let (bought, fee) = buy_units(order_cash, candle.trade_price, slippage_percent);
            units += bought;
            cash_remaining -= order_cash;
            fees_krw += fee;
            invested_days += 1;
            last_day = Some(day);
            trades.push(SimTrade {
                side: TradeSide::Buy,
                timestamp: candle.timestamp,
                price: candle.trade_price,
                reason: "DCA 기준선 일일 진입".to_string(),
            });
        }
        let value = cash_remaining + units * candle.trade_price;
        peak_value = peak_value.max(value);
        max_drawdown_percent = max_drawdown_percent.max(percent_loss(peak_value, value));
    }

    let final_value = cash_remaining
        + units
            * candles.last().expect("validated candles").trade_price
            * (1.0 - slippage_percent / 100.0);
    Simulation {
        return_percent: percent_return(thread.initial_budget_krw as f64, final_value),
        max_drawdown_percent,
        trades,
        fees_krw,
        max_loss_breached: max_drawdown_percent > thread.max_loss_percent,
        daily_cap_breached: false,
    }
}

fn simulate_buy_and_hold(thread: &InvestmentThread, candles: &[MarketCandle]) -> Simulation {
    let first = candles.first().expect("validated candles");
    let (units, fee) = buy_units(thread.initial_budget_krw as f64, first.trade_price, 0.0);
    let mut peak_value = thread.initial_budget_krw as f64;
    let mut max_drawdown_percent: f64 = 0.0;
    for candle in candles {
        let value = units * candle.trade_price;
        peak_value = peak_value.max(value);
        max_drawdown_percent = max_drawdown_percent.max(percent_loss(peak_value, value));
    }
    let final_value = units * candles.last().expect("validated candles").trade_price;
    Simulation {
        return_percent: percent_return(thread.initial_budget_krw as f64, final_value),
        max_drawdown_percent,
        trades: vec![SimTrade {
            side: TradeSide::Buy,
            timestamp: first.timestamp,
            price: first.trade_price,
            reason: "Buy-and-hold 기준선 최초 진입".to_string(),
        }],
        fees_krw: fee,
        max_loss_breached: max_drawdown_percent > thread.max_loss_percent,
        daily_cap_breached: false,
    }
}

fn evaluate_validation(
    thread: &InvestmentThread,
    simulation: &Simulation,
    dca: &Simulation,
    buy_hold: &Simulation,
    recent_strategy: &Simulation,
    recent_dca: &Simulation,
    doubled: &Simulation,
    rules: ProfileRules,
) -> (ValidationStatus, Vec<String>) {
    let mut failures = Vec::new();
    let mut reasons = Vec::new();

    if simulation.max_loss_breached || simulation.max_drawdown_percent > thread.max_loss_percent {
        failures.push("제품 최대 손실률 기준을 초과했습니다".to_string());
    }
    if simulation.daily_cap_breached {
        failures.push(format!(
            "일일 거래 제한을 초과했습니다: 전략 목표 {}회/일, 스레드 제한 {}회/일",
            rules.max_trades_per_day, thread.daily_trade_cap
        ));
    }
    match thread.strategy_profile {
        StrategyProfile::Stable => {
            if simulation.return_percent < dca.return_percent - 3.0 {
                failures.push("DCA 기준선보다 3%p 이상 낮습니다".to_string());
            }
            if simulation.max_drawdown_percent > dca.max_drawdown_percent {
                failures.push("최대 낙폭이 DCA 기준선보다 큽니다".to_string());
            }
            if doubled.return_percent < -thread.max_loss_percent {
                failures.push("슬리피지 2배 민감도에서 최대 손실 기준을 벗어납니다".to_string());
            }
        }
        StrategyProfile::Conservative => {
            if simulation.return_percent < dca.return_percent {
                failures.push("DCA 기준선 수익률을 넘지 못했습니다".to_string());
            }
            if recent_strategy.return_percent < recent_dca.return_percent - 5.0 {
                failures.push("최근 90일 결과가 DCA 대비 5%p 이상 낮습니다".to_string());
            }
            if simulation.max_drawdown_percent > buy_hold.max_drawdown_percent {
                failures.push("최대 낙폭이 Buy-and-hold 기준선보다 큽니다".to_string());
            }
            if doubled.return_percent < -thread.max_loss_percent {
                failures.push("슬리피지 2배 민감도에서 최대 손실 기준을 벗어납니다".to_string());
            }
        }
        StrategyProfile::Aggressive => {
            if simulation.return_percent < dca.return_percent + 3.0 {
                failures.push("DCA 기준선보다 3%p 이상 높지 않습니다".to_string());
            }
            if recent_strategy.return_percent < recent_dca.return_percent - 7.0 {
                failures.push("최근 90일 결과가 DCA 대비 7%p 이상 낮습니다".to_string());
            }
            if doubled.return_percent < -thread.max_loss_percent {
                failures.push("슬리피지 2배 민감도에서 최대 손실 기준을 벗어납니다".to_string());
            }
        }
    }

    reasons.push(format!(
        "전략 수익률 {}%, DCA {}%, Buy-and-hold {}%",
        round2(simulation.return_percent),
        round2(dca.return_percent),
        round2(buy_hold.return_percent)
    ));
    reasons.push(format!(
        "최대 낙폭 {}%, 수수료 약 {}원, 체결 {}건",
        round2(simulation.max_drawdown_percent),
        simulation.fees_krw.round().max(0.0) as u64,
        simulation.trades.len()
    ));
    if let Some(last_trade) = simulation.trades.last() {
        let side = match last_trade.side {
            TradeSide::Buy => "매수",
            TradeSide::Sell => "매도",
        };
        reasons.push(format!(
            "마지막 신호: {} · {} · {:.0}원 · {}",
            last_trade.timestamp.format("%Y-%m-%d %H:%M UTC"),
            side,
            last_trade.price,
            last_trade.reason
        ));
    }
    if failures.is_empty() {
        reasons.push("검증 기준을 통과했지만 수익을 보장하지 않습니다".to_string());
        (ValidationStatus::Pass, reasons)
    } else {
        reasons.extend(failures);
        (ValidationStatus::Fail, reasons)
    }
}

fn macd_above_signal(row: &IndicatorRow) -> bool {
    match (row.macd_line, row.signal_line) {
        (Some(macd), Some(signal)) => macd >= signal,
        _ => false,
    }
}

fn bullish_crossover(indicators: &[IndicatorRow], index: usize, lookback: usize) -> bool {
    let start = index.saturating_sub(lookback);
    (start + 1..=index).any(|i| {
        match (
            indicators[i - 1].macd_line,
            indicators[i - 1].signal_line,
            indicators[i].macd_line,
            indicators[i].signal_line,
        ) {
            (Some(prev_macd), Some(prev_signal), Some(macd), Some(signal)) => {
                prev_macd <= prev_signal && macd > signal
            }
            _ => false,
        }
    })
}

fn bearish_crossover(indicators: &[IndicatorRow], index: usize) -> bool {
    if index == 0 {
        return false;
    }
    match (
        indicators[index - 1].macd_line,
        indicators[index - 1].signal_line,
        indicators[index].macd_line,
        indicators[index].signal_line,
    ) {
        (Some(prev_macd), Some(prev_signal), Some(macd), Some(signal)) => {
            prev_macd >= prev_signal && macd < signal
        }
        _ => false,
    }
}

fn bearish_crossover_persistent(indicators: &[IndicatorRow], index: usize, candles: usize) -> bool {
    bearish_crossover(indicators, index)
        && (0..candles).all(|offset| {
            index >= offset
                && indicators[index - offset]
                    .histogram
                    .is_some_and(|histogram| histogram < 0.0)
        })
}

fn histogram_rising(indicators: &[IndicatorRow], index: usize, candles: usize) -> bool {
    if index < candles {
        return false;
    }
    (0..candles).all(|offset| {
        let current = indicators[index - offset].histogram;
        let previous = indicators[index - offset - 1].histogram;
        matches!((current, previous), (Some(c), Some(p)) if c > p)
    })
}

fn histogram_decreasing(indicators: &[IndicatorRow], index: usize, candles: usize) -> bool {
    if index < candles {
        return false;
    }
    (0..candles).all(|offset| {
        let current = indicators[index - offset].histogram;
        let previous = indicators[index - offset - 1].histogram;
        matches!((current, previous), (Some(c), Some(p)) if c < p)
    })
}

fn positive_histogram_rising(indicators: &[IndicatorRow], index: usize) -> bool {
    indicators[index]
        .histogram
        .is_some_and(|histogram| histogram > 0.0)
        && histogram_rising(indicators, index, 1)
}

fn recovered_above_lower_band(
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
) -> bool {
    if index == 0 {
        return false;
    }
    indicators[index - 1]
        .percent_b
        .is_some_and(|value| value <= 0.20)
        && indicators[index]
            .bollinger_lower
            .is_some_and(|lower| candles[index].trade_price > lower)
}

fn lower_band_recovery(
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
) -> bool {
    indicators[index - 1]
        .percent_b
        .is_some_and(|value| value <= 0.25)
        && indicators[index]
            .bollinger_lower
            .is_some_and(|lower| candles[index].trade_price > lower)
}

fn middle_cross_after_lower_touch(
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
) -> bool {
    if index < 2 {
        return false;
    }
    let touched_lower_recently =
        (index - 2..index).any(|i| indicators[i].percent_b.is_some_and(|value| value <= 0.25));
    touched_lower_recently
        && indicators[index - 1]
            .bollinger_middle
            .is_some_and(|middle| candles[index - 1].trade_price <= middle)
        && indicators[index]
            .bollinger_middle
            .is_some_and(|middle| candles[index].trade_price > middle)
}

fn bandwidth_squeeze_breakout(indicators: &[IndicatorRow], index: usize) -> bool {
    if index < 120 {
        return false;
    }
    let current = indicators[index].bandwidth.unwrap_or(f64::MAX);
    let mut history: Vec<f64> = indicators[index - 120..index]
        .iter()
        .filter_map(|row| row.bandwidth)
        .collect();
    if history.len() < 60 {
        return false;
    }
    history.sort_by(|a, b| a.total_cmp(b));
    let threshold = history[history.len() / 4];
    let squeezed_recently = index.saturating_sub(10)..index;
    squeezed_recently
        .filter_map(|i| indicators[i].bandwidth)
        .any(|value| value <= threshold)
        && current > threshold
}

fn bandwidth_expanding(indicators: &[IndicatorRow], index: usize, candles: usize) -> bool {
    if index < candles {
        return false;
    }
    (0..candles).all(|offset| {
        let current = indicators[index - offset].bandwidth;
        let previous = indicators[index - offset - 1].bandwidth;
        matches!((current, previous), (Some(c), Some(p)) if c > p)
    })
}

fn close_below_chandelier(
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
    rules: ProfileRules,
) -> bool {
    if index + 1 < rules.chandelier_lookback {
        return false;
    }
    let atr = if rules.chandelier_lookback == 14 {
        indicators[index].atr14
    } else {
        indicators[index].atr22
    };
    let Some(atr) = atr else {
        return false;
    };
    let high = candles[index + 1 - rules.chandelier_lookback..=index]
        .iter()
        .map(|candle| candle.high_price)
        .fold(f64::MIN, f64::max);
    candles[index].trade_price < high - atr * rules.chandelier_multiplier
}

fn entry_reason(profile: &StrategyProfile) -> String {
    match profile {
        StrategyProfile::Stable => "MACD 약세 회피와 Bollinger/ATR 안전 필터 통과".to_string(),
        StrategyProfile::Conservative => "MACD 양호와 Bollinger 회복 조건 충족".to_string(),
        StrategyProfile::Aggressive => "MACD 모멘텀과 Bollinger 돌파 조건 충족".to_string(),
    }
}

fn exit_reason(profile: &StrategyProfile) -> String {
    match profile {
        StrategyProfile::Stable => "ATR 스톱 또는 MACD 약세 지속".to_string(),
        StrategyProfile::Conservative => "ATR 스톱, MACD 약세 또는 Bollinger 목표 도달".to_string(),
        StrategyProfile::Aggressive => "ATR 스톱, MACD 약세 또는 돌파 실패".to_string(),
    }
}

fn percent_return(initial: f64, final_value: f64) -> f64 {
    if initial <= 0.0 {
        0.0
    } else {
        (final_value - initial) / initial * 100.0
    }
}

fn percent_loss(initial: f64, current: f64) -> f64 {
    if initial <= 0.0 {
        0.0
    } else {
        (initial - current) / initial * 100.0
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn macd_detects_bullish_crossover_on_reversal() {
        let candles = synthetic_candles(260, |index| {
            if index < 130 {
                100.0 - index as f64 * 0.2
            } else {
                74.0 + (index - 130) as f64 * 0.8
            }
        });
        let indicators = calculate_indicators(&candles);

        let crossed = (40..indicators.len()).any(|index| bullish_crossover(&indicators, index, 3));

        assert!(crossed);
    }

    #[test]
    fn bollinger_percent_b_and_bandwidth_are_available_after_window() {
        let candles = synthetic_candles(40, |index| 100.0 + index as f64);
        let indicators = calculate_indicators(&candles);
        let row = &indicators[25];

        assert!(row.percent_b.is_some());
        assert!(row.bandwidth.is_some_and(|value| value > 0.0));
    }

    #[test]
    fn atr_chandelier_stop_triggers_on_large_drop() {
        let mut candles = synthetic_candles(80, |index| 100.0 + index as f64);
        candles[79].trade_price = 70.0;
        candles[79].low_price = 68.0;
        let indicators = calculate_indicators(&candles);
        let rules = profile_rules(&StrategyProfile::Conservative);

        assert!(close_below_chandelier(&candles, &indicators, 79, rules));
    }

    #[test]
    fn backtest_result_has_no_live_order_side_effect_and_reports_assumptions() {
        let thread = sample_thread();
        let candles = synthetic_candles(420, |index| 100.0 + (index as f64 / 8.0).sin() * 5.0);

        let result = run_backtest_for_thread(&thread, &candles).expect("backtest");

        assert_eq!(result.thread_id, thread.id);
        assert!(matches!(
            result.status,
            ValidationStatus::Pass | ValidationStatus::Fail
        ));
        assert!(result
            .assumptions
            .iter()
            .any(|assumption| assumption.contains("주문 전송 없이")));
    }

    #[test]
    fn product_max_loss_forces_validation_failure() {
        let mut thread = sample_thread();
        thread.max_loss_percent = 1.0;
        let losing = Simulation {
            return_percent: -3.0,
            max_drawdown_percent: 2.0,
            trades: Vec::new(),
            fees_krw: 0.0,
            max_loss_breached: true,
            daily_cap_breached: false,
        };
        let baseline = Simulation {
            return_percent: -1.0,
            max_drawdown_percent: 1.0,
            trades: Vec::new(),
            fees_krw: 0.0,
            max_loss_breached: false,
            daily_cap_breached: false,
        };

        let (status, reasons) = evaluate_validation(
            &thread,
            &losing,
            &baseline,
            &baseline,
            &losing,
            &baseline,
            &losing,
            profile_rules(&thread.strategy_profile),
        );

        assert_eq!(status, ValidationStatus::Fail);
        assert!(reasons.iter().any(|reason| reason.contains("손실률")));
    }

    fn synthetic_candles(count: usize, close: impl Fn(usize) -> f64) -> Vec<MarketCandle> {
        let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        (0..count)
            .map(|index| {
                let trade_price = close(index).max(1.0);
                MarketCandle {
                    timestamp: start + Duration::hours(index as i64),
                    opening_price: trade_price,
                    high_price: trade_price * 1.01,
                    low_price: trade_price * 0.99,
                    trade_price,
                }
            })
            .collect()
    }

    fn sample_thread() -> InvestmentThread {
        let now = Utc::now();
        InvestmentThread {
            id: Uuid::new_v4(),
            name: "백테스트 스레드".to_string(),
            market: SupportedMarket::KrwBtc,
            initial_budget_krw: 100_000,
            duration_days: 30,
            strategy_profile: StrategyProfile::Conservative,
            max_loss_percent: 50.0,
            daily_trade_cap: 10,
            status: crate::types::ThreadStatus::Draft,
            validation_status: ValidationStatus::Missing,
            final_confirmation_status: crate::types::LiveOrderFinalConfirmationStatus::Missing,
            created_at: now,
            updated_at: now,
        }
    }
}
