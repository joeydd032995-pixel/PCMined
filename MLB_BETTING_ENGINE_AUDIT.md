# Quantitative Audit — MLB NRFI/YRFI Betting Prediction Engine

**Target repository:** `joeydd032995-pixel/v0-mlb-betting-analytics` (cloned read-only at audit time)
**Audit date:** 2026-06-09
**Scope:** Full prediction pipeline — data ingestion → feature construction → 7/9-model ensemble → calibration → odds/edge/Kelly output → backtest metrics → Python training pipeline.
**Method:** Line-by-line formula extraction and cross-verification against canonical forms; unit/scale checks; numerical spot-checks; test-suite execution (110/110 pass); external verification of the odds-API market against vendor documentation.

All `file:line` references below are to the `v0-mlb-betting-analytics` repository.

---

## Executive Summary

The engine is well-organized and unusually well-commented, with several genuinely correct implementations (Kelly, EV, odds conversion, Markov state machine, air density, point-in-time backfill stats, walk-forward CV in training). However, the audit found **four critical defects** and a structural pattern of **compensating distortions**: multiple components are individually miscalibrated or on the wrong probability scale, and a single monotonic calibration spline absorbs the net error. The headline output lands near sane values *at the league average*, but the discriminative signal between games is degraded, several documented claims do not match the code, and — most materially for a betting product — **the live odds integration cannot work at all**, so the edge/EV/Kelly layer never runs in production.

### Critical findings (ranked)

| # | Finding | Where |
|---|---------|-------|
| C1 | Odds fetch is non-functional: invalid market key, wrong endpoint, wrong outcome labels → no live `valueAnalysis` ever | `lib/api/odds.ts` |
| C2 | "First-inning" pitcher stats are season-level stats relabeled; the NRFI rate is synthesized from season ERA with a directionally wrong adjustment | `lib/api/shared-helpers.ts`, `lib/api/live-data.ts` |
| C3 | Bayesian shrinkage target is on the wrong scale (full-game 0.516 vs per-half ~0.65–0.72), inflating expected runs ~2× before calibration un-distorts it | `lib/nrfi-models.ts` |
| C4 | Backtest bet simulation prices YRFI bets at NRFI odds and flips negative American odds positive via `Math.abs()` → reported ROI/Sharpe unreliable | `lib/backtest-metrics.ts` |

---

## Phase 1 — Codebase Inventory

### 1.1 Prediction-engine files

**Core engine (TypeScript, live path):**
- `lib/nrfi-engine.ts` (741 LOC) — main loop, λ computation, blending, calibration, value analysis
- `lib/nrfi-models.ts` (879 LOC) — Poisson, ZIP, 24-state Markov, MAPRE, 3 meta-models, shrinkage, Log-5 PA outcomes
- `lib/calibration.ts` / `lib/calibration-v2.ts` — monotone piecewise-linear calibration knots
- `lib/ensemble-plus.ts` — 9-model stacker (ensemble7 + DeepNRFI + Monte Carlo)
- `lib/deepnrfi-model.ts` — LightGBM text-format booster parser + inference
- `lib/monte-carlo.ts`, `lib/monte-carlo-bridge.ts` — seeded stochastic first-inning simulator
- `lib/features/feature-vector.ts` (69 features), `air-density.ts`, `park-factors-extended.ts`, `umpire-zone.ts`
- `lib/weather.ts` — vector wind multiplier
- `lib/advanced-stats.ts` — FIP/xFIP/SIERA/xERA/wOBA/wRC+ calculators (mostly display path)
- `lib/utils/odds.ts` — odds conversion, EV, Kelly utilities
- `lib/backtest-metrics.ts` — Brier/accuracy/ROI/Sharpe/drawdown/calibration bins
- `lib/config.ts` — flags, league constants, Kelly config
- `lib/prediction-store.ts` — persistence of tracked predictions

**Data layer:**
- `lib/api/mlb-stats.ts` — MLB Stats API (schedule, linescores, season + game-log splits, as-of stats)
- `lib/api/live-data.ts` — maps API data → engine `Pitcher`/`Team`/`Game` objects
- `lib/api/shared-helpers.ts` — `estimateNrfiRate`, `estimateOffenseFactor`
- `lib/api/odds.ts` — The Odds API integration
- `lib/api/weather.ts` — Open-Meteo (live + historical)
- `lib/api/sportsblaze.ts` — team vs-hand splits
- `lib/api/statcast.ts`, `statcast-normalize.ts` — Baseball Savant aggregates
- `app/api/historical-sync/route.ts` — backfill of results + backtested predictions

**Python pipeline:** `scripts/deepnrfi/` — `build_real_training_set.py`, `train.py`, `backtest_v2.py`, `recalibrate.py`, `park_factors.py`, `weather_archive.py`, `predict.py`.

### 1.2 Primary prediction loop

`computeNRFIPrediction(game, pitchers, teams)` in `lib/nrfi-engine.ts:447` is the entry point (batch wrapper `computeAllPredictions`, called from `app/api/predictions`, `app/api/historical-sync`, and the dashboard). Flow:

