# ARIA Runtime Testing

Use this skill when validating the crypto-scalper bot runtime after changes to signal, risk, brain, manager, execution, control commands, or configuration paths.

## Devin Secrets Needed

- `OPENROUTER_API_KEY` — Brain LLM provider key when using OpenRouter models.
- `MANAGER_API_KEY` — TraderManager LLM provider key when Manager uses a separate key.
- `BINANCE_API_KEY` and `BINANCE_API_SECRET` — only needed for live-mode validation. Do not request or use these for paper/dry-run testing.
- `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` — only needed when validating Telegram alerts or command-panel flows.

## Setup

- Repo path: `/home/ubuntu/repos/crypto-scalper`.
- The bot loads `.env` automatically from the repo root when run from the repo.
- For local visibility, use `ARIA_CONFIG_OVERLAY=config/aggressive.toml`; the overlay should keep `[mode] dry_run = true` and `run_mode = "paper"`.
- Never commit `.env`, runtime logs, test plans, or other local test artifacts.
- Browser recording is not useful for normal ARIA runtime checks because the app is a shell CLI; collect command output and logs instead.

## Static validation

Run these before opening or updating a PR:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --lib
cargo build --release
```

## Paper runtime validation

Prefer the built binary after `cargo build --release`; it avoids recompilation noise in runtime logs.

```bash
set -a; source .env; set +a
./target/release/aria 2>&1 | tee /tmp/aria-runtime.log
```

For bounded non-interactive validation, use:

```bash
set -a; source .env; set +a
timeout --signal=INT 300s ./target/release/aria 2>&1 | tee /tmp/aria-runtime.log
```

Expected healthy paper-mode progression after a few 1m candles with `config/aggressive.toml`:

1. Startup logs `paper mode`.
2. DataAgent starts with all configured timeframes, usually shown as `Timeframe { seconds: 60 }`, `300`, and `900` for `1m/5m/15m`.
3. Bootstrap completes for all configured symbols, possibly after Binance Vision fallback.
4. Status or logs show nonzero `signals`.
5. At least one `paper scout htf-aware scalp signal` or normal strategy signal appears.
6. At least one `risk: allowed signal` appears when a signal passes risk gates.
7. Brain logs `brain: analyzing risk-approved setup` and `brain: decision` with decision, confidence, fallback flag, and reason.
8. Manager logs `manager: reviewing brain-approved setup` and `manager verdict` with action and reason.
9. Execution logs `execution: position opened`.
10. Status eventually shows `fills > 0` and no survival death-line freeze.

Useful status fields in logs: `signals`, `risk_allowed`/`allowed`, `risk_blocked`/`blocked`, `brain_go`/`go`, `manager`, `vetoes`, `fills`, `last_signal`, `last_block`, `last_brain`, `last_manager`.

If fills remain zero, inspect `last_block` first; common blockers are reward/risk, net edge after costs, TA threshold, LLM fallback threshold, or manager veto/timeouts.

## Stdin command validation

Run interactively:

```bash
set -a; source .env; set +a
./target/release/aria 2>&1 | tee /tmp/aria-command.log
```

After startup shows `stdin control ready — type `help`, then press Enter`, type:

```text
help
status
positions
```

Expected command evidence:

- `help` prints commands including `status`, `positions`, `flat`, `freeze`, `unfreeze`, and `health`.
- `status` prints risk/equity state and triggers an on-demand monitor block beginning `ARIA STATUS (on-demand)`.
- `positions` prints either open paper/live position details or an explicit empty-position response; it should not hang silently.

## Suggested log assertions

Use `rg` or the `grep` tool against captured logs for:

```text
paper mode
data agent starting
Timeframe { seconds: 60 }
Timeframe { seconds: 300 }
Timeframe { seconds: 900 }
paper scout htf-aware scalp signal
risk: allowed signal
brain: decision
manager verdict
execution: position opened
ARIA STATUS (on-demand)
Open positions
```
