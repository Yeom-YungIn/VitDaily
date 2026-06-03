# Design

## Source of truth

- Status: Product Foundation baseline — Milestone 0 decisions confirmed, v1 live trading still gated
- Last refreshed: 2026-06-02
- Primary product surfaces:
  - Overview
  - Threads
  - Thread Create
  - Thread Detail
  - Strategies
  - Schedules
  - Logs
  - Settings
- Evidence reviewed:
  - `.omx/plans/prd-vitdaily-investment-threads.md`
  - `.omx/plans/test-spec-vitdaily-investment-threads.md`
  - `.omx/plans/wireframe-vitdaily-investment-threads.md`
  - `.omx/specs/deep-interview-vitdaily-spec-develop.md`
  - `README.md`
  - `src/App.tsx`
  - `src/components/Dashboard.tsx`
  - `src/components/ScheduleList.tsx`
  - `src/components/ScheduleForm.tsx`
  - `src/components/PurchaseLogs.tsx`
  - `src/components/Settings.tsx`
  - `src/index.css`
  - `public/vitdaily-icon.png`
- Evidence notes:
  - [observed] Current app uses Tauri + React + TypeScript + Tailwind CSS.
  - [observed] Current UI is dark slate, compact, Korean-first, with orange primary actions and green/yellow/red status colors.
  - [observed] Product Foundation shell now uses six top tabs: Overview, Threads, Strategies, Schedules, Logs, Settings.
  - [observed] Main product surfaces use a wider desktop layout while legacy settings/schedule forms remain compact where appropriate.
  - [inference] Portfolio analytics and strategy validation can now be layered into the broader desktop information architecture without changing the top-level shell.

## Brand

- Personality:
  - Calm, technical, trustworthy, portfolio-grade.
  - More like a careful investment operations dashboard than a flashy trading app.
  - Korean-first language with concise English labels only where standard in trading/product UI: Paper, Live, Armed, Backtest, P/L.
- Trust signals:
  - Always-visible Paper/Live state.
  - Clear disabled states for unsafe actions.
  - Explicit safety gates and blocked-order reasons.
  - API key storage explanation: OS keyring, never displayed after saving.
  - Audit trail for live, paper, blocked, failed, and safety events.
- Avoid:
  - Gambling/casino aesthetics.
  - Profit-guarantee language.
  - “추천”, “수익 보장”, “자동으로 돈 버는” tone.
  - Hiding live-trading risks behind small text.
  - Bright red/green overload that makes the app feel like a speculative exchange terminal.

## Product goals

- Goals:
  - Present VitDaily as a portfolio-quality desktop product.
  - Support personal local-first crypto auto-investing through Upbit.
  - Make investment threads understandable as independent budget/time/strategy units.
  - Make live trading deliberately hard to enable accidentally.
  - Make paper/backtest/demo states visually distinct from live trading.
- Non-goals:
  - Paid service / monetization.
  - Non-Upbit exchange support.
  - Short selling, leverage, or lending.
  - Mobile app.
  - SaaS/web service.
  - Social login/account system.
  - Cloud server operation.
  - Real-time investment advisory service.
  - User-uploaded strategy code.
- Success signals:
  - A user can understand the product from Overview without reading source code.
  - A thread’s status, risk gates, and next safe action are visible at a glance.
  - No screen implies live trading is active unless it truly is.
  - The design can be shown in a portfolio README/screenshots as a coherent product.

## Personas and jobs

- Primary personas:
  - Developer/investor building a portfolio-grade desktop investment app.
  - Personal user who wants disciplined recurring and algorithmic crypto investing.
  - Reviewer/recruiter evaluating product thinking, UX, architecture, and safety.
- User jobs:
  - Store and test Upbit API credentials securely.
  - Create recurring DCA schedules.
  - Create investment threads with market, budget, duration, strategy, and safety limits.
  - Run or review backtest/paper validation before live activation.
  - Confirm live trading only after seeing clear risk and safety summary.
  - Monitor asset growth, thread performance, trade logs, and safety events.
- Key contexts of use:
  - Local desktop app on macOS/Windows.
  - User may leave app open for scheduled/background execution.
  - User may be reviewing the app as a portfolio/demo artifact.
  - User may be handling real funds, so error states must be clear and conservative.

## Information architecture

- Primary navigation:
  - Confirmed: top-tab style navigation.
  - Rationale: user selected top tabs over sidebar/compact sidebar.
  - Proposed items:
    1. Overview
    2. Threads
    3. Strategies
    4. Schedules
    5. Logs
    6. Settings
