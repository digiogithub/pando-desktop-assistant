# Bitácora as the reference for pando-assistant (Rust + GPUI)

Analysed: `/www/Bitacora/bitacora` @ `dd39d1d` (v2.1.1), read-only. GPUI sources checked in
`~/.cargo/registry/src/*/gpui-pre-0.3.8`, `gpui-component-0.7.1`, `gpui-base-0.7.1`, `gpui-kit-0.7.1`.
All paths below are relative to the Bitácora repo unless absolute.

---

## 0. TL;DR

- **GPUI = `gpui-kit = "=0.7.1"` from crates.io** (Longbridge, Apache-2.0), which pins the weekly
  `gpui-pre =0.3.8` snapshot crates (snapshot of `zed@279fe07`). Not a git dep, not crates.io `gpui 0.2.2`.
  GPUI is reached only through `gpui_kit::*` / `gpui_kit::gpui`. Edition 2024, toolchain `1.98.1`, MSRV 1.90.
- **`crates/pando-rs` (lib `pando`) is a generic, Bitácora-free Pando SDK** (reqwest/rustls + tokio, own SSE parser,
  tolerant AG-UI event types, Thread reducer, HITL helpers, KB REST). It is explicitly designed to move to
  Pando `sdk/rust/` (ADR-027). **This is the #1 reuse candidate.**
- **Important protocol mismatch:** Bitácora chats over Pando's **AG-UI** adapter (`/api/v1/agui`, Bearer token),
  while the pando-assistant notes (`docs/04-pando-api.md:105`) say AG-UI is "not needed" and plan
  `/api/v1/chat/stream` + orchestrator endpoints. Decide early; the reusable Rust code is AG-UI only.
- Auth/discovery facts that resolve the `[verify]` in `docs/01-strategy.md` / `04-pando-api.md`:
  `pando serve` is **HTTPS-only with a private CA at `~/.config/pando/tls/ca.crt`**, the REST header is
  **`X-Pando-Token`**, and **`GET /api/v1/token` returns the token without credentials only on a loopback bind**.
  AG-UI uses `Authorization: Bearer`.
- GPUI has **no tray, no global hotkey, no "always on top" API** for normal windows; there is `WindowKind::PopUp`
  (topmost on macOS/Windows, override-redirect notification window on X11) and `WindowKind::Floating` (above its
  parent), plus Wayland `LayerShell`. Transparent windows: `WindowBackgroundAppearance::{Transparent, Blurred}`.
- Canvas: `canvas(prepaint, paint)`, `window.paint_path(Path, impl Into<Background>)`, `PathBuilder::{fill, stroke}`
  with `move_to/line_to/curve_to/cubic_bezier_to/arc_to/add_polygon/close/transform`, `paint_quad`,
  `paint_drop_shadows` (blurred glow), `paint_image`, `linear_gradient` (no radial), and
  `window.request_animation_frame()` (a `Window` method, not `cx`). Bitácora's graph view is a working template.

---

## 1. Workspace, toolchain, GPUI, UI kit, theming, fonts, icons

### 1.1 Layout and toolchain

| Item | Value | Where |
|---|---|---|
| Workspace | `resolver = "3"`, `members = ["crates/*", "xtask"]`, `[workspace.package] edition = "2024"`, `rust-version = "1.90"`, `publish = false`, version `2.1.1` | `Cargo.toml:1-12` |
| Toolchain | `channel = "1.98.1"`, `components = ["rustfmt","clippy"]`, `profile = "minimal"` | `rust-toolchain.toml` |
| Lints | `[workspace.lints.rust] unsafe_code = "warn"`, `missing_debug_implementations = "warn"`; clippy `unwrap_used/expect_used/dbg_macro = "deny"`, `todo = "warn"`; tests allowed via `clippy.toml` | `Cargo.toml:27-36`, `clippy.toml` |
| Profiles | dev `debug = "line-tables-only"`; `[profile.dev.package."*"] opt-level = 1` ("GPUI is sluggish at opt-level 0"); release `lto="thin"`, `codegen-units=1`, `strip="debuginfo"`, `panic="unwind"` | `Cargo.toml:14-25` |
| Deps policy | Exact pins in `[workspace.dependencies]`; members opt in with `foo.workspace = true`; `cargo machete` keeps it clean | `Cargo.toml:38+` |
| Crates (15) | `bitacora-{markdown,config,core,watch,index,merge,sync,mcp,graph,pando,runtime,app,cli,testkit}`, `pando-rs`, plus `xtask` | `crates/`, `AGENTS.md` §4 |
| Linker | optional mold via commented `.cargo/config.toml`; `cargo xtask` alias | `.cargo/config.toml` |

Dependency rule (enforced by `cargo xtask check-deps`, `xtask/src/deps.rs`): **only `bitacora-app` depends on
`gpui-kit`**; `pando-rs` depends on no bitacora crate; core stays sync (no tokio). The graph layout simulation
(`bitacora-graph`) is a GPUI-free leaf crate — same rule pando-assistant wants for `organism`/`decide`.

### 1.2 How GPUI is pulled in

```toml
# Cargo.toml (workspace)
gpui-kit = { version = "=0.7.1" }          # UI (only bitacora-app may use it; ADR-001)

# crates/bitacora-app/Cargo.toml
gpui-kit.workspace = true
[dev-dependencies]
gpui-kit = { workspace = true, features = ["test-support"] }
```

`Cargo.lock` resolves `gpui-kit 0.7.1` → `gpui-base 0.7.1`, `gpui-component 0.7.1`, `gpui-kit-assets 0.7.1`,
`gpui-pre 0.3.8` (+ `gpui-pre-{platform,linux,macos,windows,wgpu,apple,...} 0.3.8`, `gpui-pre-reqwest 0.12.15`).
`gpui-kit/src/lib.rs` documents the layering: `gpui_kit::*` = GPUI, `gpui_kit::base` (unstyled behaviour),
`gpui_kit::component` (styled, feature `component`, default on), `gpui_kit::assets` (Lucide icons, `assets`,
default on), `gpui_kit::platform`. gpui-kit features worth knowing: `test-support`, `inspector`, `profiler`,
**`speech`** (cpal + macOS/Windows system speech recognizer), `tree-sitter-*` (code highlighting), `decimal`.
gpui-pre default features: `font-kit`, `wayland`, `x11`, `windows-manifest`.

