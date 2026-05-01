# Quant roadmap status

Status against the supplied markdown roadmap.

## Completed

- P0 transaction cost model.
- P0 realistic backtest fee, slippage, market-impact, and annualization fixes.
- P1 walk-forward split and OOS robustness evaluation primitives.
- P1 multi-timeframe weighted vote aggregation.
- P1 OFI stream plumbing and strategy confidence confirmation.
- P2 IC/IR, IC decay, and permutation significance primitives.
- P2 volatility targeting, Kelly, correlation, exposure, VaR, and CVaR helpers.
- Phase 2 execution quality tracking and limit-order fill probability/planning.
- Phase 5 strategy retirement, A/B variant comparison, and parameter sensitivity helpers.
- Monte Carlo drawdown confidence intervals.

## Still intentionally pending

- HMM regime detector.
- Kalman trend estimation.
- BTC/ETH pairs trading and cointegration.
- Funding-rate arbitrage strategy.
- Alternative-data factor scoring.
- Deribit options IV-skew sentiment.
- Production CLI/reporting pipeline for automated research reports.

The remaining items require deeper data dependencies or new strategy workflows, so they should be delivered as focused PRs after the core validation/execution primitives are merged.