- Core routes/screens:
  - Overview: asset growth, active threads, safety alerts, recent activity.
  - Threads: list, create, detail.
  - Strategies: stable/conservative/aggressive profile shells and validation summaries.
  - Schedules: existing recurring DCA schedules, with policy confirmation.
  - Logs: unified trade, paper, blocked, API, and safety events.
  - Settings: API credentials, notifications, global live lock, readiness checklist, app info.
- Content hierarchy:
  1. Safety/live status.
  2. Portfolio and thread performance.
  3. Next action or blocked reason.
  4. Detailed logs and configuration.
- Required global indicators:
  - Global Live Lock: Locked / Ready / Live-enabled.
  - Credential readiness: Missing / Saved / Balance OK / Order readiness OK.
  - Confirmation readiness: Markets, strategy logic, risk defaults, live copy.

## Design principles

- Principle 1: Safety is primary UI, not secondary copy.
  - Live trading controls must be disabled until all gates pass.
  - Disabled controls must explain why they are disabled.
- Principle 2: Paper and Live are separate product modes.
  - Paper/Backtest is never styled like Live.
  - Live requires stronger visual framing and final confirmation.
- Principle 3: The app should tell a portfolio story.
  - Prefer a coherent Overview, thread lifecycle, and audit trail over adding many indicators.
- Principle 4: Local-first trust must be visible.
  - Settings and onboarding should explain local data, keychain storage, and credential deletion.
- Principle 5: Keep the existing visual DNA but expand the layout.
  - Preserve dark slate/orange/green/yellow/red cues.
  - Move from narrow mobile-like panel to desktop dashboard where needed.
- Tradeoffs:
  - Desktop sidebar improves scalability but is a bigger change from current two-tab UI.
  - More safety copy improves trust but can overwhelm; use progressive disclosure and concise gate summaries.
  - Simple custom charts reduce dependency risk but may be less polished than a chart library.

## Visual language

- Color:
  - Background: dark slate, aligned with current `bg-slate-900` / `bg-slate-800` style.
  - Primary action: orange, aligned with current `bg-orange-500`.
  - Neutral text: slate-100 / slate-300 / slate-400 hierarchy.
  - Success: green for confirmed/safe/pass.
  - Warning: yellow/amber for pending confirmation, armed state, or needs attention.
  - Danger: red for live risk, failure, stop, max-loss, credential/security errors.
  - Live state: use red or red+orange emphasis sparingly and consistently.
  - Paper state: use blue/cyan or neutral badge to avoid confusion with live/profit colors.
- Typography:
  - System UI stack from `src/index.css` remains acceptable.
  - Use tabular/monospace numbers for KRW, BTC, P/L, and timestamps where readability matters.
  - Section headings should stay compact and clear.
- Spacing/layout rhythm:
  - Current compact card rhythm can be reused.
  - Desktop views should use 12–24px spacing steps and two/three-column cards where useful.
  - Avoid forcing all future screens into `max-w-[390px]`; use wider containers for charts/tables.
- Shape/radius/elevation:
  - Preserve rounded cards (`rounded-lg`, `rounded-xl`) and low-elevation dark panels.
  - Use border or subtle background difference instead of heavy shadows.
- Motion:
  - Minimal transitions for tab/nav, toggles, and disabled/enabled gates.
  - No animated trading/profit effects.
- Imagery/iconography:
  - Existing `vitdaily-icon.png` remains the brand icon.
  - Icons may help status scanning, but text labels are required for safety-critical states.

## Components

- Existing components to reuse:
  - `Dashboard` cards and summary patterns.
  - `ScheduleList` list/card row structure.
  - `ScheduleForm` modal form pattern.
  - `PurchaseLogs` status badge pattern.
  - `Settings` credential card, toggle, and message patterns.
- New/changed components:
  - App shell with desktop navigation.
  - `OverviewPage`.
  - `ThreadList`.
  - `ThreadCreateForm`.
  - `ThreadDetail`.
  - `ThreadStatusBadge`.
  - `LiveModeBadge` / `PaperModeBadge`.
  - `SafetyGateSummary`.
  - `ValidationPanel`.
  - `PerformanceChart`.
  - `DrawdownMiniChart`.
  - `TradeEventLog`.
  - `StrategyProfileCard`.
  - `GlobalLiveLockPanel`.
  - `CredentialReadinessPanel`.
