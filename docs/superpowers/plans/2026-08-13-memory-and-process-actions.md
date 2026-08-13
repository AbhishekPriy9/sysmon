# Memory Display + Process End/Close Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make sysmon's Memory page show System Monitor's exact numbers (Used = MemTotal − MemAvailable, Cache = Cached + Buffers) via a theme-aware segmented bar plus labeled rows, and add right-click → End Process / Close App with confirmation on the Process tab.

**Architecture:** Extend `parse_meminfo` to also capture `MemFree`, `Buffers`, `Cached`; sum buffers+cached into a new `Memory.cache_kb`. Replace the single `ProgressBar` + text label on the memory card with a 3-segment CSS bar (used=accent, cache=warning, free=empty) and a boxed list of rows. For process actions: add a `pids` list to `AppRow`, a `terminate(pid)` helper using `libc::kill(SIGTERM)`, a hidden pid column in the Apps store, a right-click `GestureClick` on each table that resolves the row under the cursor (`TreeView::path_at_pos`), shows a `Popover` menu item, then an `adw::MessageDialog` confirmation before sending SIGTERM.

**Tech Stack:** Rust edition 2024, GTK4 0.11.4 (`v4_6`), libadwaita 0.9.2 (`v1_1`, `v1_5`, `gtk_v4_6`), `libc` 0.2 (new, Task 3).

## Global Constraints

