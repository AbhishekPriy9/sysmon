# Tabbed Sysmon UI (Mockup A — Refined Adwaita)

Date: 2026-08-12
Status: Approved

## Problem

The sysmon dashboard currently shows every section (CPU, Battery, Memory,
Network, Processes) stacked on one scrolling page. The user provided a mockup
(`sysmon-mockup-a-refined-adwaita (1).html`) that redesigns the app as a
tabbed interface with five pages, card-style sections, boxed value rows, and
live values in the card headers.

## Approach

Rebuild the window in `src/ui.rs` around an `AdwToolbarView` that stacks the
existing `AdwHeaderBar` and an `AdwViewSwitcher` (policy `Wide`) over an
`AdwViewStack`. The view switcher drives the stack with no manual wiring —
clicking a tab switches pages automatically. All visual details come from
`GtkFlowBox` for the core grid, `GtkListBox` (theme `boxed-list` class) for
value rows, `AdwActionRow` for label/value pairs, `GtkStackSwitcher` for the
Apps/Processes toggle, and one small CSS provider that only uses Adwaita theme
CSS variables so the app follows the system light/dark preference with no
theme toggle.

## Design

### Window and navigation

- `adw::ApplicationWindow`, default size 864×640 (unchanged).
- `adw::ToolbarView`:
  - top bar 1: existing `adw::HeaderBar` with the freeze `GtkToggleButton`
    (`media-playback-pause-symbolic` ↔ `media-playback-start-symbolic`) and the
    centered window title "Sysmon".
  - top bar 2: `adw::ViewSwitcher` with `set_stack(&stack)` and
    `set_policy(ViewSwitcherPolicy::Wide)`.
  - content: `GtkScrolledWindow` containing the `adw::ViewStack`
    (`set_vhomogeneous(false)` so each page sizes to its own height).
- The `AdwViewStack` has 5 pages added with `add_titled_with_icon`:

  | Tab | Title | Icon (verified in Adwaita theme) |
  | --- | ----- | -------------------------------- |
  | CPU | CPU | `view-grid-symbolic` |
  | Battery | Battery | `battery-symbolic` |
  | Memory | Memory | `drive-harddisk-solidstate-symbolic` |
  | Network | Network | `network-wireless-symbolic` |
  | Process | Process | `view-list-symbolic` |

- No dark/light toggle anywhere in the app; the theme (and all CSS below)
  follows the system preference automatically via Adwaita CSS variables.

### Cards

`AdwPreferencesGroup` cannot render a trailing value next to its title, so the
cards are a small custom helper:

- `card(icon: &str, title: &str, trail: Option<&str>) -> (GtkBox, GtkBox, GtkBox)`
  returning the outer box, the body box, and the header box:
  - header (`GtkBox`, horizontal): `GtkImage` (icon, symbolic), `GtkLabel`
    (title uppercased via CSS `text-transform: uppercase`, dim, `.sysmon-card-title`),
    a trailing `GtkLabel` (`.sysmon-card-trail`, right-aligned, tabular) when
    `trail` is `Some`, and space for an end widget (used for the Processes
    toggle).
  - body (`GtkBox`, vertical): callers append card content.
- CPU trail shows the live average core load as `"NN% avg"`; Battery trail
  shows the charge percentage `"NN%"`. Other cards have no trail.
- The header box for the Processes card instead gets the Apps/Processes
  `GtkStackSwitcher` right-aligned.

### Value rows (mockup `.boxed-list .row`)

Rows are `AdwActionRow`s inside a `GtkListBox` with the theme's `boxed-list`
class:

- `row.set_title(label)` — dim label on the left (row title).
- `row.add_suffix(value_label)` — value on the right, `.sysmon-row-value`
  (semibold, `font-variant-numeric: tabular-nums`).
- `row.set_tooltip_text(...)` for the abbreviated terms, exact strings:

  | Row | Tooltip |
  | --- | ------- |
  | Core | "Combined power draw of the CPU cores (RAPL core domain)" |
  | Package | "Total CPU package power, incl. cores, cache, and GPU (RAPL package domain)" |
  | Temperature | "CPU package temperature" |
  | Health | "Battery capacity vs. its design capacity" |
  | Charging | "Current charging power draw" |
  | Discharging | "Current discharging power draw" |
  | Memory text | "RAM in use, swap usage, and zram compressed swap size" |
  | ↓ Download | "Download rate" |
  | ↑ Upload | "Upload rate" |
  | per-core freq | "CPU load % and current clock frequency (MHz)" |

