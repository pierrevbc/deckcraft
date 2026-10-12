# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Read [AGENTS.md](AGENTS.md) first: its rules (asset policy, clean room, never crash) bind you and win over anything here.

## Repositories & Remotes

- **Fork (for edits and pushes):** `https://github.com/yefan911/deckcraft`
- **Original upstream repo:** `https://github.com/storytold/deckcraft`
- Keep the original upstream repository intact while directing edits and branches to the fork (`https://github.com/yefan911/deckcraft`).

## Start Every Session

1. Read `plan/STATUS.md`, then the current milestone in `plan/execution-plan.md` (when `plan/` is present locally; `plan/` is gitignored). Otherwise, check [ROADMAP.md](ROADMAP.md) and pick tasks from the top of [docs/gaps.md](docs/gaps.md).
2. Read `../../craftrules/AGENTS.md` (if checked out beside the parent directory) and the standards it lists for your task.
3. Run `cargo xtask ci` before changing anything to verify the tree starts green.

## Common Commands

### Quality Gates & Verification
```sh
cargo xtask ci                  # Full gate: fmt --check, clippy -D warnings, test, assets, layers, wasm
cargo xtask assets              # Verify all assets in assets/, docs/images/, docs/brand/ are in ATTRIBUTION.md
cargo xtask layers              # Enforce crate dependency layering (xtask/src/layers.rs)
cargo xtask wasm                # cargo check --target wasm32-unknown-unknown on L0-L5 crates + ui-egui
cargo xtask parity              # Recompute feature-parity summary in docs/parity-checklist.md
cargo xtask stats [--exact]     # Count lines of code and tests per crate
```

### Build, Lint & Format
```sh
cargo fmt --all                 # Format all workspace crates
cargo fmt --all -- --check      # Check formatting
cargo clippy --workspace --all-targets -- -D warnings
```

### Running Tests
```sh
cargo test --workspace                          # Run all unit and integration tests
cargo test -p deckcraft-engine                  # Run tests in a single crate
cargo test -p deckcraft-engine <test_name>      # Run a single test by name
cargo test -p deckcraft-pptx --test roundtrip   # Run a specific integration test binary
```

### Running & Inspecting the Application
```sh
# Launch desktop app with built-in sample deck and localhost JSON control server on port 7990
cargo run --release -p deckcraft -- --sample --control 7990

# Drive a running instance over the control port (or inspect/screenshot UI changes)
cargo run -p deckcraft-cli -- app --port 7990 slide.new '{"layout":"titleOnly","title":"Hi"}'
cargo run -p deckcraft-cli -- app --port 7990 --method ui.screenshot '{"path":"/tmp/shot.png"}'

# Headless CLI rendering, command execution, and introspection
cargo run --release -p deckcraft-cli -- render --sample --all --scale 1 out/
cargo run -p deckcraft-cli -- run --sample --cmd 'slide.new={"title":"Hi"}' --export out.png
cargo run -p deckcraft-cli -- commands [filter]
cargo run -p deckcraft-cli -- describe <command_id>

# MCP server (headless stdio, or connected to a running desktop app)
cargo run -p deckcraft-cli -- mcp
cargo run -p deckcraft-cli -- mcp --connect 7990
```

## Architecture & Code Structure

### Strict Layered Crate Graph
`cargo xtask layers` (defined in `xtask/src/layers.rs`) enforces a strict downward-only dependency DAG across `crates/*`. Crates may only depend on strictly lower layers, and no UI toolkit crates (`egui`, `eframe`, `winit`, `rfd`) may be used below **L6**. Any new workspace crate must be registered in `xtask/src/layers.rs`.