- Variants and states:
  - Thread status: Draft, Paper, Armed, Live, Paused, Stopped, Completed.
  - Validation: Missing, Running, Pass, Fail, Stale.
  - Safety gate: Pass, Missing, Blocked, Warning.
  - Order/log: Planned, Submitted, Filled, Failed, Blocked, Simulated.
  - Credential: Missing, Saved, Balance OK, Order readiness OK, Invalid/Revoked.
- Token/component ownership:
  - Keep Tailwind utility classes as the first implementation path.
  - Do not introduce a separate design-token system until repeated duplication justifies it.
  - If tokens emerge, define them as Tailwind-friendly constants/classes rather than a new dependency.

## Accessibility

- Target standard:
  - Aim for WCAG 2.1 AA for contrast, keyboard access, focus visibility, and semantic structure.
- Keyboard/focus behavior:
  - All controls reachable by keyboard.
  - Modals trap focus and restore focus on close.
  - Toggle controls need accessible labels and state.
  - Live activation confirmation must not be triggerable by accidental Enter without clear focused action.
  - Recommended live activation copy: “이 작업은 실제 Upbit 주문을 실행할 수 있습니다. 선택한 마켓, 예산, 전략, 최대 손실률, 일일 거래 횟수 제한을 확인했으며 손실 가능성을 이해했습니다.”
- Contrast/readability:
  - Current dark theme must keep text contrast high; avoid low-contrast slate text for critical warnings.
  - Red/green/yellow cannot be the only status signal; include labels.
- Screen-reader semantics:
  - Tables/logs should use semantic table or list structures with clear labels.
  - Status badges need text that conveys meaning, not only color.
- Reduced motion and sensory considerations:
  - Avoid flashing price/profit updates.
  - Respect reduced-motion preferences for chart/transition animations if added.

## Responsive behavior

- Supported breakpoints/devices:
  - Primary: desktop Tauri window on macOS/Windows.
  - Secondary: narrow windows roughly matching current `390px` layout.
- Layout adaptations:
  - Wide desktop: top-tab app shell with content using two/three-column cards where useful.
  - Medium: top tabs remain primary; content can collapse to fewer columns.
  - Narrow: stack cards vertically, preserve current one-column rhythm.
- Touch/hover differences:
  - Desktop hover affordances are allowed but cannot be the only way to discover actions.
  - Buttons should remain large enough for trackpad/touch-like use.

## Interaction states

- Loading:
  - Show skeleton/card placeholders for portfolio, thread list, and logs.
  - For safety-critical checks, show explicit “확인 중” state.
- Empty:
  - No threads: explain what an investment thread is and show “Create Thread”.
  - No logs: show “아직 기록이 없습니다” and distinguish paper/live filters.
  - No credentials: guide to Settings.
- Error:
  - API failure: show sanitized error and next action.
  - Credential invalid/revoked: disable live readiness and link to Settings.
  - Data read failure: show local data error and keep live trading disabled.
  - Backtest fail: show failure reasons, not only failed status.
- Success:
  - Validation pass: show pass summary and remaining gates.
  - Credential test success: distinguish balance access from order readiness.
- Disabled:
  - Disabled live activation must always explain missing gates.
  - Disabled buttons should not silently ignore clicks.
- Offline/slow network:
  - Treat network failure as unsafe for live readiness.
  - Keep paper/demo information visible when live API is unavailable.

## Content voice

- Tone:
  - Calm, factual, risk-aware.
  - Korean-first, concise, non-alarmist.
- Terminology:
  - “투자 스레드”: independent investment unit.
  - “Paper” / “모의”: simulated mode.
  - “Live” / “실거래”: real order mode.
  - “Armed” / “실거래 준비”: all gates pass but executor has not necessarily traded yet.
  - “Blocked”: safety gate prevented action.
- Microcopy rules:
  - Do not promise profit.
  - Use “실거래 주문이 발생할 수 있습니다” for live warnings.
  - Use blocked reasons in plain language: “최종 확인이 필요합니다”, “일일 거래 한도에 도달했습니다”.
  - API key copy must explain that order permission enables real orders.

## Implementation constraints

- Framework/styling system:
  - React 19 + TypeScript + Tauri 2.
  - Tailwind CSS 4 through Vite.
  - Existing component files under `src/components/`.
