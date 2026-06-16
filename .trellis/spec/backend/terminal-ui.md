# Terminal UI Guidelines

This document captures the design and implementation contracts for the terminal‑based UI introduced in the **Adapt UI for Terminal Compatibility** task.

## Scope
- Applies to the `megaui` binary (`src/bin/megaui.rs`) and the UI module (`src/ui/mod.rs`).
- Focuses on rendering, interaction, and graceful degradation in terminal environments.

## Functional Contracts

1. **Header**
   - Displays `PROJECT_NAME` (`"MegaGate"`) in **bold** and **underlined** style.
   - Shows `AUTHOR` (currently `"doanmihh153"`) on the next line.
2. **Sidebar Menu**
   - Items: `Home`, `Settings`, `Logs`, `Help`, `Exit`.
   - The currently selected item is rendered with `Modifier::REVERSED`.
   - Navigation via `Up/Down` arrows or `j/k` keys.
3. **Console Output Area**
   - Shows log lines stored in `AppState.log`.
   - Lines containing `[INFO]` are colored **green**, `[WARN]` **yellow**, `[ERROR]` **red**.
   - Scroll offset (`console_offset`) is reserved for future scrolling support.
4. **Input Prompt**
   - Rendered at the bottom as a bordered `Paragraph` titled `Input`.
   - User input is collected in `AppState.input` and processed on `Enter`.
5. **System Info Panel**
   - Displays eight lines: CPU, RAM, SSD, GPU, Bandwidth, Cache, Public IP, Local IP.
   - Rendered as a **4 × 2 grid of bordered blocks**, each block containing one line.
   - Layout percentages: header 15 %, info 35 % (normal), footer 10 %, menu the rest.
   - In compact mode (terminal < 80 × 24) the info area expands to 30 % for readability.
6. **Responsive Layout**
   - Minimum terminal size: **80 × 24** characters.
   - If the size is smaller, a red warning `"Warning: Terminal size too small (min 80x24)."` is displayed and UI rendering stops.
   - Layout percentages adjust based on total height (header, info, footer, menu).
7. **Graceful Degradation**
   - If the terminal does not support colors, the UI will fall back to default terminal colors (no explicit fallback code needed because `crossterm`/`tui` automatically ignore unsupported colors).
   - No mouse input is processed – clicks are ignored.

1. **Header**
   - Displays `PROJECT_NAME` (`"MegaGate"`) in **bold** and **underlined** style.
   - Shows `AUTHOR` (currently `"doanmihh153"`) on the next line.
2. **Sidebar Menu**
   - Items: `Home`, `Settings`, `Logs`, `Help`, `Exit`.
   - The currently selected item is rendered with `Modifier::REVERSED`.
   - Navigation via `Up/Down` arrows or `j/k` keys.
3. **Console Output Area**
   - Shows log lines stored in `AppState.log`.
   - Lines containing `[INFO]` are colored **green**, `[WARN]` **yellow**, `[ERROR]` **red**.
   - Scroll offset (`console_offset`) is reserved for future scrolling support.
4. **Input Prompt**
   - Rendered at the bottom as a bordered `Paragraph` titled `Input`.
   - User input is collected in `AppState.input` and processed on `Enter`.
5. **Responsive Layout**
   - Minimum terminal size: **80 × 24** characters.
   - If the size is smaller, a red warning `"Warning: Terminal size too small (min 80x24)."` is displayed and UI rendering stops.
   - Layout percentages adjust based on total height (header, menu, footer).
6. **Graceful Degradation**
   - If the terminal does not support colors, the UI will fall back to default terminal colors (no explicit fallback code needed because `crossterm`/`tui` automatically ignore unsupported colors).
   - No mouse input is processed – clicks are ignored.

## Non‑Functional Requirements
- Uses only the `crossterm` and `tui` (ratatui) crates, both compatible with `no‑std` environments.
- Rendering latency is kept under **100 ms** by limiting redraws to a 150 ms event poll interval.
- All code compiles with the existing Cargo toolchain (`cargo build --bin megaui`).

## Validation & Tests (to be added)
- **Unit test**: Verify `MENU_ITEMS` length and order.
- **Integration test**: Launch the UI in a headless terminal (e.g., using `expect`), send navigation keys, and assert the selected index changes correctly.
- **Smoke test**: Run `cargo run --bin megaui` and confirm no panic on terminals of size 80×24 and larger.

## Design Decisions

- **Layout Choice**: Adopted a **4 × 2 grid of blocks** for the System Info panel to give each metric its own visual container, improving readability and enabling future per‑metric styling.
- **Info Panel Height**: Set `info_pct` to **35 %** (normal) and **30 %** (compact) to ensure all eight information lines are visible without scrolling.
- **Widget Library**: Chose `ratatui` for its lightweight widget model and built‑in support for ANSI colors.
- **Console Log Scrolling**: Kept the console log scroll offset simple (no paging) to satisfy the minimum functional requirement.
- **Overlay Enum**: Added a `Overlay` enum for future expansion (e.g., a Help overlay) but left it unused for now.

- Chose `ratatui` for its lightweight widget model and built‑in support for ANSI colors.
- Kept the console log scroll offset simple (no paging) to satisfy the minimum functional requirement.
- Added a `Overlay` enum for future expansion (e.g., a Help overlay) but left it unused for now.

---
*Generated by OpenCode after completing the terminal UI implementation.*