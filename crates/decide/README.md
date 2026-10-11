# decide

Decision layer: typed questions, policy and a transport trait (no UI).

Part of the Pando Desktop Assistant workspace. See `AGENTS.md` §5 for the crate map.

## Pipeline

`cascade::run` (typed questions over a `DecisionTransport`) → `policy::policy`
(fail-closed trust policy) → `command::dispatch` (a `CommandExecutor` implemented by the
app over `pando-client`) → `log::DecisionLog` (`decisions.jsonl`). On any decision error
use `policy::Outcome::unavailable()` (fail-open to conversation).

## Evaluation

```sh
cargo run -p decide --bin decide-eval -- eval ~/.pando-assistant/decisions.jsonl --model jev-latest --base-url http://127.0.0.1:11434
cargo run -p decide --bin decide-eval -- compare baseline.jsonl candidate.jsonl
DECIDE_BASE_URL=http://127.0.0.1:11434 cargo test -p decide --test live_intent -- --ignored
```
