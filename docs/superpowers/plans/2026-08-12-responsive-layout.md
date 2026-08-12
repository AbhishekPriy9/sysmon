# Responsive Dashboard Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the sysmon dashboard reflow with window width (CPU cores and stat cards wrap into more columns when wide) and add hover tooltips explaining abbreviated terms.

**Architecture:** Two GTK4 changes, both in `src/ui.rs`. (1) Replace the fixed 2-column CPU `GtkGrid` and the stacked Battery/Memory/Network cards with `gtk::FlowBox` containers, which automatically reflow children per line as the window width changes — no manual resize handling. (2) Split the single CPU summary label into three labels (Core/Pkg/Temp) and add `set_tooltip_text` to every abbreviation-bearing label.

**Tech Stack:** Rust edition 2024, GTK4 0.11.4 (`v4_6` feature), libadwaita 0.9.2 (`v1_1`, `v1_5`, `gtk_v4_6`).

## Global Constraints

- Rust edition 2024; do not add new dependencies.
- Only `src/ui.rs` is modified.
- Tooltip texts must match the spec verbatim (exact strings in Task 2).
- `cargo build` must compile and all 17 existing tests must pass.
- No code comments unless the codebase style already has them (it does not).
- UI has no automated test harness; verification is `cargo build` + `cargo test` + manual run.

---

### Task 1: Reflow the dashboard with GtkFlowBox

**Files:**
- Modify: `src/ui.rs` (CPU card section in `build()`, and stat-card section in `build()`)

**Interfaces:**
- Consumes: existing `card()` helper (returns `(adw::PreferencesGroup, gtk4::Box)`); existing `core_load`/`core_freq` Vecs; existing widget creation code.
- Produces: `Ui` struct unchanged in this task. `build()` now appends cards to two `gtk::FlowBox` containers. Later tasks rely on `cbox` (CPU card inner box) still existing and the stat cards (`bg`, `mg`, `ng`) being appended to `stat_flow`.

- [ ] **Step 1: Replace the CPU `GtkGrid` with a `GtkFlowBox`**

In `src/ui.rs` inside `build()`, replace this block:

```rust
    // CPU card
    let (cg, cbox) = card("CPU");
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..8 {
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let name = gtk4::Label::new(Some(&format!("CPU{i}")));
        name.set_xalign(0.0);
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        let freq = gtk4::Label::new(Some("—"));
        freq.set_xalign(0.0);
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        grid.attach(&v, i % 2, i / 2, 1, 1);
        core_load.push(bar);
        core_freq.push(freq);
    }
    cbox.append(&grid);
```

with:

```rust
    // CPU card
    let (cg, cbox) = card("CPU");
    let flow = gtk4::FlowBox::new();
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(8);
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_activate_on_single_click(false);
    flow.set_hexpand(true);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..8 {
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        v.set_size_request(170, -1);
        let name = gtk4::Label::new(Some(&format!("CPU{i}")));
        name.set_xalign(0.0);
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        let freq = gtk4::Label::new(Some("—"));
        freq.set_xalign(0.0);
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        flow.append(&v);
        core_load.push(bar);
        core_freq.push(freq);
    }
    cbox.append(&flow);
```

- [ ] **Step 2: Wrap the Battery, Memory, and Network cards in a `GtkFlowBox`**

In `src/ui.rs` inside `build()`, the three stat cards are currently built separately and each ends with `root.append(&bg);` / `root.append(&mg);` / `root.append(&ng);`. Replace all three `root.append(&...)` calls for those cards with a single flow box appended to `root`, right after the CPU card's `root.append(&cg);`:

```rust
    root.append(&cg);

    // Battery card
    let (bg, bbox) = card("Battery");
    let bat_bar = ProgressBar::new();
    bbox.append(&bat_bar);
    let bat_health = gtk4::Label::new(Some("—"));
    bat_health.set_xalign(0.0);
    bbox.append(&bat_health);
    let bat_charge = gtk4::Label::new(Some("Charging: —"));
    bat_charge.set_xalign(0.0);
    bbox.append(&bat_charge);
    let bat_discharge = gtk4::Label::new(Some("Discharging: —"));
    bat_discharge.set_xalign(0.0);
    bbox.append(&bat_discharge);

    // Memory card
    let (mg, mbox) = card("Memory");
    let mem_bar = ProgressBar::new();
    mbox.append(&mem_bar);
    let mem_text = gtk4::Label::new(Some("—"));
    mem_text.set_xalign(0.0);
    mbox.append(&mem_text);

    // Network card
    let (ng, nbox) = card("Network");
    let net_down = gtk4::Label::new(Some("↓ —"));
    net_down.set_xalign(0.0);
    nbox.append(&net_down);
    let net_up = gtk4::Label::new(Some("↑ —"));
    net_up.set_xalign(0.0);
    nbox.append(&net_up);

    // Stat cards reflow side-by-side when the window is wide
    let stat_flow = gtk4::FlowBox::new();
    stat_flow.set_min_children_per_line(1);
    stat_flow.set_max_children_per_line(3);
    stat_flow.set_homogeneous(true);
    stat_flow.set_selection_mode(gtk4::SelectionMode::None);
    stat_flow.set_activate_on_single_click(false);
    stat_flow.append(&bg);
    stat_flow.append(&mg);
    stat_flow.append(&ng);
    root.append(&stat_flow);
```

Do NOT move the widget-creation lines for the cards elsewhere — they stay where they are; only the `root.append` calls for `bg`/`mg`/`ng` are removed and replaced by the `stat_flow` block above. The Processes card section is unchanged.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: compiles with no errors or warnings.