### CPU page

- `GtkFlowBox` (min 1, max 4 children/line, homogeneous, selection off)
  inside a `.sysmon-card-body` container (card background, rounded, bordered):
  - each core chip: `GtkBox` vertical with `size_request(170, -1)`, CSS class
    `.sysmon-core-chip` (bordered, `--card-bg-color` background, rounded):
    - "CPU {i}" label (`.sysmon-core-name`),
    - slim `ProgressBar` (`.sysmon-core-bar`, no text),
    - "{load:.0}% · {freq} MHz" label (`.sysmon-core-freq`, tooltip above).
  - 170px min width yields 2-across at 480px window and 4-across at 864px.
- boxed summary list: Core / Package / Temperature rows. Values:
  `{w:.1} W` when the RAPL value is present, else "no access" for Core/Package
  and "—" for Temperature.
- RAPL hint: `.sysmon-warning` box (warning icon + label), hidden by default,
  shown only when `pkg_watts` is `None`. Exact text:
  "RAPL not readable — run: sudo udevadm control --reload-rules && sudo udevadm
  trigger --subsystem-match=powercap".

### Battery page

- body: green `ProgressBar` (`.sysmon-battery-bar` — `--success-bg-color` fill).
- boxed rows: Health (`"Health {:.0}%"`), Charging (`"Charging: {:.1} W"`),
  Discharging (`"Discharging: {:.1} W"`). When no battery: bar 0,
  "No battery" / "Charging: —" / "Discharging: —" (current behavior kept).

### Memory page

- body: accent `ProgressBar` (`.sysmon-mem-bar`).
- dim multi-line `GtkLabel` (`.sysmon-mem-text`, tooltip as above):
  `Used {} / {}\nSwap {} / {}\nZram {}` with existing `human_kb`.

### Network page

- boxed rows: `↓ Download` / value, `↑ Upload` / value with existing
  `human_bps`.

### Processes page

- card header end widget: `GtkStackSwitcher` over a `GtkStack` holding the two
  existing `build_apps_table()` / `build_procs_table()` TreeViews (unchanged:
  sortable headers, numeric sorting, grid lines, resizable columns).
- Tables keep their current store shape and `refill_table` behavior.

### Custom CSS

A single `gtk4::CssProvider` registered for the default display at
`STYLE_PROVIDER_PRIORITY_APPLICATION` in `build()`. All colors reference
Adwaita CSS variables (`--card-bg-color`, `--border-color`, `--success-bg-color`,
`--warning-bg-color`, `--window-fg-color`, `--dim-opacity`, `--accent-bg-color`,
`--accent-fg-color`) so light/dark follows the system. Rules:

- `.sysmon-card-title`, `.sysmon-card-trail`, `.sysmon-core-name`,
  `.sysmon-core-freq`, `.sysmon-mem-text` — size/weight/opacity/uppercase.
- `.sysmon-row-value` — semibold + tabular-nums.
- `.sysmon-core-chip`, `.sysmon-card-body` — border, radius, padding,
  `--card-bg-color`.
- `progressbar.sysmon-core-bar trough/progress`, `.sysmon-battery-bar
  trough/progress`, `.sysmon-mem-bar` — slim heights, rounded, colors.
- `.sysmon-warning` — warning-tinted background (`alpha(var(--warning-bg-color),
  0.15)`), `alpha(..., 0.4)` border, rounded.
- `.sysmon-freeze:checked` — accent background / accent foreground (freeze state).

### Freeze button

Keep the current `GtkToggleButton` behavior and tooltips; add the
`.sysmon-freeze` class so the toggled state uses the accent color.

## Files changed

- `src/ui.rs` — full rewrite of `build()`, the `Ui` struct, and `Ui::update`
  (new fields: `cpu_avg`, `batt_pct` trails; CPU rows split into
  Core/Package/Temperature value labels; chip/freq/store fields unchanged in
  purpose).

## Testing

- `cargo build` compiles with no warnings.
- `cargo test`: all 17 existing tests pass (UI has no test harness).
- Manual: window opens at 864px; the five tabs switch via the switcher; core
  chips reflow 2↔4 across; trails show live CPU %/battery %; tooltips appear on
  every abbreviated row; freeze toggles accent-colored; RAPL hint appears only
  when power readings are unavailable; Apps/Processes toggle switches tables;
  light/dark follows the system theme.
