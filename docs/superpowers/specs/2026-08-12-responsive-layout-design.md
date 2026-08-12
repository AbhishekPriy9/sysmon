# Responsive Dashboard Layout

Date: 2026-08-12
Status: Approved

## Problem

The sysmon dashboard has a fixed layout. The CPU core grid is hardcoded to 2
columns, the Battery/Memory/Network cards are stacked vertically, and nothing
adapts when the window is resized. The user wants the layout to reflow with
window width.

## Approach

Use `gtk::FlowBox` for automatic width-based reflow. FlowBox wraps its
children into as many columns as fit the current width, on every resize,
with no manual width tracking or signals.

## Design

### CPU card

Replace the `gtk::Grid` in the CPU card with a `gtk::FlowBox`:

- `min_children_per_line = 1`, `max_children_per_line = 8`
- `homogeneous = true` so every core cell is equally wide
- each of the 8 core cells (label + progress bar + freq label) gets
  `size_request(170, -1)` for predictable wrapping: ~2/line at 480px,
  4/line at ~800px, 8/line when very wide
- `set_selection_mode(None)` and `set_activate_on_single_click(false)` so
  the cells do not get selectable/hover styling
- `hexpand = true` so the flow box fills the available width

### Stat cards

Wrap the Battery, Memory, and Network cards in a second `gtk::FlowBox`:

- `min_children_per_line = 1`, `max_children_per_line = 3`
- `homogeneous = true`
- `set_selection_mode(None)`, `set_activate_on_single_click(false)`
- each card `hexpand = true`

Narrow window: cards stack full-width. Wide window: cards sit side-by-side
(3-across).

### Hover tooltips

Abbreviated terms get a tooltip explaining what they mean, using
`set_tooltip_text`:

- Split the CPU summary into three labels — `Core X W`, `Pkg Y W`,
  `Temp Z °C` — each with its own tooltip:
  - Core: "Combined power draw of the CPU cores (RAPL core domain)"
  - Pkg: "Total CPU package power, incl. cores, cache, and GPU (RAPL
    package domain)"
  - Temp: "CPU package temperature"
- Per-core labels: "CPU load % and current clock frequency (MHz)"
- Battery: Health → "Battery capacity vs. its design capacity",
  Charging → "Current charging power draw", Discharging → "Current
  discharging power draw"
- Memory text: "RAM in use, swap usage, and zram compressed swap size"
- Network labels: ↑ → "Upload rate", ↓ → "Download rate"

### Unchanged

- Header toolbar (`adw::ToolbarView`)
- Outer vertical scroller
- Processes card with the Apps/Processes tab stack

## Files changed

- `src/ui.rs` — swap the CPU grid and stat-card layout for FlowBoxes,
  split the CPU summary into labeled terms, add hover tooltips.

## Testing

- `cargo build` and `cargo test` (existing 17 tests) still pass.
- Manual: resize the window; CPU cores and stat cards should reflow into
  more columns when widened and stack when narrowed.
- Manual: hover over Core/Pkg/Temp, battery, memory, and network labels to
  confirm tooltips appear.