- [ ] **Step 4: Run tests**

Run: `cargo test`
Expected: all 17 tests pass.

- [ ] **Step 5: Manual verification**

Run: `cargo run`
Expected: window opens. Drag the window wider/narrower:
- CPU cores wrap into more columns as the window widens (1–8 per line).
- Battery/Memory/Network cards stack full-width when narrow and sit side-by-side (up to 3-across) when wide.
- No content is clipped.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "feat: make dashboard layout reflow with window width"
```

---

### Task 2: Split CPU summary and add hover tooltips

**Files:**
- Modify: `src/ui.rs` (`Ui` struct, `Ui::update`, CPU summary section of `build()`, core-loop label, battery/memory/network label creation)

**Interfaces:**
- Consumes: `Ui` fields `cpu_core`, `cpu_pkg`, `cpu_temp` (created in this task); widget creation blocks from Task 1.
- Produces: final `Ui` struct with `cpu_core`/`cpu_pkg`/`cpu_temp` labels. Tooltip strings (exact, from spec):
  - Core: `"Combined power draw of the CPU cores (RAPL core domain)"`
  - Pkg: `"Total CPU package power, incl. cores, cache, and GPU (RAPL package domain)"`
  - Temp: `"CPU package temperature"`
  - per-core: `"CPU load % and current clock frequency (MHz)"`
  - Health: `"Battery capacity vs. its design capacity"`
  - Charging: `"Current charging power draw"`
  - Discharging: `"Current discharging power draw"`
  - Memory: `"RAM in use, swap usage, and zram compressed swap size"`
  - ↑: `"Upload rate"`, ↓: `"Download rate"`

- [ ] **Step 1: Split the `cpu_summary` field into three labels**

In the `Ui` struct, replace:

```rust
    cpu_summary: gtk4::Label,
```

with:

```rust
    cpu_core: gtk4::Label,
    cpu_pkg: gtk4::Label,
    cpu_temp: gtk4::Label,
```

- [ ] **Step 2: Update `Ui::update` for the three labels**

Replace:

```rust
        self.cpu_summary
            .set_text(&format!("Core {cw}  ·  Pkg {pw}  ·  {t}"));
```

with:

```rust
        self.cpu_core.set_text(&format!("Core {cw}"));
        self.cpu_pkg.set_text(&format!("Pkg {pw}"));
        self.cpu_temp.set_text(&format!("{t}"));
```

- [ ] **Step 3: Replace the summary label construction with three labeled labels**

In `build()`, replace:

```rust
    let summary = gtk4::Label::new(Some("—"));
    summary.set_xalign(0.0);
    cbox.append(&summary);
```

with:

```rust
    let core_label = gtk4::Label::new(Some("—"));
    core_label.set_xalign(0.0);
    core_label.set_tooltip_text(Some(
        "Combined power draw of the CPU cores (RAPL core domain)",
    ));
    let pkg_label = gtk4::Label::new(Some("—"));
    pkg_label.set_xalign(0.0);
    pkg_label.set_tooltip_text(Some(
        "Total CPU package power, incl. cores, cache, and GPU (RAPL package domain)",
    ));
    let temp_label = gtk4::Label::new(Some("—"));
    temp_label.set_xalign(0.0);
    temp_label.set_tooltip_text(Some("CPU package temperature"));
    let summary_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    summary_row.append(&core_label);
    summary_row.append(&pkg_label);
    summary_row.append(&temp_label);
    cbox.append(&summary_row);
```

- [ ] **Step 4: Update the `Ui` construction**

In the `Rc::new(Ui { ... })` block, replace:

```rust
        cpu_summary: summary,
```

with:

```rust
        cpu_core: core_label,
        cpu_pkg: pkg_label,
        cpu_temp: temp_label,
```

- [ ] **Step 5: Add the remaining tooltips in `build()`**

In the CPU core loop (from Task 1), add one line after `freq.set_xalign(0.0);`:

```rust
        freq.set_tooltip_text(Some("CPU load % and current clock frequency (MHz)"));
```

After each of these labels in `build()`, add the matching `set_tooltip_text` call:

`bat_health`:

```rust
    bat_health.set_tooltip_text(Some("Battery capacity vs. its design capacity"));
```

`bat_charge`:

```rust
    bat_charge.set_tooltip_text(Some("Current charging power draw"));
```

`bat_discharge`:

```rust
    bat_discharge.set_tooltip_text(Some("Current discharging power draw"));
```

`mem_text`:

```rust
    mem_text.set_tooltip_text(Some("RAM in use, swap usage, and zram compressed swap size"));
```

`net_down`:

```rust
    net_down.set_tooltip_text(Some("Download rate"));
```

`net_up`:

```rust
    net_up.set_tooltip_text(Some("Upload rate"));
```

- [ ] **Step 6: Build**

Run: `cargo build`
Expected: compiles with no errors or warnings.

- [ ] **Step 7: Run tests**

Run: `cargo test`
Expected: all 17 tests pass.

- [ ] **Step 8: Manual verification**

Run: `cargo run`
Expected: CPU summary shows three separate terms — `Core …`, `Pkg …`, `Temp …`. Hover over each term and over the per-core labels, battery labels, memory label, and ↑/↓ network labels — a tooltip explaining the term appears on each.

- [ ] **Step 9: Commit**

```bash
git add src/ui.rs
git commit -m "feat: split cpu summary and add explanatory hover tooltips"
```
