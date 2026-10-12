//! Agents as cells: mapping, diffing and polling of Pando data for the organism
//! (PDA-US-0014), plus the card, actions, creation form and target model of PDA-US-0015/16.
//!
//! * `mapping` - pure engine/status/label tables.
//! * `source` - pure poll-to-cells diff (birth, Delegating, 10 s removal, instances, pending).
//! * `poller` - the async 1 s / 5 s polling loop that feeds the source.
//! * `card` / `target` / `new_agent` - pure models of the side card, the voice target and the
//!   create-agent form; `actions` - the calls to Pando.
//! * `time` - RFC 3339 parsing.

pub mod actions;
pub mod card;
pub mod mapping;
pub mod new_agent;
pub mod pane;
pub mod poller;
pub mod source;
pub mod target;
pub mod time;