| Layer | Crates | Role |
|---|---|---|
| **L0** | `geom`, `color`; standalone `bitstream`, `matroska`, `ogg`, `opus` | EMU/pt units, 2D transforms (`Xfrm`), ~150 ECMA-376 preset shape generators, adjust handles, hit testing; RGB/HSL and DrawingML color transforms; pure-Rust container demuxers and Opus audio decoder. |
| **L1** | `model`, `fonts`; `h264`, `hevc`, `vp9`, `av1`, `isobmff` | Core data model (`Presentation`, masters, layouts, slides, `Shape` trees, `TextBody`, themes, tables, charts, transitions, animations, placeholder inheritance in `resolve`); font discovery (`../craft-fonts` via `CRAFT_FONTS_DIR` + system fallback + metric-compatible Office substitutes); pure-Rust video decoders and MP4/MOV demuxer. |
| **L2** | `text`, `anim`, `format`, `media` | Text shaping (`harfrust`), UAX #9 bidi, UAX #14 line breaking, autofit, caret hit testing; animation timeline evaluation, slide show state machine, transition quad geometry, Morph matching; native `.deckcraft` (zip + `presentation.json` + `media/`) and outline I/O; media probing, decoding, audio mixing, and playback clock. |
| **L3** | `render`, `pptx` | CPU slide rasterizer (`vello_cpu`) producing RGBA pixmaps for slides, masters, layouts, charts, and Morph frames; lenient ECMA-376 `.pptx` reader and writer (unmodeled XML round-trips via `ShapeKind::Opaque` and `raw_ext` fields). |
| **L4** | `pdf` | PDF export (`krilla`) combining print-resolution slide rendering with an invisible selectable text overlay and embedded font subsets. |
| **L5** | `engine` | `Session` and `DocState` management, copy-on-write `Arc<Presentation>` undo/redo history, the command registry (`crates/engine/src/cmd/*`), pointer tools (`tools.rs`), keyboard handling (`keys.rs`), AutoRecover (`recovery.rs`), and the panic guard (`guard.rs`). |
| **L6** | `ui-egui`, `mcp` | `SlideApp` egui frontend (ribbon, canvas, thumbnails, panes, dialogs, slide show & presenter viewports, JSON-lines control channel in `control.rs`); stdio MCP server (`Headless` and `Remote` backends). |
| **Apps** | `apps/deckcraft`, `apps/deckcraft-cli`, `apps/deckcraft-web`, `xtask` | Exempt from layering. Desktop shell (`eframe`/wgpu, `cpal` audio, `rfd` dialogs, logging; defaults to DX12 on Windows), headless CLI, Trunk/WASM web app, and workspace build tooling. |

### Key Design Patterns

- **Everything is a Command (`crates/engine/src/cmd/*`):** Every user-visible action is defined as a `CommandSpec` (using the `cmd!`, `cmd!(query ...)`, or `cmd!(noundo ...)` macros in `crates/engine/src/cmd/mod.rs`) with a PowerPoint-aligned id (`slide.new`, `format.bold`, `arrange.group`, `transition.set`), label, menu/ribbon path, shortcut, parameter documentation, `enabled` predicate, and `run` handler, accompanied by unit tests.
- **Unified Dispatch (`Session::execute`):** The egui UI, `deckcraft-cli`, the localhost JSON control channel (`docs/control-protocol.md`), and the MCP server (`docs/mcp.md`) all execute commands through `Session::execute`. Pointer gestures go through `Session::pointer` and keys through `Session::key`.
- **Copy-on-Write State & Undo (`Session::edit`):** Document mutations run inside `Session::edit(|doc, sel| ...)`, which clones the inner `Presentation` via `Arc`, automatically reroutes attached connectors (`connect::reroute`), increments `DocState::revision`, clamps selection validity, and records an undo snapshot for top-level undoable commands.
- **Thin UI & Host Services (`crates/ui-egui`):** UI panels read engine state and trigger commands via `SlideApp::run`. Engine commands never block on OS dialogs; instead they push `UiRequest` variants (`Dialog`, `Pane`, `StartShow`, `PickFile`, `Media`) onto `Session::ui_requests` for the UI shell to fulfill using platform callbacks injected via `Services`.

## Non-Negotiable Rules & Habits

- **Never Crash (`AGENTS.md` §3):** Every production crate enforces `#![forbid(unsafe_code)]` and `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`. Use `Result<T, E>` and `?`, `.get()` instead of `[i]`, checked arithmetic, no NaN casts, capped allocations, and bounded recursion on all external/file/command inputs. `Session::execute` wraps commands in a panic guard (`crates/engine/src/guard.rs`), which is a last-resort safety net, not a license to panic. Every crash fix must land with a regression test.
- **Asset & Clean-Room Policy (`AGENTS.md` §1–§2):** Never use, trace, or imitate any asset (icons, fonts, themes, templates, screenshots, sample files) from Microsoft, Adobe, Avid, or Autodesk products (including Adobe Source Sans/Serif/Han and Noto CJK). Every committed or bundled asset must have a provable open license and an entry in `ATTRIBUTION.md` (`cargo xtask assets`). Never copy GPL/AGPL/LGPL/MPL code.
- **Never Break WASM:** All L0–L5 crates and `deckcraft-ui-egui` must compile for `wasm32-unknown-unknown` (`cargo xtask wasm`).
- **Look at UI Changes:** When modifying UI or rendering, visually verify the result using `deckcraft --sample --control 7990` + `ui.screenshot` or `deckcraft-cli render`.
- **Keep Progress Docs Current:** When a feature or fix lands, update `ROADMAP.md`, `docs/gaps.md`, `docs/parity-checklist.md` (and run `cargo xtask parity`), relevant `docs/*-parity.md` files, and `plan/STATUS.md`.
- **Parallel Agents:** Use a separate `CARGO_TARGET_DIR` per agent and only edit crates assigned to you.