Rationale and risks are in `docs/analysis/rust/gpui-and-gpui-kit.md` (§1.6: why not crates.io `gpui 0.2.2` or a
Zed git dep). A scheduled `.github/workflows/gpui-kit-canary.yml` rewrites the pin to the next kit release, builds and
tests, and opens/updates an issue on failure.

### 1.3 The UI facade (recommended pattern)

`crates/bitacora-app/src/ui/mod.rs` is a thin facade: **only `src/ui/` may name `gpui_kit::`**, enforced by a unit
test (`ui/mod.rs:515 only_ui_names_gpui_kit`). Explicit re-exports (not a glob — a glob with `test-support` pulls in
GPUI's `#[test]` macro and shadows Rust's). Sub-modules: `button`, `switch`, `popover`, `menu`, `sidebar`,
`status_bar`, `progress`, `theme`, `calendar`, `dock`, `frameless`, `text_edit`, `canvas`, `a11y`, `drag`, `command`,
`table`, `input`, `assets`, `testing`. Helpers: `application()` (with kit assets), `init()`, `open_main_window()`
(kit `open_window` = window with a `Root` overlay host), `notify()` / `notify_action()` toasts (`ui/mod.rs:160`),
`confirm()` / `choose()` alert dialogs (`ui/mod.rs:230`), `set_menus()`, `key_binding()` (`ui/mod.rs:351`).

```rust
// ui/mod.rs:390-410
pub fn application() -> Application {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets)
}
pub fn init(cx: &mut App) { gpui_kit::init(cx); }
pub fn open_main_window<V: Render>(options: WindowOptions, cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> anyhow::Result<(AnyWindowHandle, Entity<V>)> {
    gpui_kit::open_window(options, cx, build)
}
// ui/mod.rs:426
pub mod canvas {
    pub use gpui_kit::gpui::{BorderStyle, Corners, Edges, MouseDownEvent, MouseMoveEvent,
        MouseUpEvent, Path, PathBuilder, ScrollWheelEvent, canvas, quad};
}
```

Toasts/dialogs silently degrade when the window has no `Root` (headless tests), and `confirm()` returns `false` so a
destructive action never proceeds unasked — copy this.

### 1.4 Components available (gpui-component 0.7.1 / gpui-base 0.7.1)

`gpui-component/src`: accordion, alert, attachment, avatar, badge, breadcrumb, **bubble**, button, carousel, chart,
checkbox, clipboard, collapsible, color_picker, combobox, command (palette), description_list, **dialog**, dock,
empty, form, group_box, highlighter, history, hover_card, icon, input (Input/Textarea/Editor), kbd, label, link,
list, marker, menu, **message**, **message_scroller**, native_menu, **notification**, pagination, plot, popover,
progress, **questionnaire**, radio, rating, resizable, root, scroll, searchable_list, select, separator, setting,
sheet, **shimmer**, sidebar, skeleton, slider, **speech** (SpeechState/SpeechButton/SpeechWaveform/Microphone),
spinner, status_bar, stepper, switch, tab, table, tag, **text** (TextView: Markdown/HTML), theme, time, title_bar,
toolbar, tooltip, tree, virtual_list, window_border.

Chat-relevant pieces Bitácora did **not** use but pando-assistant should evaluate:
- `gpui_base::text::TextViewState::markdown(text, cx)` + **`push_str(new_text, cx)`** for streaming appends
  (`gpui-base-0.7.1/src/text/state.rs:162,342`), `TextViewMotion` stream fade-in (`text/stream_fade.rs`), code
  blocks (`CodeBlock`), tables, selection, range highlights.
- `MessageScroller`/`MessageScrollerState` — virtualized `list` with `FollowMode::Tail` (auto-scroll to bottom)
  and a jump-to-bottom button (`gpui-component-0.7.1/src/message_scroller.rs`); `MessageGroup`/`Bubble`.
- `speech` module: `SpeechRecognizer`/`AudioInput` traits — **Linux has no system recognizer** (doc comment in
  `speech/mod.rs`), so a Whistle-backed `SpeechRecognizer` impl would plug in; `SpeechWaveform` for levels.

Bitácora's own design-system kit wraps these: `crates/bitacora-app/src/views/kit/` (`Button`, `Card`, `Chip`,
`IconButton`, `Segmented`, `PopoverShell` with fade animation, `Tab`, `Glyph`, gallery) — all `RenderOnce`
builders styled only from the theme global, with pure `*_spec()` functions unit-tested
(`views/kit/mod.rs:1-12`, `docs/design/component-kit.md`).

### 1.5 Theming / tokens

Two layers (`ui/theme/bitacora.rs`, `theme.rs`):
1. **Kit theme** (`gpui_kit::component::theme::{Theme, ThemeRegistry, ThemeMode}`) styles kit widgets; Bitácora
   ships "Bitacora Light/Dark" in `assets/themes/bitacora.json` loaded with
   `ThemeRegistry::global_mut(cx).load_themes_from_str(..)` (`theme.rs:62-70`).
2. **App theme global** `BitacoraTheme { mode, colors: Palette, type_scale: TypeScale, metrics: Metrics }`
   (`impl Global`), accessed via an extension trait `cx.bitacora()` to avoid clashing with kit `cx.theme()`;
   `TypeStyleExt::type_style()` applies family/size/weight/line-height.

