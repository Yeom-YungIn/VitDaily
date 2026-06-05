use crate::types::{
    InvestmentThread, PaperSignalAction, StrategyProfile, StrategySignalEvaluation,
    SupportedMarket, ThreadValidationResult, ValidationStatus,
};
use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Utc};
use reqwest::header::HeaderMap;
use serde::Deserialize;
use std::collections::HashMap;
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

const DEFAULT_FEE_PERCENT: f64 = 0.05;
pub const STRATEGY_VERSION_INTRADAY_MEAN_REVERSION: &str = "intraday_mean_reversion_v1";
const UPBIT_CANDLE_BACKTEST_REQUEST_INTERVAL_MS: u64 = 250;
const UPBIT_CANDLE_SIGNAL_REQUEST_INTERVAL_MS: u64 = 120;
const UPBIT_RATE_LIMIT_RESET_MS: u64 = 1_100;
const UPBIT_CANDLE_MAX_RETRIES: usize = 4;

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
    max_round_trips_per_day: u32,
    min_round_trips_per_year: u32,
    profit_factor_threshold: f64,
    exposure_limit_percent: f64,
    average_hold_limit_hours: f64,
    recent_90d_tolerance_pp: f64,
    max_hold_hours: f64,
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
    pnl_krw: Option<f64>,
    hold_hours: Option<f64>,
}