1. Dynamic Bayesian shrinkage of each pitcher's NRFI rate (`precomputePitcherContext`)
2. Handedness/lineup offense factor → per-half λ = −ln(shrunkRate) × offense × park × weather × month × umpire
3. 7 models per half-inning (`compute7ModelEnsemble`) → game-level blend (`blend7Models`)
4. Monotone spline calibration → 76/24 blend with calibrated league anchor → clamp [0.18, 0.85]
5. Optional (flag-gated, **default off**): DeepNRFI LightGBM + Monte Carlo → 9-model stacker → calibration v2
6. Confidence/conviction scoring, factor narratives, value analysis (edge/EV/Kelly) when odds exist

### 1.3 External data sources & staleness risks

| Source | Use | Risk |
|---|---|---|
| MLB Stats API | schedule, linescores, season + game-log stats | Low; IP string parsing (".1"/".2" = thirds) is handled **correctly** (`mlb-stats.ts:115-125`) |
| The Odds API | NRFI/YRFI prices | **Broken — see C1.** Silent failure mode returns `[]`, so the app runs odds-less without alerting |
| Open-Meteo | live + historical weather | OK; historical backfill uses **monthly average temps** (not real game-time) unless `recompute=true` |
| SportsBlaze | team vs-hand splits | Optional; falls back silently |
| Baseball Savant (Python) | pitch mix, velo/spin | Only reaches the engine when DeepNRFI artifacts exist (they don't — see Phase 4) |
| Static tables | park factors (×2 tables), monthly λ factors, monthly temps, 2024 league constants, season start dates | Manually curated; require annual maintenance; two separate park-factor tables (`mlb-stadiums.ts`, `park-factors-extended.ts`) can drift (spot-checked consistent today) |
| Umpire profiles | `lib/features/umpire-zone.ts:31` | **Empty stub** — every lookup returns neutral; the README's "umpire bias integrated" claim is aspirational in practice |

Also: the user-facing `/ensemble` pages import `mockGames`/`mockPitchers`/`mockTeams` from `lib/mock-data.ts` — a production page rendering predictions computed from **fabricated inputs**.

---

## Phase 2 — Formula Audit

### 2.1 Verified-correct formulas (no deviation found)

| Formula | Location | Verdict |
|---|---|---|
| American→implied: `100/(odds+100)` / `|odds|/(|odds|+100)` | `lib/utils/odds.ts:17-22` | ✅ canonical |
| American→decimal, decimal→American | `lib/utils/odds.ts:3-15` | ✅ (div-by-zero at decimal=1.0 unguarded — edge case) |
| EV per unit: `p·b − (1−p)` ≡ `p·d − 1` | `nrfi-engine.ts:133-136`, `utils/odds.ts:24-27` | ✅ canonical |
| Kelly: `f* = (bp − q)/b`, quarter-Kelly | `nrfi-engine.ts:125-131`, `utils/odds.ts:29-35` | ✅ formula correct (variable misnamed `decimalOdds`; it holds `b`). Negative-Kelly rejected via `Math.max(0, …)` ✅ |
| Poisson zero: `P(0) = e^{−λ}` | `nrfi-engine.ts:667-668`, models | ✅ |
| ZIP zero: `P(0) = ω + (1−ω)e^{−λ}` | `nrfi-models.ts:405` | ✅ structural form |
| Log-5 (Bill James): `(bp/l) / (bp/l + (1−b)(1−p)/(1−l))` | `nrfi-models.ts:112-118` | ✅ canonical |
| Markov 24-state advancement (walk forcing incl. bases-loaded score, single/double/triple/HR) | `nrfi-models.ts:196-250` | ✅ all 8 runner configs verified by hand |
| Monte Carlo simulator + histogram convolution + seeded PRNG | `lib/monte-carlo.ts` | ✅ mirrors Markov machine; deterministic |
| FIP: `(13·HR + 3·(BB+HBP) − 2·K)/IP + C` | `advanced-stats.ts:326-330` | ✅ canonical |
| xFIP structural form | `advanced-stats.ts:333-339` | ✅ form correct, but see 2.3.4 (live inputs make it ≡ FIP) |
| Moist-air density (partial pressures, Tetens) | `lib/features/air-density.ts` | ✅ physically correct |
| OBP, SLG, ISO, BABIP, K%, BB%, WHIP | `advanced-stats.ts` | ✅ canonical |
| Empirical-Bayes regression `(n·x + N·μ)/(n+N)` | `advanced-stats.ts:202-206` | ✅ form correct |
| `P(YRFI) = 1 − P(NRFI)` exact complement | `nrfi-engine.ts:585` | ✅ |

### 2.2 Critical formula defects

#### C2 — `estimateNrfiRate`: synthetic NRFI rate from season ERA, wrong-direction adjustment

`lib/api/shared-helpers.ts:31-33`:
```ts
export function estimateNrfiRate(era: number): number {
  return Math.min(0.90, Math.max(0.45, Math.exp(-(era * 0.95) / 9)))
}
```
Math notation: `NRFI_rate = clamp(e^{−0.95·ERA/9}, 0.45, 0.90)`.

Problems:
1. **The pitcher's "first-inning" record is not first-inning data.** `mapPitcher` (`live-data.ts:236-257`) and `buildLightPitcher` (`historical-sync/route.ts:61-102`) populate every `firstInning.*` field — ERA, WHIP, K%, BB%, HR/9, `nrfiRate` — from the pitcher's **season** line. Only `last5Results` comes from real first-inning linescores. The UI then asserts "X has a 72% NRFI rate this season — among the league's best" (`nrfi-engine.ts:218`) about a number derived from ERA, not observed.
2. **ERA excludes unearned runs**; NRFI resolves on all runs (~7-8% of runs are unearned). Bias: NRFI-optimistic.
3. **The 0.95 multiplier ("typical first-inning ERA advantage — fresh arm") is directionally wrong.** The 1st inning is historically the *highest-scoring* inning in MLB (top of the order guaranteed, starter settling in) — that asymmetry is the entire reason the YRFI market exists. The factor should be >1 (≈1.1–1.25 vs a pitcher's overall rate), not 0.95. Bias: NRFI-optimistic, again.
4. `firstBatterOBP = (WHIP/(1+WHIP))·0.85` (`live-data.ts:228`) is also fabricated, and `avgRunsAllowed = 1 − nrfiRate` (`live-data.ts:250`) stores a *probability complement* in a field whose name and downstream usage imply *expected runs* — a units error.

#### C3 — Shrinkage target on the wrong probability scale

`lib/nrfi-models.ts:19,624-629`:
```ts
export const LEAGUE_AVG_NRFI = 0.516   // P(BOTH halves score 0) — full game
...
const shrunk = (observed * n + LEAGUE_AVG_NRFI * priorWeight) / (n + priorWeight)
```
`observed` is the pitcher's per-half zero-run rate (league mean ≈ 0.65 by the repo's own estimator at ERA 4.12, ≈ 0.72 empirically, since `√0.516 ≈ 0.718`). It is shrunk toward **0.516**, the *full-game* NRFI rate. With the production prior `k = 50` and a typical `n = 20` starts, an average pitcher's 0.647 collapses to 0.554; then `λ = −ln(0.554) = 0.59` runs per half-inning — roughly **2×** the true first-inning half rate (~0.27–0.33). Consequence: the raw Poisson component evaluates an *average* game at `e^{−1.18} ≈ 0.31` P(NRFI) against an actual 0.516, and the calibration spline (`calibration.ts:15-35`, which lifts mid-range values, e.g. 0.40→0.436, 0.50→0.542) plus the 24% league anchor must repair a ~20-point systematic distortion. The pipeline functions only because two large errors point in opposite directions. The same wrong target propagates into `hierarchicalBayes` (below), the feature builder (`feature-vector.ts:89`), and the Python training set.

The deprecated `bayesianShrinkage` (k = 1.14, `nrfi-models.ts:81-97`) has the opposite problem: for a binomial rate with the documented between-pitcher variance σ²≈0.035, the beta-binomial prior strength is `k ≈ μ(1−μ)/σ²_between − 1 ≈ 6`, not `σ²w/σ²b ≈ 1.14`; the code comment's own k-derivation formula is not the right one for proportions. The production k ∈ {30, 50, 80} is plausible in magnitude but stated to come from pitcher-type heuristics, not from a fitted variance decomposition.

#### 2.2.3 — Double shrinkage in `hierarchicalBayes`

`nrfi-models.ts:803-808` re-shrinks `ctx.shrunkRate` — which is *already* the shrinkage output — toward the league mean a second time with the same k. At n=20, k=50 a raw 0.65 pitcher becomes 0.554 → 0.527. Game-level is then the product of two such values (`blend7Models`, `nrfi-engine.ts:201`) ≈ **0.27** for an average matchup — the component is not on the same scale as the quantity it is averaged with (weight 1.8%, so damage is bounded, but see Phase 4 on component-scale chaos).

#### 2.2.4 — `nnInteraction` is a ratio, blended as a probability

`nrfi-models.ts:796-798`:
```ts
const nnInteraction = clamp(poisson * markov / LEAGUE_AVG_NRFI, 0.02, 0.98)
```
At league-average inputs `0.72 × 0.72 / 0.516 ≈ 1.0`, clamped to **0.98**. The code comment explicitly celebrates that the normalizer makes the value "≈ 1.0 instead of ≈ 1.46" — i.e., the quantity is designed to be a *ratio centered at 1*, then handed to `blend7Models` (game level: average of halves, `nrfi-engine.ts:199`) as if it were a probability. It contributes ≈ `0.027 × 0.98` instead of `0.027 × 0.516` — a constant ≈ +1.3-point NRFI inflation that the spline silently absorbs. There is also no neural network anywhere despite the name and README billing.

#### 2.2.5 — Unit inconsistency: HR/9 converted two different ways

- `computePAOutcomes` (`nrfi-models.ts:143`): `pitcherHR = min(0.07, hrPer9 / 9)` → **HR per inning**, ≈ 4.3× the per-PA rate. League hrPer9 ≈ 1.2 gives 0.133, pinned at the 0.07 cap — still >2× the true per-PA rate (~0.031). This inflated rate enters Log-5 against a league constant of 0.034 (per-PA), mixing units inside one formula.
- MAPRE input (`nrfi-models.ts:771`): `hrPerPa1st = hrPer9 / 38.7` → correct (~4.3 PA/inning).

Same quantity, two conversions; the Markov and Monte Carlo components are biased toward YRFI by the first one.

#### 2.2.6 — `computePAOutcomes` hit-rate construction is internally inconsistent

`nrfi-models.ts:137-185`:
- `pitcherHit = max(0.06, WHIP/3.5 − BB/PA)`: WHIP is per *inning*; dividing by 3.5 PA/inning understates the denominator (≈3.9–4.3 actual), inflating H/PA by ~10–20%.
- `LEAGUE_HIT_RATE = OBP − BB% = 0.229` (`nrfi-models.ts:38`) silently includes HBP (and includes HR), yet `batterHit` is commented "non-HR hits"; line 161 then subtracts `hrProb` from `hitProb` **again** — if `hitProb` were truly non-HR this double-subtracts; if it includes HR the labels are wrong. Either way one convention is violated.
- The out-floor renormalization (lines 167-184) is correct and guarantees Σp = 1.0 ✅.
- Single/double advancement is conservative (runner from 2nd never scores on a single; runner from 1st never scores on a double; no sac flies) — a documented simplification biasing toward NRFI, partially offsetting the inflated hit/HR rates. Compensating errors again.

#### 2.2.7 — SIERA sign error; xERA fed garbage (display path)

`advanced-stats.ts:342-359`: the "simplified SIERA" has `+6.3·K%` — strikeouts *increase* this SIERA. Canonical SIERA has ≈ −16.99·(SO/PA). ∂SIERA/∂K% here is ≈ +4 net of the interaction term: a high-K ace is scored *worse* than a soft-tosser. `calculateXERA = −8.5 + 41·xwOBA` expects a real wOBA-scale input, but `calculateXwOBA` builds "xwOBA" from a fabricated 6-bucket EV/LA step function (`advanced-stats.ts:251-258`, values 0.1/2.0/0.9/0.3/0.2/0.6) that is not on the wOBA scale. Neither feeds the NRFI loop (display only), but both are presented as advanced metrics.

#### 2.2.8 — wOBA discrepancies

- TS weights (`config.ts:77-85`): 0.69/0.72/**0.89/1.27/1.62/2.10**. Python builder (`build_real_training_set.py:471-491`): 0.692/0.722/**0.882/1.254/1.590/2.050** — the Python comment claims it "matches lib/config.ts"; it does not (HR differs by 0.05).
- Denominator uses PA (`advanced-stats.ts:109`) instead of canonical `AB + BB − IBB + SF + HBP` — small systematic understatement for players with SH/IBB.
- wRC+ park adjustment divides the *entire* wRC+ by park factor (`advanced-stats.ts:146`) — crude vs the canonical park-adjusted-runs construction.

### 2.3 Coefficient validation (Phase 2d)

| Constant | Value | Claimed provenance | Audit assessment |
|---|---|---|---|
| `ENSEMBLE_BLEND` | 0.76 | "revise only via walk-forward CV" (`nrfi-engine.ts:73`) | No CV artifact in repo; the ensemble-weights TODO (`nrfi-models.ts:579-581`) admits weights are **not** CV-optimized — the README's "8 optimizations" framing overstates validation |
| `RAW_ENSEMBLE_WEIGHTS` | .12/.30/.48/.10/.05/.03/.02 | design intent | Sum = 1.10; normalization is handled correctly (`nrfi-models.ts:591-594`) ✅, but values unvalidated by the repo's own admission |
| `MONTHLY_LAMBDA_FACTOR` | 0.88–1.06 | "derived from 2018–2024 first-inning rates" | No derivation artifact; **double-counts temperature** with the vector weather multiplier when real weather is present (acknowledged in comment, `nrfi-engine.ts:96-98`) |
| ZIP constants (−1.38, 4.0, 0.008, 0.18, 0.42, 0.90, 0.60, 0.004) | `nrfi-models.ts:383-400` | "calibration" comments | Asserted, not fitted; ω clamp [0.08, 0.60] and λ floor are arbitrary |
| `logisticMeta` α=−2.3, β=4.1 | `nrfi-models.ts:787` | "captures log-odds baseline" | Self-inconsistent: at league-average half-NRFI (0.718) it outputs σ(0.644) = 0.656, ~6 pts low; matching the identity at the mean requires α ≈ −2.01 given β = 4.1 |
| MAPRE multipliers (0.0015, 1.8, 9, 0.12) and deltas | `nrfi-models.ts:515-523` | "hidden 2024-25 factors" | Unvalidated. **Doc/code mismatch:** docstring says Δ_HFA = −0.045 (`nrfi-models.ts:477`), code applies −0.030 (`:522`) |
| MAPRE ρ = 0.06, NegBin r = 1.3, λ>0.8 switch | `nrfi-models.ts:550-561` | none | Ad hoc; the correlation correction is a step function (0 or 0.06), discontinuous at λ = 0.60 |
| `estimateNrfiRate` 0.95 | `shared-helpers.ts:32` | "fresh arm advantage" | **Wrong direction** (see C2) |
| First-inning team runs 0.48, vsLHP ×1.05 | `live-data.ts:291,317` | none | Assumed |
| Wind 0.012/mph, humidity −0.08, clamp [0.82, 1.22] | `weather.ts:25` | none | Magnitudes plausible; humidity sign issue below |
| Lineup tilt ±5% | `nrfi-models.ts:697-703` | reasoned in comment | Conservative and defensible ✅ |
| `CONFIG.kelly` (minEdge .02, maxBet .05) vs engine (`MIN_KELLY_EDGE` .03, cap .25) | `config.ts:103-107` vs `nrfi-engine.ts:57-58` | — | **Two disagreeing sources of truth**; `CONFIG.kelly` appears unused by the engine |

---

## Phase 3 — Probability & Odds Output Validation

### 3.1 Probability integrity — ✅ with caveats
- Final output clamped to [0.18, 0.85]; `yrfiProbability = 1 − nrfiProbability` exactly (`nrfi-engine.ts:578-585`). Binary complement sums to 1.0 ✅.
- PA-outcome distribution renormalized to Σ = 1.0 ✅ (`nrfi-models.ts:174-184`); Monte Carlo CDF forces the last bucket to 1 ✅.
- Intermediate "probabilities" are **not** all probabilities (nnInteraction ratio, double-shrunk hierarchicalBayes) — see 2.2.3/2.2.4. The UI's model-breakdown panel displays these as per-model NRFI probabilities, which is misleading.
- `impliedToAmerican` divides by zero at p = 0/1 (`nrfi-engine.ts:120-123`); unreachable from the clamped engine but exported.

### 3.2 Implied odds conversion — formula ✅, vig handling ✗, integration ✗✗

- Conversion formulas match the canonical forms exactly (`utils/odds.ts:17-22`) ✅.
- **No vig removal exists anywhere** — no additive/multiplicative/Shin/power method. For *edge vs. one side's actual price* this is acceptable (edge against the vig-inclusive implied is the economically relevant quantity), but the app surfaces `impliedNrfiProb`/`impliedYrfiProb` (which sum to >1) without noting the overround, and the backtest treats `−(p − implied_NRFI)` as the YRFI edge, which **ignores the vig on the opposite side** and overstates YRFI value by roughly the full overround (~4.5% at −110/−110).
- **C1 — the odds feed cannot work.** `lib/api/odds.ts:26` requests market `batter_first_inning_scored` from `/v4/sports/baseball_mlb/odds`. Verified against The Odds API documentation (2026-06): **no such market key exists**; first-inning totals are `totals_1st_1_innings` (NRFI = Under 0.5), the outcomes are `Over`/`Under` (the code looks for `Yes`/`No`, `odds.ts:51-52`), and inning markets are only served by the per-event endpoint `/events/{eventId}/odds`, not the bulk `/odds` endpoint. Three independent defects; any one is fatal. The API will reject the request, the catch block returns `[]`, `game.odds` stays `undefined`, and `computeValueAnalysis` is never invoked (`nrfi-engine.ts:658`). **In live operation the product has never produced a real edge, EV, or Kelly number.** Backtests then silently default every game to −110 (`backtest-metrics.ts:14`).

### 3.3 Edge / EV / Kelly

- `EV = p·b − (1−p)` ✅ (equivalent to `p·d − 1`).
- Kelly `f* = (bp − q)/b` ✅; quarter-Kelly (0.25) applied, plus a 0.25-of-bankroll cap that can never bind after the fraction (raw Kelly ≤ 1 → 0.25·raw ≤ 0.25) — harmless but dead.
- The Kelly fraction 0.25 is hardcoded with no stated justification; `CONFIG.kelly` disagrees (see table above).
- Negative-Kelly/negative-edge bets correctly rejected (`MIN_KELLY_EDGE = 0.03` gate + `Math.max(0, …)`) ✅.
- `kellyFraction(edge, odds)` reconstructs `modelProb = implied + edge` (`nrfi-engine.ts:126`) — algebraically exact given how edge was computed; fragile coupling, but correct today.
- `oddsToMoneyRisk` (`utils/odds.ts:46-57`) is buggy for negative odds: it returns `risk = |odds|` paired with `win = 100²/|odds|` (e.g. −150 → risk 150 to win 66.67; correct is win 100), and ignores `toWin` for negative odds. Currently dead code — flagged so it isn't wired into the UI as-is.

### 3.4 — C4: Backtest bet simulation is wrong

`lib/backtest-metrics.ts:96-116`:
```ts
const bettingOnNrfi = edge > 0
const betModelProb = bettingOnNrfi ? p : 1 - p
const betSize = kellyBetSize(betModelProb, Math.abs(americanOdds))
```
1. **`Math.abs(americanOdds)` converts −110 into +110** before sizing: `profitPerUnit` becomes 110/100 = 1.10 instead of 100/110 = 0.909 — a 21% overstatement of payout in the sizing formula for every favorite-priced line.
2. **YRFI bets are sized and settled at the NRFI side's odds** (only `nrfiOdds` is ever read; `pl = won ? betSize * profitPerUnit : −betSize` uses the NRFI-derived `profitPerUnit`).
3. **YRFI edge ignores vig** (see 3.2): `edge < −0.03` triggers a YRFI bet even when the true YRFI edge vs the YRFI price is negative.

Combined effect: simulated ROI, Sharpe, and drawdown — the numbers a user would trust to deploy capital — are systematically inflated and not reproducible against real betting. The production `computeValueAnalysis` (`nrfi-engine.ts:395-415`) does this *correctly* (separate `yrfiOdds`, no `abs()`), so backtest and live engine are inconsistent with each other.

---

## Phase 4 — Statistical Model Soundness

### 4.1 Feature engineering
- **v1 live path inputs:** shrunk pitcher NRFI rate (ERA-derived), season K/BB/HR/WHIP relabeled as first-inning, team season-OPS offense factor (optionally vs-hand from SportsBlaze), park factor, vector weather, monthly factor, last-5 form, umpire (always neutral), lineup tilt (flag-gated).
- **Leakage:** live path is clean (pre-game inputs only). Historical backfill correctly uses **as-of** stats (`fetchPitcherStatsAsOf`, `mlb-stats.ts` — game logs strictly `< beforeDate`, Bayesian-blended with prior season; well built ✅). The route docstring (`historical-sync/route.ts:9-11`) still claims 2024/25 rows use current-season stats — stale doc, code is better than its comment.
- **Training pipeline** (sub-audit): `train.py` uses chronological `TimeSeriesSplit` ✅; isotonic calibration fitted on held-out walk-forward predictions ✅; `ensemble7_nrfi` is in `DROP_COLS` ✅ (no stacking circularity in training). **However** `build_real_training_set.py:70` shrinks with the deprecated k = 1.14 while production uses k ∈ {30,50,80} — a real train/serve skew on the single most important feature.
- **Multicollinearity:** top4 OPS + top4 wRC+ + offense factor (OPS-derived) + offense-vs-hand are heavily collinear; tolerable for LightGBM, but the ablation-based "top features" explanations (`deepnrfi-model.ts:318-345`) substitute **0** for ablated features ("median stand-in") — 0 is wildly out-of-distribution for fb_velo (~93.5), stuff_plus (~100), spin (~2300), so the displayed feature attributions are unreliable.
- **DeepNRFI / Monte Carlo / v2 stacker are inert in the shipped configuration:** all flags default off (`config.ts:23-42`) and **no model artifacts exist** in `scripts/deepnrfi/artifacts/` (directory absent). The deployed engine is the v1 7-model path; README/UI marketing of the 9-model stack describes a code path that has never scored a live game.

### 4.2 Sample size & regression to mean
- Shrinkage exists and is applied before every model ✅ — but with the wrong target scale (C3) and double-applied in one component (2.2.3).
- `last5Results` (n = 3–5) drives a ±15% λ multiplier (`nrfi-engine.ts:140-151`) with a 0.30 coefficient and no shrinkage — small-sample noise amplifier, bounded only by the clamp.
- FIP/xFIP are computed but **not used by the prediction path** — λ comes from ERA via `estimateNrfiRate`, so the early-season stability benefit of FIP/xFIP (the standard remedy) is forgone. Worse, in the live path xFIP is illusory anyway: `flyBalls` is estimated as `HR / 0.128` (`live-data.ts:42`), so `xFIP`'s `FB × lgHR/FB ≈ HR` and xFIP ≡ FIP by construction.

### 4.3 Recency weighting
- No decay function — uniform window of exactly 5 starts (hardcoded, not configurable). Deviation-vs-season construction is sensible; weights trivially "sum to 1" (simple mean). The monthly λ table is a hardcoded lookback of a different kind, with the temperature double-count noted above.

### 4.4 Park / environmental adjustments
- Park factor applied **multiplicatively to λ** ✅ (correct for run-based stats); ZIP receives park = 1.0 to avoid double-counting ✅ (`nrfi-models.ts:748-755`).
- Wind is directional (cosine projection) ✅, but the direction tokens are coarse (`in/out/cross`).
- **Humidity sign contradiction:** `weather.ts:10-14,56` dampens carry as humidity rises, commenting "humid air is denser and reduces carry" — physically backwards (water vapor is lighter than dry air; humid air is *less* dense, ball carries *farther*). `air-density.ts` implements the physics correctly, so the two weather modules disagree with each other. Effect bounded (≤8% of the wind term) but it's a sign error.
- Home/away splits: type fields exist (`homeNrfiRate`/`awayNrfiRate`) but are always set equal (`live-data.ts:196-197,255-256`) — home/away first-inning splits are **not actually modeled**. HFA enters only via MAPRE's −0.030 λ delta.

---

## Phase 5 — NRFI/YRFI-Specific Audit

### 5a. First-inning probability model
The model is a λ-scaled Poisson family per half-inning. Of the canonical NRFI drivers:
- ✅ Park factor, weather, handedness — incorporated.
- ⚠️ "Starting pitcher NRFI rate" — incorporated, but **synthetic** (ERA-derived, season-level; C2). Real per-start first-inning data exists in the codebase (`fetchPitcherLast5FirstInnings` parses linescores correctly) but is only used for the last-5 form signal — the season-long observed NRFI rate is never computed from it.
- ⚠️ Opponent 1st-inning offense — proxied by season OPS (`estimateOffenseFactor = OPS/0.720`), not actual first-inning scoring or top-of-lineup OBP (top-4 stats exist only in the dormant DeepNRFI feature vector).
- ✗ Home/away first-inning splits — fields exist, never populated (see 4.4).
- The engine *does* correctly distinguish per-team half-inning components from the combined game probability ✅.

### 5b. Combined probability
`P(NRFI) = P(home half = 0) × P(away half = 0)` is implemented for the probability-scale models (`blend7Models`, `nrfi-engine.ts:201`) ✅. Independence is assumed and **documented but never tested**; the only correlation correction is MAPRE's ad-hoc ρ = 0.06 step (applied only when both λ > 0.60, discontinuous at the threshold, value unsourced). Shared run-environment factors (park, weather, umpire) make the halves positively correlated in reality, which means the pure product slightly *understates* P(NRFI) in extreme environments — directionally the ρ patch is right, but it is applied to only 1 of 7 components.

### 5c. Historical calibration
- A real calibration mechanism exists (19 monotone knots, claimed "backtest regression Apr 2025") and `recalibrate.py` can refit it ✅ in principle.
- **Circularity risk:** the knots were fitted on backfilled predictions generated by the historical-sync route with degraded inputs (monthly-average temperature, calm wind, no odds, no lineups — acknowledged in `historical-sync/route.ts:389-393`), and the public accuracy dashboard reports on the *same* games scored by the *current* model + calibration. That is in-sample evaluation presented as historical performance. Live predictions (real weather, lineups) come from a different input distribution than the one the spline was fitted on.
- `calibration-v2.ts` knots are explicitly hand-"nudged" copies of v1 ("refit … once Phase 4 backtest data is available") — i.e., the v2 path's calibration is not fitted at all.
- The 110 unit tests all pass, but they assert internal consistency (bounds, monotonicity, normalization, null-safety), not statistical correctness against ground truth — appropriate scope for unit tests, just not evidence of calibration.

---

## Consolidated Findings Register

| ID | Severity | Finding | Location | Recommended fix |
|----|----------|---------|----------|-----------------|
| C1 | **Critical** | Odds integration non-functional (invalid market key, wrong endpoint, Yes/No vs Over/Under) — no live edge/EV/Kelly ever produced; failure is silent | `lib/api/odds.ts:26,48,51` | Use `totals_1st_1_innings` via `/events/{id}/odds`; map NRFI = Under 0.5; alert on empty odds |
| C2 | **Critical** | "First-inning" pitcher stats are season stats; NRFI rate synthesized from ERA with wrong-direction 0.95 factor; UI presents synthetic rates as observed | `lib/api/shared-helpers.ts:31-33`, `lib/api/live-data.ts:161-271` | Compute observed first-inning rates from game-log linescores (machinery already exists); replace 0.95 with an empirically fitted >1 first-inning factor; relabel UI claims |
| C3 | **Critical** | Shrinkage target 0.516 (full-game) applied to per-half rates (~0.72 mean) → λ inflated ~2×; spline compensates globally | `lib/nrfi-models.ts:19,624-629` | Shrink toward `√LEAGUE_AVG_NRFI ≈ 0.718` (or a directly measured league per-half zero rate); refit calibration after |
| C4 | **Critical** | Backtest sizes/settles YRFI bets at NRFI odds; `Math.abs()` flips negative odds; YRFI edge ignores vig → ROI/Sharpe inflated | `lib/backtest-metrics.ts:96-116` | Pass signed odds through; carry separate yrfiOdds; compute per-side edges like `computeValueAnalysis` |
| H1 | High | Ensemble blends non-commensurable components (ratio nnInteraction ≈ 0.98, double-shrunk hierarchicalBayes ≈ 0.27, Poisson ≈ 0.31 at avg); spline hides it; any re-weighting breaks calibration | `nrfi-models.ts:735-819`, `nrfi-engine.ts:186-206` | Put every component on the calibrated half-inning probability scale before blending |
| H2 | High | HR/9 unit inconsistency (÷9 vs ÷38.7) — Markov/MC HR rate ~2× league per-PA | `nrfi-models.ts:143` vs `:771` | Use `hrPer9/38.7` everywhere; remove the 0.07 cap masking it |
| H3 | High | `computePAOutcomes` hit-rate: WHIP/3.5 inflates H/PA; "non-HR hit" labeling double-subtracts HR; HBP folded into hits | `nrfi-models.ts:38,143-167` | Derive per-PA event rates from BF-denominated stats; one consistent hit definition |
| H4 | High | Calibration circularity: knots fitted on degraded-input backfill; accuracy page reports in-sample on same games; v2 knots hand-tweaked, unfitted | `calibration*.ts`, `historical-sync/route.ts` | Walk-forward refit on real-weather recomputed rows; report only out-of-sample accuracy; fit v2 before enabling |
| H5 | High | Train/serve skew: training shrinkage k = 1.14 vs production k ∈ {30,50,80}; DeepNRFI artifacts absent (v2 path inert) | `build_real_training_set.py:70,408-410` | Port `getDynamicPriorWeight` to the builder; retrain |
| M1 | Medium | logisticMeta intercept ~6 pts low at league mean; constants asserted not fitted | `nrfi-models.ts:786-787` | Fit (α, β) by logistic regression on outcomes |
| M2 | Medium | Humidity effect sign contradicts physics and contradicts `air-density.ts` | `lib/weather.ts:10-14,56` | Flip sign or drive carry from computed air density |
| M3 | Medium | Monthly λ factor double-counts temperature when real weather present (admitted) | `nrfi-engine.ts:96-116` | Apply monthly factor only when weather is imputed (use the presence flag) |
| M4 | Medium | MAPRE Δ_HFA doc −0.045 vs code −0.030; ρ/NegBin constants unsourced and discontinuous | `nrfi-models.ts:477,522,550-561` | Reconcile; smooth the ρ ramp; fit r |
| M5 | Medium | SIERA sign error (+6.3·K%); xERA fed non-wOBA-scale toy xwOBA (display path) | `advanced-stats.ts:342-367` | Use published SIERA coefficients or remove; use Statcast xwOBA buckets |
| M6 | Medium | xFIP ≡ FIP in live path (FB estimated from HR/0.128); FIP/xFIP unused by prediction loop anyway | `live-data.ts:42`, `advanced-stats.ts:333` | Get real FB counts (Statcast) or drop the xFIP claim; consider FIP-based λ for small samples |
| M7 | Medium | wOBA weight mismatch TS vs Python (claims to match; doesn't); PA denominator non-canonical | `config.ts:77-85` vs `build_real_training_set.py:471-491` | Single source of truth |
| M8 | Medium | No-vig fair probabilities never computed; UI implied probs sum > 1 unannotated | `nrfi-engine.ts:395-415` | Add multiplicative or Shin de-vig for display & CLV tracking |
| M9 | Medium | `/ensemble` pages render predictions from `mock-data.ts` in production UI | `app/ensemble/*.tsx` | Wire to live data or label as demo |
| M10 | Medium | Home/away first-inning splits never populated though typed & documented | `live-data.ts:196,255` | Compute from game logs or remove fields |
| L1 | Low | `oddsToMoneyRisk` wrong win/risk pairing for negative odds (dead code) | `utils/odds.ts:46-57` | Fix before any UI use |
| L2 | Low | `impliedToAmerican`/`decimalToAmerican` div-by-zero at boundary inputs | `nrfi-engine.ts:120`, `utils/odds.ts:14` | Guard |
| L3 | Low | Kelly config duplicated and inconsistent (`CONFIG.kelly` vs engine constants) | `config.ts:103` vs `nrfi-engine.ts:57` | Consolidate |
| L4 | Low | Feature-attribution ablation uses 0 stand-in — out-of-distribution for velo/spin/stuff+ | `deepnrfi-model.ts:330` | Use stored training medians |
| L5 | Low | Last-5 form: tiny unsmoothed sample drives ±15% λ swing | `nrfi-engine.ts:140-151` | Shrink the deviation by sample size |
| L6 | Low | `avgRunsAllowed = 1 − nrfiRate` stores a probability in a runs-named field | `live-data.ts:250` | Store λ or rename |
| L7 | Low | Umpire module is an empty stub while README/UI advertise umpire integration | `umpire-zone.ts:31` | Label as not yet active |

### What is demonstrably right (worth preserving)
Odds-conversion, EV, and Kelly formulas (canonical); strict YRFI = 1 − NRFI; PA-distribution renormalization; the Markov/Monte-Carlo base-out machine (verified state-by-state); seeded reproducible simulation; moist-air density physics; innings-pitched ".1/.2" parsing; point-in-time as-of stat construction with prior-season blending; chronological CV and held-out calibration in `train.py`; `ensemble7_nrfi` excluded from training features; graceful degradation and presence masks throughout.

### Bottom line
As a *statistical demo*, the architecture is thoughtful. As a *betting product*, it is not currently fit for purpose: the live odds pipe has never worked (C1), the headline pitcher input is synthetic with a directionally wrong adjustment (C2), the probability engine relies on a calibration layer to cancel a built-in 2× λ distortion (C3), and the backtest that would tell you whether any of this makes money is itself miscomputed (C4). Fixing C1–C4 and refitting the calibration on real-weather, out-of-sample data is the minimum bar before any capital is staked on its outputs.