**Token pipeline (ADR-032):** `design/tokens/tokens.json` → `cargo xtask tokens` generates
`ui/theme/palette.rs` (`// GENERATED ... Do not edit`), `ui/theme/scale.rs`, `assets/themes/bitacora.json`;
`--check` (CI staleness) and `--contrast` (WCAG 4.5:1) (`xtask/src/tokens.rs`). pando-assistant already has
`design-system/tokens.json` (top-level keys `color, typography, space, radius, stroke, motion, organism,
engines, status` vs Bitácora's `color, font, typography, space, radius, size, shadow`) — port `xtask/src/tokens.rs`
and add generators for `motion`/`organism`/`engines`/`status`.

Light/dark/system: `window.observe_window_appearance(...)` re-applies when preference is System (`app.rs:281`).
Reduce motion is an app setting wired to `App::set_reduce_motion` (GPUI has no OS reduce-motion detection);
`with_animation` respects it automatically, `request_animation_frame` does not (check it yourself).

### 1.6 Fonts

`crates/bitacora-app/src/fonts.rs`: TTFs `include_bytes!`'d and registered before the first window:

```rust
const EMBEDDED_FONTS: &[&[u8]] = &[ include_bytes!("../assets/fonts/AtkinsonHyperlegibleNext-Regular.ttf"), /* ... */ ];
pub fn register(cx: &mut App) {
    let fonts = EMBEDDED_FONTS.iter().map(|b| Cow::Borrowed(*b)).collect();
    if let Err(err) = cx.text_system().add_fonts(fonts) { tracing::warn!("cannot register the embedded fonts: {err}"); }
}
```

Use **static** TTFs per weight (Literata static files name themselves "Literata 36pt <Weight>" but share family
"Literata"). OFL licence texts are shipped as packager `resources` (`bitacora-app/Cargo.toml` packager section).
A Linux-only test asserts families resolve (`fonts.rs` tests; macOS creates its platform only on the main thread
and Windows DirectWrite `all_font_names` doesn't list memory fonts — hence `#[cfg_attr(not(linux), ignore)]`).

### 1.7 Icons / SVG

Lucide via `gpui-kit-assets` (`Icon::new(IconName::X)`, `ui::icon()`); raw SVG bytes via
`gpui_kit::assets::AllAssets.load("icons/<name>.svg")` (`ui/mod.rs:376`); custom paint via
`window.paint_svg(bounds, path, data, transform, color, cx)`. App/window icon: `packaging/icons/*`
(16–1024 PNG, `.icns`, `.ico`, SVG; `packaging/make-icons.sh`), Windows `.ico` embedded via `build.rs` +
`embed-resource` (GPUI loads icon resource 1), Linux via `.desktop` `StartupWMClass` matching
`WindowOptions::app_id = "es.digio.bitacora"`; X11 also has `WindowOptions::icon`.

---

## 2. Pando integration

### 2.1 Crates

```
bitacora-config (PandoSettings, validate_pando_url)  \
pando-rs (lib `pando`, generic SDK, no bitacora deps) --> bitacora-pando --> bitacora-runtime --> {app, cli}
bitacora-core / bitacora-index (read APIs)           /
```
Design: `docs/design/pando-integration.md`, `docs/design/ai-agents.md`, `docs/design/semantic-search.md`;
ADR-027..031 in `docs/architecture.md`.

### 2.2 `crates/pando-rs` — generic SDK (reuse as-is)

~2.2k LOC src + tests. Deps: `reqwest 0.13` (`default-features = false, features = ["json","query","rustls"]`),
`serde`, `serde_json`, `thiserror`. Dev: `axum`, `tokio`. **No eventsource crate**: own incremental `SseParser`.

| Module | Content |
|---|---|
| `config.rs` | `PandoConfig { base_url, token: Option<Token>, timeout(30s), connect_timeout(5s), stream_idle_timeout(90s; server keep-alive is 15s), root_certificate_pem }`; `Token` redacts `Debug` |
| `client.rs` | `PandoClient` (Arc, cheap clone, **two reqwest clients**: one with overall timeout, one for streams with only `read_timeout`), `info()` (`GET /health`), `list_models()` (`GET /api/v1/models`), **`fetch_api_token()` (`GET /api/v1/token`, loopback-only)**, `X-Pando-Token` header, status→`Error` mapping, extra root CA |
| `error.rs` | `NotConfigured(503)`, `Unauthorized(401/403)`, `Unreachable`, `Timeout`, `ReindexRunning`, `Server{status,message}`, `Decode`, `Protocol` (200 but not SSE), `Run{code,message}` |
| `agui/sse.rs` | `SseParser::push(&[u8]) -> Vec<String>` / `finish()`; handles CRLF, comments, multi-line `data:`, `[DONE]` |
| `agui/client.rs` | `AguiClient`: `info`, `healthz`, `run`/`run_agent` (POST `{path}/{agent}`, SSE), `run_text`, **`attach(thread)`** (GET `/threads/{id}/stream`, replay + live), `cancel_run`, `list_threads`, `thread_messages`, `delete_thread`; `RunStream::next()` (`client.rs:300`) pulls `resp.chunk()`; checks `Content-Type: text/event-stream` (proxy guard) |
| `agui/types.rs` | Tolerant `Event` enum: `RunStarted/Finished/Error`, `StepStarted/Finished`, `TextMessageStart/Content/End`, `ToolCallStart/Args/End/Result`, `StateSnapshot/Delta` (RFC 6902), `MessagesSnapshot`, `ActivitySnapshot`, `Reasoning*`, `Custom`, `Raw`, **`Unknown{raw}`**; `Message`, `MessageContent` (text or parts), `ToolCall`, `RunInput`, `Info`, `ThreadsPage` |
| `agui/thread.rs` | `Thread`: transcript + shared-state reducer, `send`, `resume(tool_call_id, payload)`, `attach`, `cancel`, `interrupts()`, `pending_tool_calls()`, `load_history`, `set_context` |
| `agui/hitl.rs` | Permission prompt (`pando_permission_request`, `PermissionRequest{tool_name, action, description, path, params, justification, require_explicit_approval, never_auto_approve}`) and `AskUserQuestion` (`QuestionRequest`); answer builders `approve/deny/answers`; **fail closed** |
| `kb/` | `KbClient`: `upsert`, `delete`, `search(SearchRequest)`, `reindex`, `embedding_models`, `test_embedding` over `/api/v1/remembrances/kb/...` |
| tests | mock axum server tests, `tests/conformance.rs` replays SSE fixtures shared byte-for-byte with Pando's TS SDK, `tests/live.rs` (`#[ignore]`, spawns `pando agui-serve --no-tls --no-token`) |

Typical use (from `pando-rs/README.md`):

```rust
let client = PandoClient::new(PandoConfig::new("https://127.0.0.1:8765")
    .with_token(token).with_root_certificate_pem(ca_pem))?;
let mut thread = Thread::new(client.agui());
let mut run = thread.send("Refactor the parser").await?;
while let Some(ev) = run.next().await { let _ = ev?; }        // already reduced into `thread`
for i in thread.interrupts() {
    if let Interrupt::Permission { tool_call_id, .. } = i {
        thread.resume(&tool_call_id, hitl::deny()).await?.drain().await?;
    }
}
```

**Gaps for pando-assistant:** no models for `/api/v1/chat/stream`, sessions, orchestrator/mesnada tasks,
notifications stream, instances, `/v1/systemone`. No auto-reconnect: `RunStream` yields one `Err` and ends;
`ChatSession` does not call `attach` to resume after a dropped stream (only on thread resume). Add a generic SSE
`EventStream<T>` on top of `SseParser` with reconnect/backoff + `Last-Event-ID` if Pando supports it.

### 2.3 `crates/bitacora-pando` — Bitácora-specific, partially reusable

| File | Reusable? | Notes |
|---|---|---|
| `credentials.rs` (405) | **Yes, with renames** | `SecretBackend` trait, `KeyringBackend` (keyring 4.2, service `bitacora`, accounts `pando/rest`, `pando/agui`), `MemoryBackend`, `PandoCredentials::resolve(kind) -> Option<(Token, TokenSource)>` (`credentials.rs:224`): env var first (`BITACORA_PANDO_{REST,AGUI}_TOKEN`), then keychain; env never written back; trace-log test asserts no secret leaks |
| `connection.rs` | Yes | `test_connection` (health + min version), `version_at_least`, `default_global_pando_config()` = `$XDG_CONFIG_HOME/pando/.pando.toml`, `kb_sharing()` reads `[Data] Directory` |
| `service.rs` (760) | Pattern | `PandoService::start(PandoOptions)` (`service.rs:217`): own 1-worker tokio runtime, endpoint resolution, `GET /health` probe every 15 s with exponential backoff up to 8× (`probe_delay`, `service.rs:457`), `PandoStatus` machine `Off/ConsentRequired/Connected{version}/Unavailable{reason}/Unauthorized/TooOld`, events over `std::sync::mpsc` |
| `events.rs`, `activity.rs` | Pattern | `EventSink`, bounded JSONL activity log (500 entries) |
| `managed/` (~2.1k) | Optional | Spawns/supervises `pando serve --host 127.0.0.1 --port P --agui-port Q` per graph: flock'd instance dir, adopt existing instance, orphan kill, `PR_SET_PDEATHSIG`, crash backoff 1→60 s, 5 crashes/10 min → Failed, reads token via `/api/v1/token`, trusts `~/.config/pando/tls/ca.crt` (`default_ca_path`, `managed/supervise.rs:208`), generated `.pando.toml` with AG-UI profiles. Windows unsupported (no Job Object). The assistant probably connects to the user's running Pando instead ("external" mode) |
| `agents/chat.rs` (1668) | **Extract core** | `ChatSession::spawn(&tokio::Handle, ChatDeps) -> (ChatHandle, mpsc::Receiver<ChatEvent>)` (`chat.rs:540`); `ChatHandle` (send/approve/deny/decide/decide_and_remember/cancel/close, usable from any thread); `ChatEvent` (History, RunStarted, MessageStart/TextDelta/MessageEnd, ReasoningDelta, ToolCall*, ApprovalRequested(card), AutoAnswered, Error{fatal}, RunFinished, Closed…); **`ChatModel::apply(&ChatEvent)`** (`chat.rs:1497`) folds events into a renderable transcript (messages, reasoning, tool calls w/ args+result+duration, approval cards with state); `AgentState::from_value` reads `model`, `tokenUsage`, `subAgents` from STATE_SNAPSHOT; `ModelChoices::from_info` (model selector). Coupled to Bitácora via `ContentGuard`, `EditApplier`/`propose_edit`, `ToolMemory` — make those trait seams with no-op defaults |
| `agents/approvals.rs` (375) | **Yes** | `PendingApprovals`: fail-closed approval book; only explicit `Decision::Approve` approves; `DenyReason::{Timeout(120 s), PanelClosed, ThreadSwitch, GraphSwitch, Quit, Cancelled, ServerExpired}`; injectable `Clock` |
| `agents/memory.rs`, `runs.rs` | Yes | remembered tool decisions; one-shot runs (`run_once`, budget then cancel) + tolerant JSON extraction from LLM output |
| `semantic/` | No (graph-specific) | block→KB document sync with ledger; shows KB usage (`POST/DELETE /api/v1/remembrances/kb/documents`, search with `path_prefix`) |

### 2.4 Endpoint/auth discovery (facts)

- Settings `PandoSettings` (JSON `pando.json` in platform config dir, atomic write): `enabled`, `mode
  (managed|external|off)`, `rest_url`, `agui_url`, `allow_remote`, features, per-graph consent
  (`docs/design/pando-integration.md` §2).
- URL policy `validate_pando_url` (`crates/bitacora-config/src/pando.rs:249`): http/https only, no credentials in
  the URL, loopback always allowed, remote requires `allow_remote` **and** https.
- `pando serve` → HTTPS only, private CA at `<pando config dir>/tls/ca.crt`, own API token served by loopback-only
  `GET /api/v1/token`; same cert+token for the `--agui-port` listener (§5.3). Min version assumed `1.2.0`.
- Bitácora never reads Pando's config for the token in external mode: user pastes it (keychain) or env var.
  For pando-assistant: try `GET /api/v1/token` on loopback first, then keychain/env.

### 2.5 Chat panel UI (`crates/bitacora-app/src/views/chat/`)

- `mod.rs` (1145): `ChatView` entity, `Phase {Idle, Starting, Live, Unavailable(String)}`, session
  **generation counter** (late events of an old session are ignored), thread list, model selector, context chips.
- **Event pump** (`mod.rs:547 pump_events`): std `mpsc::Receiver<ChatEvent>` → forwarding OS thread →
  `async_channel` → one `cx.spawn` foreground task that **drains bursts with `try_recv` and applies a batch, one
  `cx.notify()` per batch**:

```rust
self.events_task = Some(cx.spawn(async move |this, cx| {
    while let Ok(first) = rx.recv().await {
        let mut batch = vec![first];
        while let Ok(more) = rx.try_recv() { batch.push(more); }
        if this.update(cx, |this, cx| this.apply_events(generation, batch, cx)).is_err() { break; }
    }
}));
```

- `markdown.rs` (179): tiny streaming-safe block parser (headings, paragraphs, bullets, quotes, fenced code; an
  **unclosed fence renders as code** so a half-streamed answer doesn't flicker). Inline rendering reuses the
  outline's inline renderer (for `[[refs]]`). Layouts cached per message, recomputed only when its text changed.
  For pando-assistant, prefer kit `TextViewState::markdown` + `push_str`.
- `render.rs` (1413): `message_el`, `reasoning_row` (collapsed by default), `tool_group`/`tool_card` (collapsible
  mono card: status, name, arg summary, duration, JSON in/out), `approval_card` (permission / `propose_edit` diff /
  question with options, Apply/Deny, "remember"), `thread_list`, `composer_box` (kit `TextareaState`), `usage_el`
  (token usage), header with running sub-agents and amber `ACTIVITY_SNAPSHOT` progress line.
- `tools.rs` (pure helpers: arg summaries), `diff.rs` (line diff), `host.rs` (frontend tool seams `AppHost`).
- Close semantics: `close_session(DenyReason)` on panel close / thread switch / quit — all pending cards denied.

### 2.6 Semantic search

`bitacora-pando/src/semantic/search.rs` + `bitacora-runtime` hybrid search: lexical FTS5 + Pando KB search, degrades
to lexical when Pando is not `Ok`. Documents keyed `bitacora/<graph_id>/<block-uuid>`, blake3 content hash, durable
ledger. Not directly relevant to the assistant beyond showing `KbClient::search` usage.

### 2.7 What to extract into a shared crate

| Proposed crate | Source | Work |
|---|---|---|
| `pando-rs` (lib `pando`) | `crates/pando-rs` verbatim | Move to `pando/sdk/rust` (already planned, ADR-027) or git-dep/path; add chat-stream, sessions, orchestrator, notifications, systemone typed APIs + reconnecting SSE |
| `pando-agent` (GPUI-free) | `bitacora-pando/src/agents/{chat.rs (ChatSession, ChatHandle, ChatEvent, ChatModel, AgentState, ModelChoices), approvals.rs, memory.rs, runs.rs}` | Replace `ContentGuard`/`EditApplier`/`FrontendHost` with traits; drop `propose_edit` |
| `pando-connect` | `credentials.rs`, `connection.rs`, `service.rs` probe/status loop, `validate_pando_url`, `default_ca_path` | Rename env vars/keychain service; add loopback `/api/v1/token` discovery |
| `gpui-bridge` (app-local module) | `bitacora-app/src/tokio_bridge.rs`, `events.rs` (`EventPump`), `ui/mod.rs` facade skeleton | Copy (MIT, original impl of Zed's Apache-2.0 `gpui_tokio` idea) |
| xtask | `xtask/src/{tokens.rs,bundle.rs,deps.rs,release.rs}` | Adapt paths/names |

---

## 3. App architecture patterns

### 3.1 Startup (`crates/bitacora-app/src/app.rs`)

`main.rs`: multi-call (askpass) check → `update::init_velopack()` (`VelopackApp::build().run()` must run first) →
`app::run(Args)`. `run`: platform dirs (`directories::ProjectDirs::from("es","Digio","Bitacora")`, `paths.rs`),
logging (`tracing-appender` rolling files in cache/logs), **single instance** (`instance::acquire` — exclusive
`File::try_lock` + IPC forwarding of args to the running instance, `bitacora-runtime::instance`), migration, crash
handler, then:

```rust
ui::application().run(move |cx| { if let Err(e) = start(cx, &args, &dirs, services) { ...; cx.quit(); } });
// start():
ui::init(cx);                       // gpui_kit::init
crate::fonts::register(cx);
tokio_bridge::init(cx)?;            // tokio runtime as a GPUI Global
theme::install(cx, AppSettings::load(..), Some(settings_path));
keymap::load_with_user_report(cx, user_keymap.as_deref())?;
cx.on_action(|_: &Quit, cx| cx.quit());
crate::menus::install(cx, ..);      // cx.set_menus (macOS menu bar)
cx.on_window_closed(|cx, _| { if cx.windows().is_empty() && !keep_running_in_background { cx.quit() } }).detach();
cx.on_app_quit(|cx| { /* save, ordered shutdown on background_executor */ async move { .. } }).detach();
```

"Keep running in background" (window closed, process alive, MCP server alive) is implemented with a
`CurrentWorkspace(Entity<Workspace>)` Global (`app.rs:31`) — relevant for a tray-less assistant.
Smoke test (`--smoke-test`): `window.on_next_frame` twice then `cx.quit()`, with a 30 s watchdog
(`app.rs:230-250`); logs `window.gpu_specs()` (device, driver, `is_software_emulated`).

### 3.2 Entities, views, events

- Views are `Entity<T>` with `impl Render`; components `RenderOnce` builders. Domain events via
  `impl EventEmitter<E>` + `cx.emit` / `cx.subscribe` (e.g. `ChatViewEvent::Navigate`, `PageEvent`).
- Heavy state lives outside GPUI: `GraphSession` runs the runtime `Session` on **one std thread**
  (`session.rs`), control via channels, `SessionHandle::run(closure)` executes on that thread; events back through
  `async_channel` + `EventPump::attach(cx, rx, apply)` (`events.rs:22`) which holds the view weakly and stops when
  the view is released.
- Workspace layout uses kit `DockArea` with registered panels (`views/panels`, `register_panel`), persisted JSON.

### 3.3 Async & threads (ADR-010/012)

- `tokio_bridge.rs`: multi-thread tokio runtime (2–4 workers, `bitacora-tokio`) stored as GPUI `Global`;
  `spawn(cx, fut) -> Task<Result<R, JoinError>>` (`tokio_bridge.rs:102`) wraps the tokio `JoinHandle` in a GPUI
  background task with an `AbortOnDrop` guard → **dropping the GPUI Task aborts the tokio task**; `spawn_result`
  flattens to `anyhow`; `handle(cx)` for long-lived servers; runtime `shutdown_timeout(2 s)` on quit off the main
  thread. Tested with `#[gpui_test]` incl. cancellation.

```rust
pub fn spawn<Fut, R>(cx: &App, fut: Fut) -> Task<Result<R, JoinError>>
where Fut: Future<Output = R> + Send + 'static, R: Send + 'static {
    let join = cx.global::<TokioGlobal>().handle.spawn(fut);
    let guard = AbortOnDrop(join.abort_handle());
    cx.background_spawn(async move { let r = join.await; drop(guard); r })
}
```

- Clippy guard (`crates/bitacora-app/clippy.toml`): `disallowed-methods` = `tokio::spawn`,
  `Runtime::block_on`, `Handle::block_on` in the app crate.
- UI usage: `cx.spawn(async move |this, cx| { let r = tokio_bridge::spawn(cx, client.list_threads(..)).await; this.update(cx, ..) })`
  (`views/chat/mod.rs:864`); CPU/blocking work via `cx.background_spawn`; timers
  `cx.background_executor().timer(d).await`.
- Simulation threads: `bitacora-graph/src/worker.rs` — std thread, `Control` messages in
  (`SetParams/Pause/Reheat/Pin/Unpin`), `Snapshot { generation, positions }` out via `Arc<Mutex<_>>`; blocks on its
  channel when settled (0 CPU). The view calls `sync_snapshot()` in `render`.

### 3.4 Actions & keybindings

`actions!(bitacora, [ToggleLeftSidebar, ..., Quit])` via the kit macro re-exported from `ui` (`actions.rs`).
Bindings in embedded JSON `assets/keymaps/default.json` (`[{ "context": "Workspace && !Input", "bindings":
{"secondary-k": "bitacora::OpenSearch", "g g": "bitacora::GoGraph"} }]`; `secondary-` = cmd/ctrl; multi-stroke
supported), user keymap layered from `<config>/keymap.json` with `unbind`, actions built with
`cx.build_action(name, None)`, bad entries reported as a toast (`keymap.rs`). Views declare `.key_context("X")` and
`.on_action(cx.listener(Self::handler))`.

### 3.5 Settings persistence

`AppSettings` serde JSON at `<config_dir>/settings.json`, `load` tolerant (defaults on error), `save` via
`write_atomic` (temp file in same dir + rename) (`settings.rs:163-189`); separate files for `pando.json`, workspace
layout, recent graphs, keymap, activity JSONL, caches. Secrets only in OS keychain (`keyring 4.2`).

### 3.6 i18n

`rust-i18n 4.2` (same lib GPUI Kit uses → one locale switch): `rust_i18n::i18n!("assets/locales", fallback="en")`
(`lib.rs:14`), YAML per namespace `assets/locales/<ns>.<lang>.yml` (en, es, fr, zh), `t!("key", var = ..)`.
`i18n.rs`: `LANGUAGES`, `match_tag`, `resolve(setting)` with `sys-locale` and **English forced under `cfg!(test)`**.
pando-assistant notes propose Fluent `.ftl`; rust-i18n is the proven, kit-compatible option.

### 3.7 Windows

- Main window options (`views/title_bar.rs:121`): `TitlebarOptions { appears_transparent: true,
  traffic_light_position }`, `window_decorations: Some(Client)`, `app_owns_titlebar_drag: true`,
  `app_id: Some("es.digio.bitacora")`, `window_min_size`, `WindowBounds::Windowed(Bounds::centered(..))`.
  Custom 52 px `AppTitleBar` porting kit `TitleBar` logic + kit `window_border()` (Linux shadow ring, resize edges).
  Always branch on the **granted** `window.window_decorations()`; GNOME Wayland always ends up `Client`.
  Full API table: `docs/design/frameless-window.md` §1.
- Multiple windows: supported (`cx.windows()`, handle `.update(cx, |_, window, cx| ..)`, `window.activate_window()`);
  Bitácora reopens a window from a forwarded launch in background mode (`app.rs:355-395`).
- **GPUI 0.3.8 capabilities relevant to the mini orb window** (verified in source):
  - `WindowKind::{Normal, PopUp, AnchoredPopup, Floating, LayerShell (linux+wayland), Dialog}` (`gpui-pre/src/platform.rs:2653`).
    - `PopUp`: macOS `NSPopUpWindowLevel` non-activating panel, joins all Spaces; Windows `WS_EX_TOOLWINDOW|WS_EX_TOPMOST`;
      X11 `override_redirect` + `_NET_WM_WINDOW_TYPE_NOTIFICATION` (WM won't manage/focus it → keyboard input issues);
      Wayland: plain toplevel (no always-on-top exists in xdg-shell).
    - `Floating`: macOS `NSFloatingWindowLevel` (above normal windows); X11/Wayland: transient-for/parent of the main window only.
    - `LayerShell(LayerShellOptions)`: wlroots/KDE/COSMIC overlay layer (not GNOME).
  - `WindowBackgroundAppearance::{Opaque, Transparent, Blurred, MicaBackdrop, MicaAltBackdrop}` + `window.set_background_appearance(..)`.
  - `is_movable`, `is_resizable`, `is_minimizable`, `display_id`, `cx.displays()/primary_display()`,
    `window.start_window_move()`, `inactive_frame_interval` (throttles animation frames when inactive — set `None`
    for an always-animating orb, or keep throttling to save power).
  - **No system tray, no global hotkeys, no set-always-on-top at runtime** in GPUI. Bitácora has none either
    (it uses single-instance IPC + "keep running in background"). Use `tray-icon`/`ksni` and `global-hotkey` crates
    (on Linux `global-hotkey` is X11-only; Wayland needs the XDG GlobalShortcuts portal) — **[verify]** event-loop
    integration with GPUI (tray-icon needs a GTK loop on Linux; macOS needs the main thread).

### 3.8 Custom canvas painting (graph view = organism template)

`crates/bitacora-app/src/views/graph_view.rs` (1095 LOC) + `src/graph_view/{model,viewport,export}.rs`:

```rust
// render(): request frames only while something moves (graph_view.rs:895-912)
if self.wants_frames() { window.request_animation_frame(); }
let data = self.paint_data(cx);                 // snapshot of what to draw (owned, 'static)
div().relative().size_full()
  .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| this.pointer_down(rel(e.position), ..)))
  .on_mouse_move(cx.listener(..)).on_scroll_wheel(cx.listener(..))
  .child(canvas(move |b, _, _| { bounds.set(b); b },          // prepaint: remember bounds (Rc<Cell<Bounds>>)
                move |b, _, window, _| paint_graph(b, &data, window))
         .absolute().size_full())
  .child(labels)                                              // text as absolutely positioned divs

// paint (graph_view.rs:790-905)
window.paint_quad(fill(bounds, d.colors.bg));
let mut builder = PathBuilder::stroke(px(1.));                // batched edges, ≤2000 segments per path
for (a, b) in chunk { builder.move_to(*a); builder.line_to(*b); }
match builder.build() { Ok(path) => window.paint_path(path, color), Err(e) => tracing::debug!(..) }
window.paint_quad(quad(rect, px(r), color, border_w, border_color, BorderStyle::default()));  // circles
```

Lessons encoded there: interaction math takes positions relative to the canvas centre so it is testable without a
window; canvas size is only known at paint → request one more frame on first layout; settled = no frames (idle
0 CPU); export to PNG reuses the scene with `tiny-skia` (`graph_view/export.rs`). Performance doc:
5k nodes = 3.4 ms/tick simulation CPU; GPU paint cost unmeasured (`docs/design/performance-v2.md` §2.4, 2.7).

GPUI 0.3.8 paint API inventory (`gpui-pre-0.3.8/src/window.rs`): `paint_quad` (4550), `paint_path(Path<Pixels>,
impl Into<Background>)` (4621), `paint_drop_shadows`/`paint_inset_shadows` (4424/4460, blur radius → soft glow on
round quads), `paint_svg` (4849), `paint_image(bounds, .., Arc<RenderImage>, frame, grayscale)` (4919),
`paint_layer`, `with_content_mask`, `request_animation_frame` (2670; = `on_next_frame(|_, cx| cx.notify(view))`),
`on_next_frame` (2650). `PathBuilder` (`path_builder.rs`): `fill()`, `stroke(w)`, `dash_array`, `move_to`, `line_to`,
`curve_to` (quadratic), `cubic_bezier_to`, `arc_to`, `relative_arc_to`, `add_polygon`, `close`, `transform`,
`translate`, `scale`, `rotate`, `build() -> Result<Path>` (lyon tessellation into **u16 index buffers** — keep each
path well under 65k vertices; Bitácora chunks edges at 2000). Backgrounds: solid, `linear_gradient`,
`pattern_slash`, `checkerboard`; **no radial gradient**. Paths are MSAA-rendered on wgpu (`path_msaa_texture`).
`gpui::spring` (`SpringConfig`) and `gpui_base::motion::{Transition, transition}` exist for physics/transitions.

Note for `design-system/README.md` §6: it says `Model<Organism>` and `cx.request_animation_frame()`; in this GPUI it is
`Entity<Organism>` and `window.request_animation_frame()`.

### 3.9 Tests

- `#[gpui_test]` (re-export of `gpui_kit::test`) with `TestAppContext`, `cx.add_window_view(|_, cx| View::new(cx))`
  → `(Entity, &mut VisualTestContext)`, `cx.run_until_parked()`, `cx.executor().allow_parking()` when real threads
  (tokio) are involved, `window.simulate_next_frame(cx)` for frame callbacks. Example: `tokio_bridge.rs` tests,
  `views/sidebar.rs:1008`, `views/kit/tests.rs`, `views/chat/tests.rs` (431), `editor/tests/*`.
- Pure "spec" functions for look decisions, unit-tested without a window.
- `ui::testing::platform_text_system()` for real font shaping in tests (Linux only).
- GPUI-free crates tested separately in CI (`test-core`), with a guard that `gpui-kit` is absent from that graph.

---

## 4. Build, packaging, CI

| Area | Bitácora |
|---|---|
| Linux deps | `script/install-linux-deps.sh [--minimal]`: `gcc g++ clang cmake pkg-config mold libssl-dev libzstd-dev libdbus-1-dev` + GUI `libfontconfig-dev libfreetype-dev libwayland-dev libxkbcommon-dev libxkbcommon-x11-dev libx11-xcb-dev libxcb1-dev libvulkan1 mesa-vulkan-drivers libasound2-dev` |
| Packaging | **cargo-packager** config in `[package.metadata.packager]` of `crates/bitacora-app/Cargo.toml` (identifier `es.digio.bitacora`, icons, OFL resources, deb `depends` incl. `libvulkan1`, `mesa-vulkan-drivers | libvulkan-driver`, `libxkbcommon-x11-0`, `libwayland-client0`, `libasound2t64 | libasound2`; custom `.desktop` template with `StartupWMClass`; NSIS `install-mode = "both"`, languages En/Es). `cargo xtask bundle [--no-build] --formats ..` (`xtask/src/bundle.rs`) overlays signing from env only. Formats: `.deb`, AppImage, `.app`/`.dmg`, NSIS (+ MSI beta per changes doc), **Flatpak** (`packaging/flatpak/es.digio.bitacora.yml`, metainfo), install scripts `packaging/install/install.{sh,ps1}` |
| Signing | macOS: `APPLE_SIGNING_IDENTITY`, `APPLE_CERTIFICATE(_PASSWORD)`, notarization via `APPLE_ID/PASSWORD/TEAM_ID` or API key; reusable action `digiogithub/ci-actions/macos-signing-keychain@v1`. Windows: `WINDOWS_CERTIFICATE_THUMBPRINT` or `WINDOWS_SIGN_COMMAND` (Azure Trusted Signing) |
| Auto-update | `velopack =1.2.161` (`update/` module, `VelopackApp::build().run()` first in `main`), ADR-018 |
| Workflows | `ci.yml` (changes filter; checks: fmt, typos, machete, cargo-deny, `xtask check-deps`, fixtures verify, `tokens --check/--contrast`; test-core on ubuntu/mac/windows; clippy `-D warnings` on **macos-latest**; test-app matrix ubuntu-24.04/macos/windows with release build + smoke test under `xvfb-run` + lavapipe, `continue-on-error`), `build-app.yml`, `bundle.yml` (bundles + install/launch smoke tests per OS), `release.yml` (tag == workspace version via `xtask release-check`, linux x86_64/aarch64, macOS signed+notarized, Windows, flatpak, sha256sums), `audit.yml` (cargo-deny), `coverage.yml` (llvm-cov excluding app), `flatpak.yml`, `gpui-kit-canary.yml`, `ai-e2e.yml` (self-hosted runner with Pando), `pages.yml` |
| CI tricks | `actions-rust-lang/setup-rust-toolchain@v2` with `cache-shared-key` and `cache-save-if: main`; `rui314/setup-mold@v1`; `taiki-e/install-action` for cargo-packager/machete; Windows checkout with `core.autocrlf true` |
| Other | `deny.toml` (licence allow-list; blocks GPL from Zed crates), `typos.toml`, `rustfmt.toml` (`edition = "2024"`, `newline_style = "Unix"`), `.gitattributes` |

---

## 5. Pitfalls and lessons

1. **GPUI/Kit churn** (0.6 renamed everything, 0.7 changed the window entry point within a month). Mitigations in
   Bitácora: exact `=` pin, `ui/` facade + test, canary workflow, monthly upgrades. Any GPUI API knowledge from
   older Zed (`Model<T>`, `ViewContext`, `cx.request_animation_frame`) is stale.
2. **Executor mix-ups**: tokio futures on GPUI executors panic "no reactor running"; never `block_on` on the main
   thread → tokio bridge + clippy `disallowed-methods`.
3. **Linux rendering needs Vulkan** (wgpu). CI and agents only ever tested with Xvfb + Mesa **lavapipe** (software
   Vulkan, `VK_ICD_FILENAMES=.../lvp_icd.json`, `WAYLAND_DISPLAY` unset); GL fallback unverified; no real-GPU numbers.
   Deb depends on `libvulkan1` + a driver. Smoke tests use a frame watchdog because a compositor without output
   never delivers frames.
4. **Wayland**: GNOME has no xdg-decoration → client decorations forced; no always-on-top, no global hotkeys
   without portals; fractional scaling/IME vary per compositor. X11 `PopUp` = override-redirect.
5. **Frameless**: branch on granted decorations, don't draw controls on macOS, kit `TitleBar` fixed 34 px height, no
   rounded content masks (square window border), `on_close_window` Linux-only.
6. **IME** (if a custom text element is built): three bugs in GPUI `examples/input.rs` (relative marked selection,
   surrogate rounding, `character_index_for_point` assert) — `docs/analysis/rust/block-editor-spike-report.md` §4.2.
   Use kit `Input`/`Textarea` for the composer instead.
7. **Fonts in tests**: TestAppContext text system has no fonts; macOS platform only on main thread; DirectWrite
   doesn't list memory fonts.
8. **Accessibility**: AccessKit tree only built while AT is connected; elements need `.id()` + `.role()`; no live
   announcements, no OS reduce-motion → app setting + `App::set_reduce_motion`; `request_animation_frame` ignores it.
9. **Streaming UI cost**: batch deltas per frame (drain channel), cache layouts per message, only re-render the
   message that changed; an unclosed code fence must render as code.
10. **Approvals fail closed**: deny on timeout/close/quit; never deliver an answer after close; Pando's own approval
    timeout is unknown to the client (card timeout 120 s is a guess) — `docs/design/ai-agents.md` open questions.
11. **SSE**: separate reqwest client without overall timeout (only read/idle timeout 90 s vs 15 s heartbeat); check
    `Content-Type` (proxies answer 200 HTML); dropping a stream detaches but does not cancel the run (call
    `cancel_run`).
12. **Idle power**: remaining wakeups (50/100 ms polls) measured at 0.2 % CPU; settled canvases request no frames.
    An always-animating orb is the opposite — throttle when hidden/inactive (`inactive_frame_interval`), stop frames
    when minimized.
13. **Managed Pando** is Unix-only (no Windows Job Object); macOS watchdog unverified.
14. **Licensing**: copy only from Apache-2.0 `gpui*`/GPUI Kit, never Zed GPL crates (`editor`, `ui`, `theme`);
    `cargo deny` enforces.
15. `AGENTS.md` §9: GPUI worktrees cost 5–25 GB of `target/` each — clean them.

---

## 6. Recommendations for pando-assistant

1. Workspace: copy Bitácora's root `Cargo.toml` profile/lint blocks, `rust-toolchain.toml` (1.98.1),
   `rustfmt.toml`, `clippy.toml`, `deny.toml`, `typos.toml`, `script/install-linux-deps.sh` (+ `libasound2-dev`
   already there for cpal), and `xtask` (`check-deps`, `tokens`, `bundle`, `release`).
2. `gpui-kit = "=0.7.1"` only in `crates/app`; `ui/` facade with the "only ui names gpui_kit" test; add the canary
   workflow. Consider enabling kit `speech` only if its `cpal` version matches the one `voice-in` uses.
3. Depend on `pando-rs` (path/git dep now; publish or move to `pando/sdk/rust`), extend it with the
   non-AG-UI endpoints you need; extract `ChatSession/ChatModel/PendingApprovals` into a GPUI-free crate.
   If the assistant talks through AG-UI, the chat panel backend is ~80 % done.
4. Copy `tokio_bridge.rs`, `events.rs` (EventPump), the batching event pump of `views/chat/mod.rs`, `fonts.rs`,
   `i18n.rs` + rust-i18n YAML, `keymap.rs` + JSON keymap, `settings.rs` atomic JSON, `paths.rs`, the
   single-instance glue, `title_bar.rs` frameless options.
5. Organism: pure `crates/organism` (like `bitacora-graph`) stepping on its own thread or in `render` with `dt`,
   `canvas()` + `PathBuilder::fill()` per blob/cell (chunk paths), `paint_drop_shadows` for glow,
   `window.request_animation_frame()` gated by visibility/reduce-motion; tiny-skia raster fallback (already proven
   for PNG export).
6. Mini window: try `WindowKind::PopUp` + `WindowBackgroundAppearance::Transparent` +
   `window_decorations: Client` + `is_resizable: false`; on Wayland fall back to `LayerShell` where available and a
   normal toplevel on GNOME. Tray/hotkeys via external crates; spike their event-loop integration first.

---

## 7. File index (most useful)

- `Cargo.toml`, `rust-toolchain.toml`, `clippy.toml`, `crates/bitacora-app/clippy.toml`, `deny.toml`
- `crates/pando-rs/src/{client.rs,config.rs,error.rs,agui/{client.rs,sse.rs,types.rs,thread.rs,hitl.rs,patch.rs},kb/*}`, `crates/pando-rs/README.md`
- `crates/bitacora-pando/src/{credentials.rs,connection.rs,service.rs,events.rs,agents/{chat.rs,approvals.rs,memory.rs,runs.rs},managed/supervise.rs}`
- `crates/bitacora-app/src/{main.rs,app.rs,tokio_bridge.rs,events.rs,ui/mod.rs,ui/theme/*,theme.rs,fonts.rs,i18n.rs,keymap.rs,actions.rs,settings.rs,paths.rs,instance.rs,session.rs}`
- `crates/bitacora-app/src/views/{chat/*,graph_view.rs,title_bar.rs,kit/*}`
- `crates/bitacora-graph/src/worker.rs`
- `xtask/src/{tokens.rs,bundle.rs,deps.rs,release.rs}`, `.github/workflows/*.yml`, `packaging/*`, `script/install-linux-deps.sh`
- Docs: `docs/analysis/rust/{gpui-and-gpui-kit.md,crate-stack.md,block-editor-spike-report.md}`, `docs/design/{pando-integration.md,ai-agents.md,frameless-window.md,performance-v2.md,component-kit.md,design-tokens.md}`, `docs/architecture.md` (ADR log)