- Design-token constraints:
  - Prefer existing Tailwind utility patterns.
  - No new design-system package without explicit approval.
  - No chart dependency unless approved; first pass may use SVG/CSS charts.
- Performance constraints:
  - Charts should render smoothly in desktop app without heavy runtime cost.
  - Avoid unnecessary polling in UI; backend scheduler/executor owns timing.
- Compatibility constraints:
  - macOS/Windows desktop.
  - Dark theme is current default; light theme is not required for 1차 unless later requested.
- Test/screenshot expectations:
  - UI implementation should be smoke-tested in the Tauri/Vite app.
  - Future visual iteration may use `$visual-ralph` after a visual reference/baseline is approved.
  - Screenshots for portfolio README should include Overview, Thread Detail, Settings safety/readiness.

## Open questions

- [x] Confirm navigation style: top-tab style / owner: user / impact: app shell implementation.
- [x] Confirm supported markets/altcoins: KRW-BTC base plus Ethereum and Ripple support (`KRW-ETH`, `KRW-XRP`) / owner: user / impact: Thread Create, Settings readiness, backend validation.
- [x] Confirm strategy logic for 안정적, 보수적, 공격적 / owner: research + user confirmation / impact: Strategies screen and validation copy. Confirmed baseline: `.omx/plans/strategy-spec-vitdaily.md`; may be revised after backtest and paper/live operation evidence.
- [x] Confirm default max-loss percent: 50% / owner: user / impact: Create Thread safety defaults and safety gate tests. Note: high-risk default; implementation must still require explicit final confirmation.
- [x] Confirm default daily trade cap: 10 trades/day / owner: user / impact: Create Thread, Thread Detail, Live Order Gate.
- [x] Confirm live activation wording: use recommended safety copy / owner: Codex draft + user delegated / impact: final confirmation modal and legal/risk clarity.
- [x] Confirm schedule policy: existing edit default remains next-day to preserve legacy DCA behavior; immediate apply remains an explicit edit option / owner: design review / impact: Schedules screen and backend behavior.
- [ ] Confirm whether first implementation should include sample/demo data for portfolio screenshots / owner: user / impact: README/screenshots and demo flow.


## Confirmed Milestone 0 decisions — 2026-06-02

- Navigation: top-tab style, not sidebar.
- Supported markets for first pass: `KRW-BTC`, `KRW-ETH`, `KRW-XRP`.
- Default max-loss percent: 50%.
- Default daily trade cap: 10 live trades per day.
- Live activation copy: use recommended safety confirmation wording.
- Strategy criteria: confirmed as implementation-planning baseline; may be revised after backtest and paper/live operation evidence.
- Schedule policy: existing edit default remains next-day; immediate apply must be explicit, not silent.

### Recommended live activation copy

> 이 작업은 실제 Upbit 주문을 실행할 수 있습니다. 선택한 마켓, 예산, 전략, 최대 손실률, 일일 거래 횟수 제한을 확인했으며 손실 가능성을 이해했습니다. 실거래를 활성화하려면 아래 내용을 다시 확인하세요.

Short button label:

> 위험을 이해했고 실거래를 활성화합니다

### Remaining design/planning questions

- Strategy criteria: confirmed baseline captured in `.omx/plans/strategy-spec-vitdaily.md`; future changes require evidence from backtest or operation.
- Whether sample/demo data should be included for portfolio screenshots.


## Strategy profile research handoff

- Research artifact: `.omx/plans/research-vitdaily-strategy-profiles.md`
- Proposed profile trade-frequency targets:
  - 안정적: 0–2 trades/day
  - 보수적: 0–5 trades/day
  - 공격적: 0–10 trades/day
- Proposed indicator candidates: RSI, MACD, Bollinger Bands, ATR, SMA/EMA, volume/trade activity filters.
- Status: strategy criteria confirmed as implementation-planning baseline; may be revised after backtest and paper/live operation evidence.


## Strategy parameter research update

- Backtest period confirmed by user: last 1 year / most recent 365 days.
- Strategy parameter draft artifact: `.omx/plans/strategy-spec-vitdaily.md`.
- Draft defaults pending approval:
  - MACD: `12,26,9`
  - Bollinger Bands: `20,2`
  - ATR: `14`
  - Chandelier-style ATR stop: 안정적 `22,3.5`, 보수적 `22,3.0`, 공격적 `14,2.0`
- These values are confirmed UI/product strategy defaults for implementation planning, not live-trading authorization.