#[derive(Debug, Clone)]
struct Simulation {
    return_percent: f64,
    max_drawdown_percent: f64,
    trades: Vec<SimTrade>,
    fees_krw: f64,
    cost_drag_krw: f64,
    round_trips: u32,
    win_rate_percent: f64,
    profit_factor: f64,
    expectancy_krw: f64,
    average_hold_hours: f64,
    exposure_percent: f64,
    max_loss_breached: bool,
    daily_cap_breached: bool,
    stop_exit_count: u32,
    time_exit_count: u32,
    day_flat_exit_count: u32,
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

pub async fn fetch_backtest_hourly_candles(
    market: &SupportedMarket,
    days: u32,
) -> Result<Vec<MarketCandle>, String> {
    let requested_days = days.max(1);
    let target_candles = requested_days as usize * 24;
    let max_batches = ((target_candles.saturating_add(199)) / 200).saturating_add(2);
    fetch_hourly_candles(
        market,
        requested_days,
        max_batches.max(2),
        200,
        TokioDuration::from_millis(UPBIT_CANDLE_BACKTEST_REQUEST_INTERVAL_MS),
    )
    .await
}

pub async fn fetch_recent_signal_hourly_candles(
    market: &SupportedMarket,
) -> Result<Vec<MarketCandle>, String> {
    fetch_hourly_candles(
        market,
        30,
        2,
        200,
        TokioDuration::from_millis(UPBIT_CANDLE_SIGNAL_REQUEST_INTERVAL_MS),
    )
    .await
}

async fn fetch_hourly_candles(
    market: &SupportedMarket,
    days: u32,
    max_batches: usize,
    count: usize,
    request_interval: TokioDuration,
) -> Result<Vec<MarketCandle>, String> {
    let client = reqwest::Client::new();
    let mut candles = Vec::new();
    let mut to: Option<String> = None;
    let cutoff = Utc::now() - Duration::days(days as i64);

    for batch_index in 0..max_batches {
        if batch_index > 0 {
            sleep(request_interval).await;
        }

        let (batch, remaining_sec) =
            fetch_upbit_hourly_candle_batch(&client, market, count, to.as_deref()).await?;
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

        if remaining_sec == Some(0) {
            sleep(TokioDuration::from_millis(UPBIT_RATE_LIMIT_RESET_MS)).await;
        }
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

async fn fetch_upbit_hourly_candle_batch(
    client: &reqwest::Client,
    market: &SupportedMarket,
    count: usize,
    to: Option<&str>,
) -> Result<(Vec<UpbitMinuteCandle>, Option<u32>), String> {
    let mut retry_delay = TokioDuration::from_millis(UPBIT_RATE_LIMIT_RESET_MS);

    for attempt in 0..=UPBIT_CANDLE_MAX_RETRIES {
        let mut request = client
            .get("https://api.upbit.com/v1/candles/minutes/60")
            .query(&[
                ("market", market.as_upbit_market().to_string()),
                ("count", count.to_string()),
            ]);
        if let Some(to_value) = to {
            request = request.query(&[("to", to_value.to_string())]);
        }

        let response = request.send().await.map_err(|e| e.to_string())?;
        let status = response.status();
        let remaining_sec = upbit_remaining_req_sec(response.headers());
        if status.is_success() {
            let batch = response
                .json::<Vec<UpbitMinuteCandle>>()
                .await
                .map_err(|e| e.to_string())?;
            return Ok((batch, remaining_sec));
        }

        let status_code = status.as_u16();
        let body = response.text().await.unwrap_or_default();
        if status_code == 429 && attempt < UPBIT_CANDLE_MAX_RETRIES {
            sleep(retry_delay).await;
            retry_delay *= 2;
            continue;
        }

        return Err(format!("업비트 캔들 조회 실패: HTTP {status_code} {body}"));
    }

    Err("업비트 캔들 조회 실패: 요청 제한 재시도를 모두 사용했습니다".to_string())
}

fn upbit_remaining_req_sec(headers: &HeaderMap) -> Option<u32> {
    headers
        .get("Remaining-Req")
        .and_then(|header| header.to_str().ok())
        .and_then(parse_upbit_remaining_req_sec)
}

fn parse_upbit_remaining_req_sec(header: &str) -> Option<u32> {
    header
        .split(';')
        .map(str::trim)
        .find_map(|part| part.strip_prefix("sec="))
        .and_then(|value| value.parse::<u32>().ok())
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

    let exit_candidate = signal_exit_reason(thread, candles, &indicators, index, rules);
    let (action, reason, exit_reason) = if let Some(reason) = exit_candidate {
        (
            PaperSignalAction::Sell,
            format!("Paper 평가: {}", reason),
            Some(reason),
        )
    } else if should_enter(thread, candles, &indicators, index, rules) {
        (
            PaperSignalAction::Buy,
            format!("Paper 평가: {}", entry_reason(&thread.strategy_profile)),
            None,
        )
    } else {
        (
            PaperSignalAction::Hold,
            "Paper 평가: 진입/청산 조건이 충족되지 않아 대기합니다".to_string(),
            None,
        )
    };

    Ok(StrategySignalEvaluation {
        thread_id: thread.id,
        market: thread.market.clone(),
        strategy_profile: thread.strategy_profile.clone(),
        strategy_version: STRATEGY_VERSION_INTRADAY_MEAN_REVERSION.to_string(),
        action,
        reason,
        exit_reason,
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
    let period_days = thread.duration_days.max(1);
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
    let recent_window_days = period_days.min(90);
    let recent_start = candles
        .len()
        .saturating_sub(recent_window_days as usize * 24);
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
        period_days,
    );

    Ok(ThreadValidationResult {
        id: Uuid::new_v4(),
        thread_id: thread.id,
        strategy_version: STRATEGY_VERSION_INTRADAY_MEAN_REVERSION.to_string(),
        strategy_variant_label: strategy_variant_label(&thread.strategy_profile).to_string(),
        status,
        period_days,
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
        cost_drag_krw: simulation.cost_drag_krw.round().max(0.0) as u64,
        fee_percent: DEFAULT_FEE_PERCENT,
        slippage_percent: rules.slippage_percent,
        doubled_slippage_return_percent: round2(doubled.return_percent),
        round_trips: simulation.round_trips,
        win_rate_percent: round2(simulation.win_rate_percent),
        profit_factor: round2(simulation.profit_factor),
        expectancy_krw: round2(simulation.expectancy_krw),
        average_hold_hours: round2(simulation.average_hold_hours),
        exposure_percent: round2(simulation.exposure_percent),
        cash_flat_return_percent: 0.0,
        stop_exit_count: simulation.stop_exit_count,
        time_exit_count: simulation.time_exit_count,
        day_flat_exit_count: simulation.day_flat_exit_count,
        reasons,
        assumptions: vec![
            "Upbit 공개 60분 캔들 기준".to_string(),
            "주문 전송 없이 순수 시뮬레이션만 수행".to_string(),
            format!(
                "{}: 하루 단위 반복 매수/매도 평균회귀 전략",
                STRATEGY_VERSION_INTRADAY_MEAN_REVERSION
            ),
            format!("수수료 {}%/side fallback 적용", DEFAULT_FEE_PERCENT),
            format!("슬리피지 {}%/fill 적용", rules.slippage_percent),
            format!("백테스트 기간은 투자 기간 {}일 기준", period_days),
            "DCA와 Buy-and-hold는 참고 지표이며 PASS gate가 아닙니다".to_string(),
            "백테스트 통과는 수익 보장이 아닌 모의 검증 자격입니다".to_string(),
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
            max_round_trips_per_day: 1,
            min_round_trips_per_year: 10,
            profit_factor_threshold: 1.10,
            exposure_limit_percent: 20.0,
            average_hold_limit_hours: 8.0,
            recent_90d_tolerance_pp: 8.0,
            max_hold_hours: 8.0,
            slippage_percent: 0.05,
            atr_limit: 0.06,
            chandelier_lookback: 22,
            chandelier_multiplier: 3.5,
        },
        StrategyProfile::Conservative => ProfileRules {
            max_round_trips_per_day: 2,
            min_round_trips_per_year: 15,
            profit_factor_threshold: 1.15,
            exposure_limit_percent: 35.0,
            average_hold_limit_hours: 12.0,
            recent_90d_tolerance_pp: 10.0,
            max_hold_hours: 12.0,
            slippage_percent: 0.07,
            atr_limit: 0.08,
            chandelier_lookback: 22,
            chandelier_multiplier: 3.0,
        },
        StrategyProfile::Aggressive => ProfileRules {
            max_round_trips_per_day: 4,
            min_round_trips_per_year: 20,
            profit_factor_threshold: 1.20,
            exposure_limit_percent: 50.0,
            average_hold_limit_hours: 18.0,
            recent_90d_tolerance_pp: 12.0,
            max_hold_hours: 18.0,
            slippage_percent: 0.10,
            atr_limit: 0.12,
            chandelier_lookback: 14,
            chandelier_multiplier: 2.0,
        },
    }
}

fn strategy_variant_label(profile: &StrategyProfile) -> &'static str {
    match profile {
        StrategyProfile::Stable => "Stable mean reversion",
        StrategyProfile::Conservative => "Conservative mean reversion",
        StrategyProfile::Aggressive => "Aggressive mean reversion",
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
    let mut entry_cash = 0.0;
    let mut entry_timestamp: Option<DateTime<Utc>> = None;
    let mut peak_value = thread.initial_budget_krw as f64;
    let mut max_drawdown_percent: f64 = 0.0;
    let mut fees_krw = 0.0;
    let mut cost_drag_krw = 0.0;
    let mut trades = Vec::new();
    let mut daily_trade_events: HashMap<(i32, u32, u32), u32> = HashMap::new();
    let mut daily_round_trips: HashMap<(i32, u32, u32), u32> = HashMap::new();
    let mut max_loss_breached = false;
    let mut daily_cap_breached = false;
    let mut stop_exit_count = 0;
    let mut time_exit_count = 0;
    let mut day_flat_exit_count = 0;
    let mut round_trip_pnls = Vec::new();
    let mut hold_hours_total = 0.0;

    for index in 30..candles.len() {
        let candle = &candles[index];
        let price = candle.trade_price;
        let portfolio_value = cash + units * price;
        peak_value = peak_value.max(portfolio_value);
        max_drawdown_percent =
            max_drawdown_percent.max(percent_loss(peak_value, portfolio_value).max(0.0));
        if percent_loss(thread.initial_budget_krw as f64, portfolio_value)
            >= thread.max_loss_percent
        {
            max_loss_breached = true;
            if units > 0.0 {
                close_position(
                    &mut cash,
                    &mut units,
                    &mut entry_cash,
                    &mut entry_timestamp,
                    candle,
                    slippage_percent,
                    "제품 최대 손실률 도달".to_string(),
                    &mut fees_krw,
                    &mut cost_drag_krw,
                    &mut trades,
                    &mut daily_trade_events,
                    &mut daily_round_trips,
                    &mut round_trip_pnls,
                    &mut hold_hours_total,
                );
                stop_exit_count += 1;
            }
            continue;
        }

        let key = (
            candle.timestamp.year(),
            candle.timestamp.month(),
            candle.timestamp.day(),
        );

        if units > f64::EPSILON {
            let reason = exit_reason_for_position(
                thread,
                candles,
                indicators,
                index,
                rules,
                entry_timestamp.expect("entry timestamp exists while position is open"),
            );
            if let Some(reason) = reason {
                if reason.contains("일 단위") {
                    day_flat_exit_count += 1;
                } else if reason.contains("보유 시간") {
                    time_exit_count += 1;
                } else if reason.contains("스톱") || reason.contains("손실률") {
                    stop_exit_count += 1;
                }
                close_position(
                    &mut cash,
                    &mut units,
                    &mut entry_cash,
                    &mut entry_timestamp,
                    candle,
                    slippage_percent,
                    reason,
                    &mut fees_krw,
                    &mut cost_drag_krw,
                    &mut trades,
                    &mut daily_trade_events,
                    &mut daily_round_trips,
                    &mut round_trip_pnls,
                    &mut hold_hours_total,
                );
            }
        }

        if units <= f64::EPSILON {
            let today_events = *daily_trade_events.get(&key).unwrap_or(&0);
            let today_round_trips = *daily_round_trips.get(&key).unwrap_or(&0);
            let event_cap = thread
                .daily_trade_cap
                .min(rules.max_round_trips_per_day.saturating_mul(2));
            if today_events >= event_cap || today_round_trips >= rules.max_round_trips_per_day {
                continue;
            }
            if should_enter(thread, candles, indicators, index, rules) {
                let (bought_units, fee) = buy_units(cash, price, slippage_percent);
                units = bought_units;
                fees_krw += fee;
                cost_drag_krw += fee + cash * slippage_percent / 100.0;
                entry_cash = cash;
                entry_timestamp = Some(candle.timestamp);
                cash = 0.0;
                let count = daily_trade_events.entry(key).or_insert(0);
                *count += 1;
                if *count > event_cap {
                    daily_cap_breached = true;
                }
                trades.push(SimTrade {
                    side: TradeSide::Buy,
                    timestamp: candle.timestamp,
                    price,
                    reason: entry_reason(&thread.strategy_profile),
                    pnl_krw: None,
                    hold_hours: None,
                });
            }
        }
    }

    if units > 0.0 {
        let last = candles.last().expect("validated candles");
        close_position(
            &mut cash,
            &mut units,
            &mut entry_cash,
            &mut entry_timestamp,
            last,
            slippage_percent,
            "백테스트 기간 종료".to_string(),
            &mut fees_krw,
            &mut cost_drag_krw,
            &mut trades,
            &mut daily_trade_events,
            &mut daily_round_trips,
            &mut round_trip_pnls,
            &mut hold_hours_total,
        );
    }

    let final_value = cash;
    let round_trips = round_trip_pnls.len() as u32;
    let wins = round_trip_pnls.iter().filter(|pnl| **pnl > 0.0).count() as f64;
    let gross_profit: f64 = round_trip_pnls.iter().filter(|pnl| **pnl > 0.0).sum();
    let gross_loss: f64 = round_trip_pnls
        .iter()
        .filter(|pnl| **pnl < 0.0)
        .map(|pnl| pnl.abs())
        .sum();
    let profit_factor = if gross_loss <= f64::EPSILON {
        if gross_profit > 0.0 {
            gross_profit
        } else {
            0.0
        }
    } else {
        gross_profit / gross_loss
    };
    let expectancy_krw = if round_trips == 0 {
        0.0
    } else {
        round_trip_pnls.iter().sum::<f64>() / round_trips as f64
    };
    let total_period_hours = candles
        .first()
        .zip(candles.last())
        .map(|(first, last)| {
            (last.timestamp - first.timestamp).num_seconds().max(1) as f64 / 3600.0
        })
        .unwrap_or(1.0);

    Simulation {
        return_percent: percent_return(thread.initial_budget_krw as f64, final_value),
        max_drawdown_percent,
        trades,
        fees_krw,
        cost_drag_krw,
        round_trips,
        win_rate_percent: if round_trips == 0 {
            0.0
        } else {
            wins / round_trips as f64 * 100.0
        },
        profit_factor,
        expectancy_krw,
        average_hold_hours: if round_trips == 0 {
            0.0
        } else {
            hold_hours_total / round_trips as f64
        },
        exposure_percent: (hold_hours_total / total_period_hours * 100.0).min(100.0),
        max_loss_breached,
        daily_cap_breached,
        stop_exit_count,
        time_exit_count,
        day_flat_exit_count,
    }
}

#[allow(clippy::too_many_arguments)]
fn close_position(
    cash: &mut f64,
    units: &mut f64,
    entry_cash: &mut f64,
    entry_timestamp: &mut Option<DateTime<Utc>>,
    candle: &MarketCandle,
    slippage_percent: f64,
    reason: String,
    fees_krw: &mut f64,
    cost_drag_krw: &mut f64,
    trades: &mut Vec<SimTrade>,
    daily_trade_events: &mut HashMap<(i32, u32, u32), u32>,
    daily_round_trips: &mut HashMap<(i32, u32, u32), u32>,
    round_trip_pnls: &mut Vec<f64>,
    hold_hours_total: &mut f64,
) {
    let open_units = *units;
    let (proceeds, fee) = sell_value(open_units, candle.trade_price, slippage_percent);
    let pnl_krw = proceeds - *entry_cash;
    let hold_hours = entry_timestamp
        .map(|entered_at| (candle.timestamp - entered_at).num_seconds().max(0) as f64 / 3600.0)
        .unwrap_or(0.0);
    let key = (
        candle.timestamp.year(),
        candle.timestamp.month(),
        candle.timestamp.day(),
    );

    *cash += proceeds;
    *fees_krw += fee;
    *cost_drag_krw += fee + open_units * candle.trade_price * slippage_percent / 100.0;
    *units = 0.0;
    *entry_cash = 0.0;
    *entry_timestamp = None;
    *daily_trade_events.entry(key).or_insert(0) += 1;
    *daily_round_trips.entry(key).or_insert(0) += 1;
    round_trip_pnls.push(pnl_krw);
    *hold_hours_total += hold_hours;
    trades.push(SimTrade {
        side: TradeSide::Sell,
        timestamp: candle.timestamp,
        price: candle.trade_price,
        reason,
        pnl_krw: Some(pnl_krw),
        hold_hours: Some(hold_hours),
    });
}

fn exit_reason_for_position(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
    rules: ProfileRules,
    entry_timestamp: DateTime<Utc>,
) -> Option<String> {
    let candle = &candles[index];
    if candle.timestamp.date_naive() != entry_timestamp.date_naive() {
        return Some("일 단위 포지션 정리".to_string());
    }

    let hold_hours = (candle.timestamp - entry_timestamp).num_seconds().max(0) as f64 / 3600.0;
    if hold_hours >= rules.max_hold_hours {
        return Some(format!(
            "보유 시간 제한 {:.0}시간 도달",
            rules.max_hold_hours
        ));
    }

    signal_exit_reason(thread, candles, indicators, index, rules)
}

fn signal_exit_reason(
    thread: &InvestmentThread,
    candles: &[MarketCandle],
    indicators: &[IndicatorRow],
    index: usize,
    rules: ProfileRules,
) -> Option<String> {
    if close_below_chandelier(candles, indicators, index, rules) {
        return Some("ATR 스톱 도달".to_string());
    }

    let row = &indicators[index];
    let price = candles[index].trade_price;
    match thread.strategy_profile {
        StrategyProfile::Stable => {
            if row.bollinger_middle.is_some_and(|middle| price >= middle) {
                Some("Bollinger 중단 목표 도달".to_string())
            } else if bearish_crossover_persistent(indicators, index, 2) {
                Some("MACD 약세 전환".to_string())
            } else {
                None
            }
        }
        StrategyProfile::Conservative => {
            if row.percent_b.is_some_and(|value| value >= 0.80)
                || row.bollinger_middle.is_some_and(|middle| price >= middle)
            {
                Some("평균회귀 목표 구간 도달".to_string())
            } else if bearish_crossover(indicators, index) && row.histogram.is_some_and(|h| h < 0.0)
            {
                Some("MACD 약세 전환".to_string())
            } else {
                None
            }
        }
        StrategyProfile::Aggressive => {
            if row.percent_b.is_some_and(|value| value >= 0.90)
                || row.bollinger_upper.is_some_and(|upper| price >= upper)
            {
                Some("상단 밴드 반등 목표 도달".to_string())
            } else if bearish_crossover(indicators, index) {
                Some("MACD 약세 전환".to_string())
            } else {
                None
            }
        }
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

    let volatility_ok = row.bandwidth.is_some_and(|value| value <= 0.35);

    match thread.strategy_profile {
        StrategyProfile::Stable => {
            volatility_ok
                && (histogram_rising(indicators, index, 1) || macd_above_signal(row))
                && (row.percent_b.is_some_and(|v| (0.15..=0.55).contains(&v))
                    || recovered_above_lower_band(candles, indicators, index))
        }
        StrategyProfile::Conservative => {
            volatility_ok
                && (bullish_crossover(indicators, index, 3)
                    || positive_histogram_rising(indicators, index))
                && (lower_band_recovery(candles, indicators, index)
                    || row.percent_b.is_some_and(|v| (0.10..=0.45).contains(&v)))
        }
        StrategyProfile::Aggressive => {
            row.bandwidth.is_some_and(|value| value <= 0.50)
                && (histogram_rising(indicators, index, 1) || macd_above_signal(row))
                && (lower_band_recovery(candles, indicators, index)
                    || row.percent_b.is_some_and(|v| v <= 0.25))
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
    let days = thread.duration_days.max(1) as usize;
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
                pnl_krw: None,
                hold_hours: None,
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
        cost_drag_krw: fees_krw,
        round_trips: 0,
        win_rate_percent: 0.0,
        profit_factor: 0.0,
        expectancy_krw: 0.0,
        average_hold_hours: 0.0,
        exposure_percent: 100.0,
        max_loss_breached: max_drawdown_percent > thread.max_loss_percent,
        daily_cap_breached: false,
        stop_exit_count: 0,
        time_exit_count: 0,
        day_flat_exit_count: 0,
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
            pnl_krw: None,
            hold_hours: None,
        }],
        fees_krw: fee,
        cost_drag_krw: fee,
        round_trips: 0,
        win_rate_percent: 0.0,
        profit_factor: 0.0,
        expectancy_krw: 0.0,
        average_hold_hours: 0.0,
        exposure_percent: 100.0,
        max_loss_breached: max_drawdown_percent > thread.max_loss_percent,
        daily_cap_breached: false,
        stop_exit_count: 0,
        time_exit_count: 0,
        day_flat_exit_count: 0,
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
    period_days: u32,
) -> (ValidationStatus, Vec<String>) {
    let mut failures = Vec::new();
    let mut reasons = Vec::new();
    let min_round_trips_for_period = min_round_trips_for_period(rules, period_days);

    if simulation.max_loss_breached || simulation.max_drawdown_percent > thread.max_loss_percent {
        failures.push("제품 최대 손실률 기준을 초과했습니다".to_string());
    }
    if simulation.daily_cap_breached {
        failures.push(format!(
            "일일 거래 제한을 초과했습니다: 전략 목표 {} round-trip/일, 스레드 제한 {} 이벤트/일",
            rules.max_round_trips_per_day, thread.daily_trade_cap
        ));
    }

    if simulation.return_percent <= 0.0 {
        failures.push("현금 대기 기준선(0%)을 넘는 순수익을 만들지 못했습니다".to_string());
    }
    if simulation.expectancy_krw <= 0.0 {
        failures.push("round-trip 기대값이 양수로 검증되지 않았습니다".to_string());
    }
    if simulation.round_trips < min_round_trips_for_period {
        failures.push(format!(
            "{}일 completed round-trip 수가 부족합니다: {}회 < 최소 {}회",
            period_days, simulation.round_trips, min_round_trips_for_period
        ));
    }
    if simulation.profit_factor < rules.profit_factor_threshold {
        failures.push(format!(
            "profit factor가 기준 미달입니다: {:.2} < {:.2}",
            simulation.profit_factor, rules.profit_factor_threshold
        ));
    }
    if doubled.return_percent < -thread.max_loss_percent {
        failures.push("슬리피지 2배 민감도에서 최대 손실 기준을 벗어납니다".to_string());
    }
    if simulation.exposure_percent > rules.exposure_limit_percent {
        failures.push(format!(
            "시장 노출 시간이 기준을 초과했습니다: {:.2}% > {:.2}%",
            simulation.exposure_percent, rules.exposure_limit_percent
        ));
    }
    if simulation.average_hold_hours > rules.average_hold_limit_hours {
        failures.push(format!(
            "평균 보유 시간이 기준을 초과했습니다: {:.2}h > {:.2}h",
            simulation.average_hold_hours, rules.average_hold_limit_hours
        ));
    }
    if recent_strategy.return_percent < simulation.return_percent - rules.recent_90d_tolerance_pp {
        failures.push(format!(
            "최근 90일 수익률이 전체 결과 대비 허용 범위를 벗어납니다: {:.2}% < {:.2}%",
            recent_strategy.return_percent,
            simulation.return_percent - rules.recent_90d_tolerance_pp
        ));
    }

    reasons.push(format!(
        "전략 수익률 {}%, 현금 대기 0%, DCA 참고 {}%, Buy-and-hold 참고 {}%",
        round2(simulation.return_percent),
        round2(dca.return_percent),
        round2(buy_hold.return_percent)
    ));
    reasons.push(format!(
        "round-trip {}회, 기간 기준 최소 {}회, 승률 {}%, PF {}, 기대값 {}원",
        simulation.round_trips,
        min_round_trips_for_period,
        round2(simulation.win_rate_percent),
        round2(simulation.profit_factor),
        simulation.expectancy_krw.round() as i64
    ));
    reasons.push(format!(
        "노출 {}%, 평균 보유 {}h, 최대 낙폭 {}%, 비용 약 {}원, 체결 {}건",
        round2(simulation.exposure_percent),
        round2(simulation.average_hold_hours),
        round2(simulation.max_drawdown_percent),
        simulation.cost_drag_krw.round().max(0.0) as u64,
        simulation.trades.len()
    ));
    reasons.push(format!(
        "DCA/Buy-and-hold는 방향성 노출 참고값이며 PASS gate가 아닙니다 · 최근 90일 DCA 참고 {}%",
        round2(recent_dca.return_percent)
    ));
    if let Some(last_trade) = simulation.trades.last() {
        let side = match last_trade.side {
            TradeSide::Buy => "매수",
            TradeSide::Sell => "매도",
        };
        let outcome = last_trade
            .pnl_krw
            .map(|pnl| format!(" · P/L {}원", pnl.round() as i64))
            .unwrap_or_default();
        let hold = last_trade
            .hold_hours
            .map(|hours| format!(" · 보유 {:.1}h", hours))
            .unwrap_or_default();
        reasons.push(format!(
            "마지막 신호: {} · {} · {:.0}원 · {}{}{}",
            last_trade.timestamp.format("%Y-%m-%d %H:%M UTC"),
            side,
            last_trade.price,
            last_trade.reason,
            outcome,
            hold
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

fn min_round_trips_for_period(rules: ProfileRules, period_days: u32) -> u32 {
    ((rules.min_round_trips_per_year as f64 * period_days.max(1) as f64) / 365.0)
        .ceil()
        .max(1.0) as u32
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
        let losing = test_simulation(-3.0, 2.0, true);
        let baseline = test_simulation(-1.0, 1.0, false);

        let (status, reasons) = evaluate_validation(
            &thread,
            &losing,
            &baseline,
            &baseline,
            &losing,
            &baseline,
            &losing,
            profile_rules(&thread.strategy_profile),
            thread.duration_days,
        );

        assert_eq!(status, ValidationStatus::Fail);
        assert!(reasons.iter().any(|reason| reason.contains("손실률")));
    }

    #[test]
    fn validation_uses_intraday_round_trip_gates_not_dca_outperformance() {
        let thread = sample_thread();
        let rules = profile_rules(&thread.strategy_profile);
        let mut strategy = test_simulation(1.2, 0.5, false);
        strategy.round_trips = min_round_trips_for_period(rules, thread.duration_days);
        strategy.profit_factor = rules.profit_factor_threshold;
        strategy.expectancy_krw = 500.0;
        strategy.average_hold_hours = rules.average_hold_limit_hours;
        strategy.exposure_percent = rules.exposure_limit_percent;
        let mut dca = test_simulation(15.0, 3.0, false);
        dca.exposure_percent = 100.0;
        let buy_hold = test_simulation(30.0, 8.0, false);

        let (status, reasons) = evaluate_validation(
            &thread,
            &strategy,
            &dca,
            &buy_hold,
            &strategy,
            &dca,
            &strategy,
            rules,
            thread.duration_days,
        );

        assert_eq!(status, ValidationStatus::Pass);
        assert!(reasons
            .iter()
            .any(|reason| reason.contains("PASS gate가 아닙니다")));
    }

    #[test]
    fn parses_upbit_remaining_req_sec_header() {
        assert_eq!(
            parse_upbit_remaining_req_sec("group=candle; min=1800; sec=7"),
            Some(7)
        );
        assert_eq!(
            parse_upbit_remaining_req_sec("group=candle;sec=0; min=1800"),
            Some(0)
        );
        assert_eq!(
            parse_upbit_remaining_req_sec("group=candle; min=1800"),
            None
        );
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

    fn test_simulation(
        return_percent: f64,
        max_drawdown_percent: f64,
        max_loss_breached: bool,
    ) -> Simulation {
        Simulation {
            return_percent,
            max_drawdown_percent,
            trades: Vec::new(),
            fees_krw: 0.0,
            cost_drag_krw: 0.0,
            round_trips: 0,
            win_rate_percent: 0.0,
            profit_factor: 0.0,
            expectancy_krw: 0.0,
            average_hold_hours: 0.0,
            exposure_percent: 0.0,
            max_loss_breached,
            daily_cap_breached: false,
            stop_exit_count: 0,
            time_exit_count: 0,
            day_flat_exit_count: 0,
        }
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
            final_confirmation_text: None,
            final_confirmed_at: None,
            created_at: now,
            updated_at: now,
        }
    }
}
