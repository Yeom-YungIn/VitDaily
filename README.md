# VitDaily

VitDaily is a local-first Tauri desktop app for building a safety-gated Upbit investment workflow.

The current v1 implementation focuses on portfolio-grade product structure, investment threads, strategy validation, paper execution, local analytics, and audit visibility. Real Upbit order submission is deliberately not enabled in the normal app flow.

## Current Status

- App shell: Overview, Threads, Strategies, Schedules, Logs, Settings.
- Supported markets: `KRW-BTC`, `KRW-ETH`, `KRW-XRP`.
- Strategy profiles: 안정적, 보수적, 공격적.
- Live trading: locked and fail-closed.
- Paper execution: enabled for investment threads through simulated logs only.
- Real Upbit order submission: excluded from the current release path.

VitDaily is not investment advice and does not guarantee profit. Strategy and paper results are product validation signals, not recommendations.

## Core Features

### Investment Threads

Investment threads are independent units with:

- market
- initial budget
- duration
- strategy profile
- max-loss limit
- daily trade cap
- lifecycle status: Draft, Paper, Armed, Live, Paused, Stopped, Completed

Current creation and editing flows keep threads non-live. Editing risk or strategy settings invalidates prior validation where appropriate.

### Strategy and Backtest

VitDaily can run recent 1-year strategy validation using public Upbit candle data.

Implemented strategy foundations:

- MACD
- Bollinger Bands
- ATR and ATR stop logic
- buy-and-hold baseline
- DCA baseline
- fees and slippage assumptions
- pass/fail/stale validation status
- human-readable signal and failure reasons

Backtests never submit orders.

### Paper Execution Loop

Paper execution evaluates the latest strategy signal for a thread and records simulated paper logs.

Safety properties:

- works without API credentials
- records `paper` mode logs only
- uses an idempotency key to prevent duplicate tick execution
- evaluates the shared Live Order Gate for audit visibility
- does not call the Upbit order endpoint

### Portfolio Analytics

The Overview surface summarizes local and simulated portfolio state:

- total budget
- invested amount
- current estimated value
- return percent
- max drawdown
- allocation by market
- thread-level validation performance

When real local trade logs are absent, the app can use backtest validation results as simulated portfolio evidence.

### Logs and Safety Events

The Logs screen separates:

- live trade logs
- paper trade logs
- blocked order logs
- API failure logs
- safety gate events
- validation events

Paper, live, and blocked states are visually distinct.

### Schedules

The legacy recurring DCA schedule UI remains available. The current scheduler path is reconciled behind a fail-closed safety boundary, so legacy schedules cannot bypass the shared live-order policy.

Schedule edits support immediate or delayed application, and pending changes are validated before application.

### Credentials and Local Storage

Upbit credentials are stored in the operating system keyring, not in JSON files.

Local JSON files are stored under the OS local data directory in a `vitdaily` folder:

- `schedules.json`
- `logs.json`
- `settings.json`
- `investment-threads.json`
- `thread-validations.json`
- `safety-events.json`
- `portfolio-snapshots.json`

Versioned v1 storage is used for newer thread, validation, safety, and portfolio files.

## Safety Model

VitDaily treats live trading as unsafe by default.

Current invariants:

- Global Live Lock defaults to locked.
- Unsupported markets are rejected.
- Legacy schedule live execution is blocked until explicitly migrated.
- Investment thread live order attempts must pass the shared Live Order Gate.
- Paper execution does not become live execution.
- No regular backend command submits a real Upbit order in the current scope.
- Regression tests assert that real Upbit order submission remains isolated and unwired.

The real order adapter remains outside the active product path until a future live-trading package explicitly enables it behind final confirmation, credential readiness, validation, max-loss, daily-cap, and audit requirements.

## Demo Flow Without Real Money

1. Open the app.
2. Create an investment thread for `KRW-BTC`, `KRW-ETH`, or `KRW-XRP`.
3. Run a recent 1-year backtest.
4. Review pass/fail reasons and baseline comparisons.
5. Run Paper execution.
6. Confirm that the thread enters Paper state and records paper-only logs.
7. Open Logs and verify paper, validation, and safety events are separated.
8. Open Settings and confirm live trading remains locked.

This flow does not require Upbit credentials and does not place real orders.

## Tech Stack

- Tauri 2
- Rust 2021
- React 19
- TypeScript
- Vite
- Tailwind CSS 4

## Getting Started

Install dependencies:

```sh
npm install
```

Run the frontend dev server:

```sh
npm run dev
```

Run the Tauri desktop app:

```sh
npm run tauri dev
```

Build the frontend:

```sh
npm run build
```

Check the Rust backend:

```sh
cd src-tauri
cargo check
```

Run Rust tests:

```sh
cd src-tauri
cargo test
```

## Project Structure

```text
src/
  App.tsx                  React app shell and tab routing
  components/              Overview, threads, strategies, schedules, logs, settings UI
  types/                   Frontend TypeScript domain types

src-tauri/
  src/commands.rs          Tauri commands, local storage, safety gates, scheduler, Upbit API helpers
  src/strategy.rs          Strategy indicators, backtest, signal evaluation
  src/types.rs             Backend domain models
  src/lib.rs               Tauri builder and command registration
  tauri.conf.json          Tauri configuration
```

## Verification Snapshot

Latest verified commands during G007 work:

```sh
npm run build
cd src-tauri && cargo check
cd src-tauri && cargo test
git diff --check
```

All passed after this documentation update was prepared.
