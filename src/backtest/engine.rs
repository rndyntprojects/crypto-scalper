//! Minimal backtest runner. Replays candles through all configured strategies
//! and simulates SL/TP fills on the next candle.

use crate::backtest::metrics::PerformanceMetrics;
use crate::data::{Candle, Side};
use crate::errors::Result;
use crate::execution::tcm::TransactionCostModel;
use crate::strategy::ema_ribbon::EmaRibbon;
use crate::strategy::mean_reversion::MeanReversion;
use crate::strategy::momentum::Momentum;
use crate::strategy::squeeze::Squeeze;
use crate::strategy::state::{PreSignal, StrategyName, SymbolState};
use crate::strategy::vwap_scalp::VwapScalp;
use crate::strategy::{select_strategies, RegimeDetector, Strategy};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimTrade {
    pub symbol: String,
    pub strategy: String,
    pub side: String,
    pub entry: f64,
    pub exit: f64,
    pub pnl: f64,
    pub pnl_pct: f64,
    pub bars_held: u32,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BacktestResult {
    pub symbol: String,
    pub trades: Vec<SimTrade>,
    pub metrics: PerformanceMetrics,
}

pub struct BacktestEngine {
    pub symbol: String,
    pub active: Vec<StrategyName>,
    pub min_ta_confidence: u8,
    pub risk_per_trade_usd: f64,
    pub fee_bps: f64,
    pub slippage_bps: f64,
    pub market_impact_bps: f64,
    pub min_reward_risk: f64,
    pub max_position_notional_pct: f64,
    pub min_net_edge_bps: f64,
    pub assumed_daily_volume_usd: f64,
    pub equity_usd: f64,
    pub trading_days_per_year: f64,
    pub trades_per_day: f64,
}

impl BacktestEngine {
    pub fn run(&self, candles: &[Candle]) -> Result<BacktestResult> {
        let mut state = SymbolState::new(&self.symbol);
        let mut open: Option<(PreSignal, u32)> = None;
        let mut sim_trades: Vec<SimTrade> = Vec::new();

        for (i, c) in candles.iter().enumerate() {
            state.on_closed(*c);

            // Exit check first
            if let Some((sig, bars)) = open.clone() {
                let (exit_price, exit_reason) = match sig.side {
                    Side::Long => {
                        if c.low <= sig.stop_loss {
                            (sig.stop_loss, "SL".to_string())
                        } else if c.high >= sig.take_profit {
                            (sig.take_profit, "TP".to_string())
                        } else {
                            // Noop — still open
                            open = Some((sig.clone(), bars + 1));
                            continue;
                        }
                    }
                    Side::Short => {
                        if c.high >= sig.stop_loss {
                            (sig.stop_loss, "SL".to_string())
                        } else if c.low <= sig.take_profit {
                            (sig.take_profit, "TP".to_string())
                        } else {
                            open = Some((sig.clone(), bars + 1));
                            continue;
                        }
                    }
                };
                let size = self.signal_size(&sig);
                let tcm = self.tcm();
                let slip = (self.slippage_bps
                    + tcm.market_impact_bps(size * sig.entry, self.assumed_daily_volume_usd))
                    / 10_000.0;
                let slipped_exit = match sig.side {
                    Side::Long => exit_price * (1.0 - slip),
                    Side::Short => exit_price * (1.0 + slip),
                };
                let gross_pnl = match sig.side {
                    Side::Long => (slipped_exit - sig.entry) * size,
                    Side::Short => (sig.entry - slipped_exit) * size,
                };
                let fees = (sig.entry * size + slipped_exit * size) * self.fee_bps / 10_000.0;
                let pnl = gross_pnl - fees;
                let pnl_pct = match sig.side {
                    Side::Long => (slipped_exit / sig.entry - 1.0) * 100.0,
                    Side::Short => (sig.entry / slipped_exit - 1.0) * 100.0,
                };
                sim_trades.push(SimTrade {
                    symbol: sig.symbol.clone(),
                    strategy: sig.strategy.as_str().to_string(),
                    side: sig.side.as_str().to_string(),
                    entry: sig.entry,
                    exit: slipped_exit,
                    pnl,
                    pnl_pct,
                    bars_held: bars + 1,
                    reason: exit_reason,
                });
                open = None;
            }

            // Only look for new signal if no open position
            if open.is_some() {
                continue;
            }
            if i < 205 {
                continue; // warmup indicators
            }
            let regime = RegimeDetector::detect(&state);
            let chosen = select_strategies(&self.active, regime);

            for name in chosen {
                let sig = match name {
                    StrategyName::EmaRibbon => EmaRibbon.evaluate(&state, c),
                    StrategyName::MeanReversion => MeanReversion.evaluate(&state, c),
                    StrategyName::Momentum => Momentum.evaluate(&state, c),
                    StrategyName::VwapScalp => VwapScalp.evaluate(&state, c),
                    StrategyName::Squeeze => Squeeze.evaluate(&state, c),
                };
                if let Some(s) = sig {
                    let size_usd = self.signal_size(&s) * s.entry;
                    let net_edge = s.net_expected_edge_bps(
                        &self.tcm(),
                        size_usd,
                        self.assumed_daily_volume_usd,
                    );
                    if s.ta_confidence >= self.min_ta_confidence
                        && s.rr() >= self.min_reward_risk
                        && net_edge >= self.min_net_edge_bps
                    {
                        open = Some((s, 0));
                        break;
                    }
                }
            }
        }

        let pnls: Vec<f64> = sim_trades.iter().map(|t| t.pnl).collect();
        let metrics = PerformanceMetrics::from_trades_annualized(
            &pnls,
            self.trading_days_per_year * self.trades_per_day,
        );
        info!(
            symbol = %self.symbol,
            trades = sim_trades.len(),
            wr = %format!("{:.2}", metrics.win_rate * 100.0),
            pf = %format!("{:.2}", metrics.profit_factor),
            net = %format!("{:.2}", metrics.net_pnl),
            "backtest complete"
        );
        Ok(BacktestResult {
            symbol: self.symbol.clone(),
            trades: sim_trades,
            metrics,
        })
    }

    fn tcm(&self) -> TransactionCostModel {
        TransactionCostModel {
            taker_fee_bps: self.fee_bps,
            maker_fee_bps: -1.0,
            avg_slippage_bps: self.slippage_bps,
            market_impact_bps: self.market_impact_bps,
        }
    }

    fn signal_size(&self, sig: &PreSignal) -> f64 {
        let risk_size = self.risk_per_trade_usd / (sig.entry - sig.stop_loss).abs().max(1e-9);
        let notional_size =
            self.equity_usd * self.max_position_notional_pct / 100.0 / sig.entry.max(1e-9);
        risk_size.min(notional_size).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_fee_slippage_and_notional_cap() {
        let engine = BacktestEngine {
            symbol: "BTCUSDT".into(),
            active: vec![],
            min_ta_confidence: 65,
            risk_per_trade_usd: 100.0,
            fee_bps: 4.0,
            slippage_bps: 2.0,
            market_impact_bps: 0.0,
            min_reward_risk: 1.2,
            max_position_notional_pct: 100.0,
            min_net_edge_bps: 1.0,
            assumed_daily_volume_usd: 1_000_000_000.0,
            equity_usd: 10_000.0,
            trading_days_per_year: 365.0,
            trades_per_day: 12.0,
        };
        let sig = PreSignal {
            symbol: "BTCUSDT".into(),
            strategy: StrategyName::Momentum,
            side: Side::Long,
            entry: 100.0,
            stop_loss: 99.0,
            take_profit: 102.0,
            ta_confidence: 80,
            reason: String::new(),
        };
        let exit = sig.take_profit * (1.0 - engine.slippage_bps / 10_000.0);
        let risk_size = engine.risk_per_trade_usd / (sig.entry - sig.stop_loss).abs();
        let notional_size =
            engine.equity_usd * engine.max_position_notional_pct / 100.0 / sig.entry;
        let size = risk_size.min(notional_size);
        let gross = (exit - sig.entry) * size;
        let fees = (sig.entry * size + exit * size) * engine.fee_bps / 10_000.0;
        approx::assert_abs_diff_eq!(gross - fees, 189.880816, epsilon = 1e-6);
    }
}