- Only new dependency permitted: `libc = "0.2"` (Task 3), already approved by the user. Everything else must use crates already in `Cargo.toml`.
- No code comments in changed/new code.
- Cache = `Cached + Buffers` only (NOT `SReclaimable`) — user decision.
- Signal sent is always SIGTERM only (no SIGKILL option) — user decision.
- Both sub-views get actions: Processes → "End Process" (that PID); Apps → "Close App" (all PIDs of the app). sysmon's own PID is never signalled.
- Build/test: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test` then `cargo build`.
- Files that may change: `Cargo.toml`, `src/read.rs`, `src/model.rs`, `src/sampler.rs`, `src/process.rs`, `src/ui.rs`.
- Existing tests must keep passing; `cargo build` must be warning-free.

---

### Task 1: Memory data plumbing (`MemFree`, `Buffers`, `Cached`)

**Files:**
- Modify: `src/read.rs:7-29` (`parse_meminfo`) and test `src/read.rs:115-119`
- Modify: `src/model.rs:29-35` (`Memory` struct)
- Modify: `src/sampler.rs:157-172` (`read_memory`) and test `src/sampler.rs:224-231`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `parse_meminfo(&str) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64)` — `(total_kb, free_kb, avail_kb, buffers_kb, cached_kb, swap_total_kb, swap_free_kb, zswap_kb, zswapped_kb)` in `/proc/meminfo` order.
  - `Memory` gains `free_kb: u64` and `cache_kb: u64` (cache_kb pre-summed = buffers + cached).

- [ ] **Step 1: Write the failing test** — replace the body of `meminfo_parses_values` in `src/read.rs` (line 116-119) with:

```rust
    #[test]
    fn meminfo_parses_values() {
        let s = "MemTotal:       15755096 kB\nMemFree:         1234567 kB\nMemAvailable:    9876543 kB\nBuffers:         45678 kB\nCached:          789012 kB\nSwapTotal:       20775844 kB\nSwapFree:        19000000 kB\nZswap:           100 kB\nZswapped:        20 kB\n";
        assert_eq!(
            parse_meminfo(s),
            (15755096, 1234567, 9876543, 45678, 789012, 20775844, 19000000, 100, 20)
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test meminfo_parses_values`
Expected: FAIL (tuple arity mismatch).

- [ ] **Step 3: Extend `parse_meminfo`** — replace the entire function in `src/read.rs:7-29`:

```rust
pub fn parse_meminfo(s: &str) -> (u64, u64, u64, u64, u64, u64, u64, u64, u64) {
    let mut total = 0;
    let mut free = 0;
    let mut avail = 0;
    let mut buffers = 0;
    let mut cached = 0;
    let mut swap_total = 0;
    let mut swap_free = 0;
    let mut zswap = 0;
    let mut zswapped = 0;
    for line in s.lines() {
        let mut it = line.split_whitespace();
        let key = it.next().unwrap_or("");
        let val: u64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        match key {
            "MemTotal:" => total = val,
            "MemFree:" => free = val,
            "MemAvailable:" => avail = val,
            "Buffers:" => buffers = val,
            "Cached:" => cached = val,
            "SwapTotal:" => swap_total = val,
            "SwapFree:" => swap_free = val,
            "Zswap:" => zswap = val,
            "Zswapped:" => zswapped = val,
            _ => {}
        }
    }
    (total, free, avail, buffers, cached, swap_total, swap_free, zswap, zswapped)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test meminfo_parses_values`
Expected: PASS.

- [ ] **Step 5: Add `free_kb` and `cache_kb` to `Memory`** — replace struct in `src/model.rs:29-35`:

```rust
pub struct Memory {
    pub total_kb: u64,
    pub free_kb: u64,
    pub avail_kb: u64,
    pub cache_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub zram_compressed_kb: u64,
}
```

- [ ] **Step 6: Update `read_memory`** — replace body in `src/sampler.rs:157-172`:

```rust
    fn read_memory(&self) -> Memory {
        let s = read_file("/proc/meminfo").unwrap_or_default();
        let (total_kb, free_kb, avail_kb, buffers_kb, cached_kb, swap_total_kb, swap_free_kb, _zswap, _zswapped) =
            parse_meminfo(&s);
        let zram_compressed_kb = read_file("/sys/block/zram0/mm_stat")
            .and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
            .map(|b| b / 1024)
            .unwrap_or(0);
        Memory {
            total_kb,
            free_kb,
            avail_kb,
            cache_kb: buffers_kb + cached_kb,
            swap_total_kb,
            swap_free_kb,
            zram_compressed_kb,
        }
    }
```

- [ ] **Step 7: Strengthen sampler test** — in `src/sampler.rs` test `sample_populates_core_fields` (line 225-231), after `assert!(snap.mem.total_kb > 0);` add:

```rust
        assert!(snap.mem.free_kb > 0);
        assert!(snap.mem.cache_kb > 0);
```

- [ ] **Step 8: Full test run**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test`
Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add src/read.rs src/model.rs src/sampler.rs
git commit -m "feat: expose free and cache memory from meminfo"
```

---

### Task 2: Segmented bar + memory rows in UI

**Files:**
- Modify: `src/ui.rs:14-31` (CSS), `src/ui.rs:33-52` (struct fields), `src/ui.rs:111-126` (`update` memory section), `src/ui.rs:450-463` (memory card construction), `src/ui.rs:529-548` (Ui init).

**Interfaces:**
- Consumes: `Memory.free_kb`, `Memory.cache_kb` (Task 1); `WidgetExt::width()` from gtk4 0.11.4; existing helpers `row()`, `boxed_list()`, `human_kb()`.
- Produces: Ui fields `mem_seg_bar` (Box), `mem_seg_used/cache/free` (Box), `mem_used/cache/free/swap/zram` (Label). No later task consumes these.

- [ ] **Step 1: Replace the memory CSS** — in `src/ui.rs`, delete lines 26-27 (the two `progressbar.sysmon-mem-bar` rules) and add after the `.sysmon-freeze` rule (line 30):

```css
.sysmon-seg-bar { min-height: 10px; border-radius: 5px; background-color: alpha(var(--border-color), 0.4); }
.sysmon-seg-used { border-radius: 5px; background-color: var(--accent-bg-color); }
.sysmon-seg-cache { border-radius: 5px; background-color: var(--warning-bg-color); }
.sysmon-seg-free { border-radius: 5px; }
```

- [ ] **Step 2: Update `Ui` struct** — in `src/ui.rs:33-52`, replace `mem_bar: ProgressBar,` and `mem_text: gtk4::Label,` with:

```rust
    mem_seg_bar: gtk4::Box,
    mem_seg_used: gtk4::Box,
    mem_seg_cache: gtk4::Box,
    mem_seg_free: gtk4::Box,
    mem_used: gtk4::Label,
    mem_cache: gtk4::Label,
    mem_free: gtk4::Label,
    mem_swap: gtk4::Label,
    mem_zram: gtk4::Label,
```

- [ ] **Step 3: Rewrite the memory card construction** — in `src/ui.rs:450-463`, replace the `mem_bar`/`mem_text` block (lines 452-463) with:

```rust
    let mem_seg_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    mem_seg_bar.add_css_class("sysmon-seg-bar");
    mem_seg_bar.set_hexpand(true);
    let mem_seg_used = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_used.add_css_class("sysmon-seg-used");
    mem_seg_used.set_size_request(0, -1);
    let mem_seg_cache = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_cache.add_css_class("sysmon-seg-cache");
    mem_seg_cache.set_size_request(0, -1);
    let mem_seg_free = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_free.add_css_class("sysmon-seg-free");
    mem_seg_free.set_size_request(0, -1);
    mem_seg_bar.append(&mem_seg_used);
    mem_seg_bar.append(&mem_seg_cache);
    mem_seg_bar.append(&mem_seg_free);
    mbody.append(&mem_seg_bar);

    let mlist = boxed_list();
    let (used_row, mem_used) =
        row("Used", "MemTotal − MemAvailable, as System Monitor counts it");
    mlist.append(&used_row);
    let (cache_row, mem_cache) = row("Cache", "Buffers + Cached (reclaimable page cache)");
    mlist.append(&cache_row);
    let (free_row, mem_free) = row("Free", "MemFree (completely unused)");
    mlist.append(&free_row);
    let (swap_row, mem_swap) = row("Swap", "Swap used / total");
    mlist.append(&swap_row);
    let (zram_row, mem_zram) = row("Zram", "Compressed zram swap in use");
    mlist.append(&zram_row);
    mbody.append(&mlist);
```

- [ ] **Step 4: Rewrite the memory section of `update`** — in `src/ui.rs:111-126`, replace with:

```rust
        let t = s.mem.total_kb;
        let used = t.saturating_sub(s.mem.avail_kb);
        let cache = s.mem.cache_kb;
        let free = s.mem.free_kb;
        let swap_used = s.mem.swap_total_kb.saturating_sub(s.mem.swap_free_kb);

        self.mem_used.set_text(&human_kb(used));
        self.mem_cache.set_text(&human_kb(cache));
        self.mem_free.set_text(&human_kb(free));
        self.mem_swap.set_text(&format!(
            "{} / {}",
            human_kb(swap_used),
            human_kb(s.mem.swap_total_kb)
        ));
        self.mem_zram.set_text(&human_kb(s.mem.zram_compressed_kb));

        let w = self.mem_seg_bar.width();
        if t > 0 && w > 0 {
            let avail_w = (w - 4).max(1) as f64;
            let used_w = (avail_w * (used as f64 / t as f64)).round() as i32;
            let cache_w = (avail_w * (cache as f64 / t as f64)).round() as i32;
            let free_w = (avail_w as i32 - used_w - cache_w).max(0);
            self.mem_seg_used.set_size_request(used_w, -1);
            self.mem_seg_cache.set_size_request(cache_w, -1);
            self.mem_seg_free.set_size_request(free_w, -1);
        }
```

- [ ] **Step 5: Update the `Ui` init** — in `src/ui.rs:529-548`, replace `mem_bar,` and `mem_text,` with:

```rust
        mem_seg_bar,
        mem_seg_used,
        mem_seg_cache,
        mem_seg_free,
        mem_used,
        mem_cache,
        mem_free,
        mem_swap,
        mem_zram,
```

- [ ] **Step 6: Build and test**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test && cargo build`
Expected: All tests pass; `cargo build` completes with no warnings.

- [ ] **Step 7: Manual verification**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run`
Compare against `free -m` and `/usr/bin/gnome-system-monitor`:
- Used row ≈ `MemTotal − MemAvailable` (e.g., ~5.0 GB)
- Cache row ≈ `Buffers + Cached` (e.g., ~4.0 GB)
- Segments show used (accent) + cache (warning) proportionally, remainder empty; resizing the window re-settles widths within ~0.5 s.

- [ ] **Step 8: Commit**

```bash
git add src/ui.rs
git commit -m "feat: add system-monitor style memory bar and cache rows"
```

---

### Task 3: Process model + `terminate` helper

**Files:**
- Modify: `Cargo.toml` (add `libc`)
- Modify: `src/model.rs:50-56` (`AppRow` struct)
- Modify: `src/process.rs:55-76` (`group_apps`) and tests `src/process.rs:82-102`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `AppRow` gains `pub pids: Vec<u32>`.
  - `group_apps(&[ProcRow]) -> Vec<AppRow>` now fills `pids` with the PIDs of every member process.
  - `terminate(pid: u32) -> bool` — sends SIGTERM via `libc::kill`; returns false for `pid == 0`, `pid > i32::MAX as u32`, or when `kill` fails. Task 4 depends on this signature.

- [ ] **Step 1: Write the failing tests** — in `src/process.rs`:

  a) Update `group_apps_aggregates_and_sorts_by_cpu` (line 82-97) to include pids. Replace the `assert_eq!(apps[0].proc_count, 2);` line with:

```rust
        assert_eq!(apps[0].proc_count, 2);
        assert_eq!(apps[0].pids, vec![1, 2]);
```

  b) Add a new test after `group_apps_empty_input`:

```rust
    #[test]
    fn terminate_rejects_invalid_pids() {
        assert!(!terminate(0));
        assert!(!terminate(3_000_000_000));
        assert!(!terminate(2_000_000_000));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test group_apps terminate_rejects_invalid_pids`
Expected: FAIL (no `pids` field yet; `terminate` not defined).

- [ ] **Step 3: Add `libc` dependency** — in `Cargo.toml`, after the `gtk4` line (line 8) add:

```toml
libc = "0.2"
```

- [ ] **Step 4: Add `pids` to `AppRow`** — in `src/model.rs:50-56`:

```rust
pub struct AppRow {
    pub name: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub rss_kb: u64,
    pub proc_count: u32,
    pub pids: Vec<u32>,
}
```

- [ ] **Step 5: Update `group_apps` and add `terminate`** — in `src/process.rs`, replace the `group_apps` function (lines 55-76) with:

```rust
pub fn group_apps(procs: &[ProcRow]) -> Vec<AppRow> {
    let mut map: HashMap<&str, (f64, f64, u64, u32, Vec<u32>)> = HashMap::new();
    for p in procs {
        let e = map
            .entry(p.name.as_str())
            .or_insert((0.0, 0.0, 0, 0, Vec::new()));
        e.0 += p.cpu_pct;
        e.1 += p.mem_pct;
        e.2 += p.rss_kb;
        e.3 += 1;
        e.4.push(p.pid);
    }
    let mut rows: Vec<AppRow> = map
        .into_iter()
        .map(|(name, (cpu, mem, rss, count, pids))| AppRow {
            name: name.to_string(),
            cpu_pct: cpu,
            mem_pct: mem,
            rss_kb: rss,
            proc_count: count,
            pids,
        })
        .collect();
    rows.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

pub fn terminate(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    unsafe { libc::kill(pid as i32, libc::SIGTERM) == 0 }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test group_apps terminate_rejects_invalid_pids`
Expected: PASS.

- [ ] **Step 7: Full test run + build**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test && cargo build`
Expected: All tests pass; build warning-free.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/model.rs src/process.rs
git commit -m "feat: track app pids and add sigterm terminate helper"
```

---

### Task 4: Right-click End Process / Close App with confirmation

**Files:**
- Modify: `src/ui.rs:279-323` (`build_apps_table` / `build_procs_table`), `src/ui.rs:135-162` (`refill_table`), `src/ui.rs:480-484` (call sites), add helpers near line 223, wire gestures in `build()` after the tables are created.

**Interfaces:**
- Consumes: `AppRow.pids` and `terminate(pid) -> bool` (Task 3); `TreeModelExt::get_value(&iter, i32) -> glib::Value`, `TreeView::path_at_pos`, `TreeSelection::select_iter`, `GestureClick::set_button/connect_pressed`, `Popover::set_child/set_parent/present/popdown`, `adw::MessageDialog` (all verified in gtk4 0.11.4 / adw 0.9.2).
- Produces: `build_apps_table() -> (ListStore, gtk4::TreeView, gtk4::ScrolledWindow)` with a hidden STRING column at store index 5 holding comma-joined PIDs; `build_procs_table() -> (ListStore, gtk4::TreeView, gtk4::ScrolledWindow)`; free functions `action_popover(...)` and `confirm_terminate(...)`. No later task consumes these.

- [ ] **Step 1: Return the TreeView from both table builders** — in `src/ui.rs`:

  a) `build_apps_table` (lines 279-300): change signature to `(ListStore, gtk4::TreeView, gtk4::ScrolledWindow)`, add a hidden column to the store type list, and return the view. Replace the store construction (lines 280-286) with:

```rust
    let store = ListStore::new(&[
        glib::Type::STRING,
        glib::Type::U32,
        glib::Type::STRING,
        glib::Type::U32,
        glib::Type::U64,
        glib::Type::STRING,
    ]);
```

  and replace `(store, sw)` (line 299) with `(store, view, sw)`. Note `view` is currently an internal binding (`gtk4::TreeView::with_model(&store)`, line 287) — keep it in scope; do NOT rename it.

  b) `build_procs_table` (lines 302-323): same change to signature and return — `(store, view, sw)`.

- [ ] **Step 2: Update the call sites** — in `src/ui.rs:480-481`:

```rust
    let (apps_store, apps_view, apps_sw) = build_apps_table();
    let (procs_store, procs_view, procs_sw) = build_procs_table();
    stack.add_titled(&apps_sw, Some("apps"), "Apps");
    stack.add_titled(&procs_sw, Some("procs"), "Processes");
```

  (replacing the existing `add_titled` lines 482-483 which used the old `apps_view`/`procs_view` scrolled windows).

- [ ] **Step 3: Write the hidden pid column in `refill_table`** — in `src/ui.rs`, apps branch (line 150), after `store.set_value(&it, 4, &a.rss_kb.to_value());` add:

```rust
                store.set_value(&it, 5, &a.pids.join(",").to_value());
```

- [ ] **Step 4: Add helper functions** — after `fn boxed_list` (line 208-213) add:

```rust
fn action_popover(
    anchor: &impl IsA<gtk4::Widget>,
    label: &str,
    activate: impl Fn() + 'static,
) {
    let pop = Rc::new(gtk4::Popover::new());
    let btn = gtk4::Button::with_label(label);
    btn.add_css_class("flat");
    btn.set_hexpand(true);
    let pop2 = Rc::clone(&pop);
    btn.connect_clicked(move |_| {
        pop2.popdown();
        activate();
    });
    let box_ = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    box_.append(&btn);
    pop.set_child(Some(&box_));
    pop.set_parent(anchor);
    pop.present();
}

fn confirm_terminate(
    window: &impl IsA<gtk4::Window>,
    heading: &str,
    body: &str,
    mut pids: Vec<u32>,
) {
    let own = std::process::id();
    pids.retain(|&p| p != own);
    if pids.is_empty() {
        return;
    }
    let dlg = adw::MessageDialog::new(Some(heading), Some(body));
    dlg.set_transient_for(Some(window));
    dlg.add_response("cancel", "Cancel");
    dlg.add_response("end", "End");
    dlg.set_default_response(Some("end"));
    dlg.set_close_response("cancel");
    dlg.connect_response(move |d, resp| {
        if resp == "end" {
            for pid in &pids {
                crate::process::terminate(*pid);
            }
        }
        d.close();
    });
    dlg.present();
}
```

- [ ] **Step 5: Wire the gestures** — in `build()`, after `pbody.append(&stack);` (line 484) add:

```rust
    {
        let win = window.clone();
        let store = Rc::new(apps_store.clone());
        let view = apps_view.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);
        gesture.connect_pressed(move |_, _, x, y| {
            let Some((Some(path), _, _, _)) = view.path_at_pos(x as i32, y as i32) else {
                return;
            };
            let Some(iter) = store.iter(&path) else {
                return;
            };
            view.selection().select_iter(&iter);
            let name: String = store.get_value(&iter, 0).get().unwrap_or_default();
            let pids: Vec<u32> = store
                .get_value(&iter, 5)
                .get::<String>()
                .unwrap_or_default()
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect();
            let heading = format!("Close app \"{name}\"?");
            let body = format!("Its {} process(es) will be terminated.", pids.len());
            action_popover(&view, "Close App", move || {
                confirm_terminate(&win, &heading, &body, pids.clone())
            });
        });
        apps_view.add_controller(gesture);
    }

    {
        let win = window.clone();
        let store = Rc::new(procs_store.clone());
        let view = procs_view.clone();
        let gesture = gtk4::GestureClick::new();
        gesture.set_button(3);
        gesture.connect_pressed(move |_, _, x, y| {
            let Some((Some(path), _, _, _)) = view.path_at_pos(x as i32, y as i32) else {
                return;
            };
            let Some(iter) = store.iter(&path) else {
                return;
            };
            view.selection().select_iter(&iter);
            let name: String = store.get_value(&iter, 0).get().unwrap_or_default();
            let pid: u32 = store.get_value(&iter, 1).get().unwrap_or_default();
            let heading = format!("End process \"{name}\" (PID {pid})?");
            let body = "The process will be terminated.";
            action_popover(&view, "End Process", move || {
                confirm_terminate(&win, &heading, &body, vec![pid])
            });
        });
        procs_view.add_controller(gesture);
    }
```

- [ ] **Step 6: Build and test**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test && cargo build`
Expected: All tests pass; build warning-free (note `apps_view`/`procs_view` and `apps_sw`/`procs_sw` names must line up at the call sites and gesture blocks).

- [ ] **Step 7: Manual verification**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run`
- Processes view: right-click a row → row highlights → popover "End Process" → dialog "End process \"<name>\" (PID <n>)? → Cancel / End. Confirm on a disposable process (e.g., run `sleep 999` first) → process disappears from the list within ~1 s.
- Apps view: right-click an app with multiple processes → "Close App" → dialog lists its process count → End → all disappear.
- Right-clicking empty space does nothing; no crash when clicking a row that disappears between samples.

- [ ] **Step 8: Commit**

```bash
git add src/ui.rs
git commit -m "feat: right-click end process and close app with confirmation"
```

---

## Self-Review

- **Spec coverage:** Memory numbers (Tasks 1-2), segmented bar (Task 2), End Process (Task 4), Close App both views (Tasks 3-4), confirmation dialog (Task 4), SIGTERM only + libc (Task 3). ✓
- **Placeholder scan:** all code inline; no TBDs. ✓
- **Type consistency:** `parse_meminfo` 9-tuple matches sampler unpack; `Memory` fields match UI usage; `AppRow.pids` flows model → refill_table hidden column → gesture → `confirm_terminate`; `terminate(pid)` signature used in ui.rs. ✓
- **Rationale:** `SReclaimable` deliberately not parsed (user chose `Cached + Buffers`). Segments sized from `Box::width()` per update tick because this gtk4 version exposes no `set_draw_func`. `TreeModelExt::get_value` used (not `value`). PID guards prevent accidental `kill(-1)` / kill(0) / self-kill.
