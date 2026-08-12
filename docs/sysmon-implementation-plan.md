# Sysmon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `sysmon`, a standalone Rust + GTK4/libadwaita desktop system monitor for a single laptop (i5-1145G7, Debian 13, GNOME Shell), showing per-core load/freq, CPU watts, battery health/drain, memory, network, and an apps/processes table — live numbers only, refreshed every 1 s. The app is packaged as a `.deb` so it installs on Debian and Ubuntu.

**Architecture:** A background `sampler` thread reads sysfs/`/proc` every second and produces a `Snapshot` struct sent over a `glib::MainContext::channel` to the GTK main loop, where widgets update in place. Data acquisition is pure Rust (no GTK) so parsers are unit-testable; the GUI is verified manually.

**Tech Stack:** Rust (edition 2021), `gtk4`, `libadwaita`, `glib`. System packages `libgtk-4-dev`, `libadwaita-1-dev`, Rust via rustup. RAPL access via a udev rule.

---

## Global Constraints

- **The app code lives in THIS repo root (`/home/abhishek/Documents/Git/sysmon`)** — a new standalone crate created at the repo root, NOT at `~/sysmon` (user override). This plan file lives at `docs/sysmon-implementation-plan.md` inside the same repo. Substitute this repo root for every `~/sysmon` in the commands below.
- Target machine facts (verified): 8 logical CPUs (`/sys/devices/system/cpu/online` = `0-7`), GTK 4.18 runtime present, libadwaita 1.7 runtime present, dev packages NOT yet installed, rustup NOT installed. Battery `BAT0` present, health ≈48.6%. Network interfaces `enp0s31f6` + `wlp0s20f3` (exclude `lo`).
- **Crate features: broad compatibility (user override of "highest installed"):** `gtk4` feature `v4_6` and `libadwaita` features `v1_1` + `gtk_v4_6`. The app uses only basic widgets, so a binary built with these runs on GTK 4.6+ / libadwaita 1.1+ — i.e. Ubuntu 22.04+, Debian 12+, Debian 13. The crate versions gtk4 0.11 / libadwaita 0.9 expose these features.
- Refresh interval is 1 s. Layout B: single scrolling dashboard, all cards visible. Live numbers only — no history graphs anywhere.
- Processes card: **Apps** table is the default view, rows aggregated per app, **sorted by CPU descending by default**, clickable column headers to re-sort. A toggle switches to raw per-process rows. **No kill/terminate actions.** No "top apps" strip.
- No history state, no config files, no persistence. No external crates beyond `gtk4`, `libadwaita`, `glib`.
- Non-interactive sudo password is `1234` — use `export PW=1234` then `echo "$PW" | sudo -S <cmd>` when commands block on a prompt.
- Unit tests cover pure functions only (meminfo, cpu load, RAPL watts incl. wrap, battery watts, health %, net rate, `/proc/*/stat` parsing, app grouping). GUI correctness is verified by the manual checklist in each task — run it, don't skip it.
- Commit after every green task. Commit style: imperative, lowercase (`feat:`, `test:`, `chore:`).
- **Debian/Ubuntu installability:** Task 8 packages the app with `cargo-deb` (binary → `/usr/bin`, `.desktop` file → `/usr/share/applications`, SVG icon → hicolor theme, readme → `/usr/share/doc`) and verifies the `.deb` with `dpkg`.
- **Cleanup after development (Task 9, user override):** remove ONLY what this development installed: rustup (`~/.rustup`, `~/.cargo`), `cargo-deb`, `libgtk-4-dev`, `libadwaita-1-dev`, the RAPL udev rule, and build artifacts (`target/`, the produced `.deb`). Keep the source code and pre-existing runtime packages (`libgtk-4-1`, `libadwaita-1-0`, gcc, dpkg, pkg-config).

---

### Task 0: Environment, scaffold, and a window that opens

**Files:**
- Create (in THIS repo root): `Cargo.toml`, `src/main.rs`, `src/lib.rs`
- System: install rustup + dev packages, create the udev rule dir if needed (later task).

**Interfaces:**
- Consumes: nothing.
- Produces: a compiling crate named `sysmon` with an empty GTK window; `src/lib.rs` exports no modules yet.

- [ ] **Step 1: Install the Rust toolchain**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env
rustc --version && cargo --version
```
Expected: prints versions (e.g. `rustc 1.8x.x`, `cargo 1.8x.x`). If a new shell is opened later, `source ~/.cargo/env` is required.

- [ ] **Step 2: Install GTK4 and libadwaita development packages**

```bash
export PW=1234
echo "$PW" | sudo -S apt-get update
echo "$PW" | sudo -S apt-get install -y libgtk-4-dev libadwaita-1-dev
```
Expected: install completes without errors.

- [ ] **Step 3: Record the exact installed versions (reference only — features are fixed at `v4_6`/`v1_1`)**

```bash
pkg-config --modversion gtk4 libadwaita-1
```
Expected: two lines, e.g. `4.18.x` and `1.7.x`. Note them for the record; the crate features stay `v4_6`/`v1_1` for broad Debian/Ubuntu compatibility.

- [ ] **Step 4: Create the project (in this repo root, NOT `~/sysmon`)**

```bash
cargo init --name sysmon
cargo add gtk4 libadwaita glib
```

`cargo init` creates the git repo (this repo root is not one yet) and a `.gitignore` with `/target`. `cargo add` pins the latest versions of the three crates.

- [ ] **Step 5: Set crate features for broad Debian/Ubuntu compatibility**

Edit `Cargo.toml` so the dependencies look like this (gtk4 0.11 / libadwaita 0.9 as resolved by `cargo add`; keep the resolved versions, only set the feature flags):

```toml
[package]
name = "sysmon"
version = "0.1.0"
edition = "2021"

[dependencies]
glib = "0.20"
gtk4 = { version = "0.11", features = ["v4_6"] }
libadwaita = { version = "0.9", features = ["v1_1", "gtk_v4_6"] }
```

The `v4_6`/`v1_1` features keep the binary runnable on GTK 4.6+/libadwaita 1.1+ (Ubuntu 22.04+, Debian 12+). The build machine has GTK 4.18 / libadwaita 1.7, which satisfy those minimums.

- [ ] **Step 6: Write a minimal window so we prove the crates link**

`src/lib.rs` (empty module root for now):

```rust
```

`src/main.rs`:

```rust
use gtk::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("dev.sysmon.Sysmon")
        .build();

    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("sysmon")
            .default_width(560)
            .default_height(820)
            .build();
        window.present();
    });

    app.run();
}
```

- [ ] **Step 7: Build and run**

```bash
cargo build
cargo run
```
Expected: `cargo build` compiles cleanly (first build is slow, GTK crates are large). `cargo run` opens an empty `sysmon` window titled "sysmon". Close it. If the build fails on an unknown feature, go back to Step 5 and adjust, then re-run.

- [ ] **Step 8: Commit**

```bash
git add -A && git commit -m "chore: scaffold sysmon project with empty gtk window"
```

---

### Task 1: Data model and pure parsers (unit tests first)

**Files:**
- Modify: `src/lib.rs`
- Create: `src/model.rs`, `src/read.rs` (tests inline)

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (used by every later task — these exact names/types are the contract):
  - `model::Snapshot { cpu: Cpu, battery: Option<Battery>, mem: Memory, net: Net, apps: Vec<AppRow>, procs: Vec<ProcRow> }`
  - `model::Cpu { cores: Vec<Core>, pkg_watts: Option<f64>, core_watts: Option<f64>, temp_c: Option<f64> }`
  - `model::Core { load: f64, freq_mhz: u64 }`
  - `model::Battery { charge_pct: f64, health_pct: f64, watts: f64, status: String }`
  - `model::Memory { total_kb: u64, avail_kb: u64, swap_total_kb: u64, swap_free_kb: u64, zram_compressed_kb: u64 }`
  - `model::Net { down_bps: u64, up_bps: u64 }`
  - `model::ProcRow { pid: u32, name: String, cpu_pct: f64, mem_pct: f64 }`
  - `model::AppRow { name: String, cpu_pct: f64, mem_pct: f64, proc_count: u32 }`
  - `read::read_file(path: &str) -> Option<String>`
  - `read::parse_meminfo(s: &str) -> (u64, u64, u64, u64, u64, u64)` — (MemTotal, MemAvailable, SwapTotal, SwapFree, Zswap, Zswapped) in kB
  - `read::parse_cpu_line(line: &str) -> Option<(u64, u64)>` — (busy_ticks, total_ticks), `None` for the aggregate `cpu` line or non-cpu lines
  - `read::load_percent(prev: (u64, u64), cur: (u64, u64)) -> f64`
  - `read::rapl_watts(prev: u64, cur: u64, max: u64, dt_sec: f64) -> f64`
  - `read::battery_watts(current_now: i64, voltage_now: i64) -> f64`
  - `read::health_pct(charge_full: u64, charge_design: u64) -> f64`
  - `read::net_rate(prev: u64, cur: u64, dt_sec: f64) -> u64`
  - `read::parse_proc_stat(line: &str) -> Option<(String, u64, u64)>` — (comm, utime, stime)
  - `read::parse_statm_rss(line: &str, page_size_kb: u64) -> Option<u64>` — resident kB
  - `read::process_cpu_percent(prev_ticks: u64, cur_ticks: u64, dt_sec: f64, clk_tck: u64) -> f64`

- [ ] **Step 1: Register the modules**

`src/lib.rs`:

```rust
pub mod model;
pub mod read;
```

- [ ] **Step 2: Write the model**

`src/model.rs`:

```rust
pub struct Snapshot {
    pub cpu: Cpu,
    pub battery: Option<Battery>,
    pub mem: Memory,
    pub net: Net,
    pub apps: Vec<AppRow>,
    pub procs: Vec<ProcRow>,
}

pub struct Cpu {
    pub cores: Vec<Core>,
    pub pkg_watts: Option<f64>,
    pub core_watts: Option<f64>,
    pub temp_c: Option<f64>,
}

pub struct Core {
    pub load: f64,
    pub freq_mhz: u64,
}

pub struct Battery {
    pub charge_pct: f64,
    pub health_pct: f64,
    pub watts: f64,
    pub status: String,
}

pub struct Memory {
    pub total_kb: u64,
    pub avail_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub zram_compressed_kb: u64,
}

pub struct Net {
    pub down_bps: u64,
    pub up_bps: u64,
}

pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
}

pub struct AppRow {
    pub name: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub proc_count: u32,
}
```

- [ ] **Step 3: Write the failing tests (TDD red)**

`src/read.rs` — write this file **with every function body as `unimplemented!()`** plus the full test module below:

```rust
use std::fs;

pub fn read_file(path: &str) -> Option<String> {
    unimplemented!()
}

pub fn parse_meminfo(s: &str) -> (u64, u64, u64, u64, u64, u64) {
    unimplemented!()
}

pub fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    unimplemented!()
}

pub fn load_percent(prev: (u64, u64), cur: (u64, u64)) -> f64 {
    unimplemented!()
}

pub fn rapl_watts(prev: u64, cur: u64, max: u64, dt_sec: f64) -> f64 {
    unimplemented!()
}

pub fn battery_watts(current_now: i64, voltage_now: i64) -> f64 {
    unimplemented!()
}

pub fn health_pct(charge_full: u64, charge_design: u64) -> f64 {
    unimplemented!()
}

pub fn net_rate(prev: u64, cur: u64, dt_sec: f64) -> u64 {
    unimplemented!()
}

pub fn parse_proc_stat(line: &str) -> Option<(String, u64, u64)> {
    unimplemented!()
}

pub fn parse_statm_rss(line: &str, page_size_kb: u64) -> Option<u64> {
    unimplemented!()
}

pub fn process_cpu_percent(prev_ticks: u64, cur_ticks: u64, dt_sec: f64, clk_tck: u64) -> f64 {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_parses_values() {
        let s = "MemTotal:       15755096 kB\nMemFree:         1234567 kB\nMemAvailable:    9876543 kB\nSwapTotal:       20775844 kB\nSwapFree:        19000000 kB\nZswap:           100 kB\nZswapped:        20 kB\n";
        assert_eq!(parse_meminfo(s), (15755096, 9876543, 20775844, 19000000, 100, 20));
    }

    #[test]
    fn cpu_line_and_load_percent() {
        let cur = parse_cpu_line("cpu0 100 0 50 50 50 0 0 0 0 0").unwrap();
        let prev = parse_cpu_line("cpu0 80 0 40 70 10 0 0 0 0 0").unwrap();
        assert_eq!(cur, (150, 250));
        assert_eq!(prev, (120, 200));
        let l = load_percent(prev, cur);
        assert!((l - 60.0).abs() < 1e-9);
    }

    #[test]
    fn cpu_line_rejects_aggregate_line() {
        assert!(parse_cpu_line("cpu 100 0 50 50 50 0 0 0 0 0").is_none());
    }

    #[test]
    fn rapl_watts_plain_and_wrap() {
        assert!((rapl_watts(10_000_000, 20_000_000, 100_000_000_000, 1.0) - 10.0).abs() < 1e-9);
        let w = rapl_watts(90_000_000_000, 5_000_000_000, 100_000_000_000, 1.0);
        assert!((w - 15000.0).abs() < 1e-9);
    }

    #[test]
    fn battery_watts_sign_and_magnitude() {
        assert!((battery_watts(1_689_000, 11_400_000) - 19.25).abs() < 0.02);
        assert!(battery_watts(-1_000_000, 11_400_000) < 0.0);
    }

    #[test]
    fn health_percent() {
        assert!((health_pct(1_792_000, 3_685_000) - 48.6).abs() < 0.1);
    }

    #[test]
    fn net_rate_is_zero_when_stable() {
        assert_eq!(net_rate(1000, 1000, 1.0), 0);
        assert_eq!(net_rate(0, 5000, 1.0), 5000);
    }

    #[test]
    fn proc_stat_parses_comm_with_spaces_and_parens() {
        let line = "1234 (my app (test)) S 1 2 3 4 5 6 7 8 9 10 100 200 300";
        let (name, utime, stime) = parse_proc_stat(line).unwrap();
        assert_eq!(name, "my app (test)");
        assert_eq!(utime, 100);
        assert_eq!(stime, 200);
    }

    #[test]
    fn statm_rss_multiplies_pages() {
        assert_eq!(parse_statm_rss("40 3 0 37 0 0 0", 4), Some(12));
    }

    #[test]
    fn process_cpu_percent_basic() {
        assert!((process_cpu_percent(0, 100, 1.0, 100) - 100.0).abs() < 1e-9);
        assert!((process_cpu_percent(0, 50, 1.0, 100) - 50.0).abs() < 1e-9);
    }
}
```

- [ ] **Step 4: Run tests and confirm they FAIL for the right reason**

```bash
cargo test
```
Expected: all 9 tests FAIL by panicking on `unimplemented!()` (never a compile error in the test code itself). If a test fails for a different reason (typo, missing module), fix that first.

- [ ] **Step 5: Implement the parsers (TDD green)**

Replace the `unimplemented!()` bodies in `src/read.rs` with:

```rust
pub fn read_file(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub fn parse_meminfo(s: &str) -> (u64, u64, u64, u64, u64, u64) {
    let mut total = 0;
    let mut avail = 0;
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
            "MemAvailable:" => avail = val,
            "SwapTotal:" => swap_total = val,
            "SwapFree:" => swap_free = val,
            "Zswap:" => zswap = val,
            "Zswapped:" => zswapped = val,
            _ => {}
        }
    }
    (total, avail, swap_total, swap_free, zswap, zswapped)
}

pub fn parse_cpu_line(line: &str) -> Option<(u64, u64)> {
    let mut it = line.split_whitespace();
    let name = it.next()?;
    if name == "cpu" || !name.starts_with("cpu") {
        return None;
    }
    let vals: Vec<u64> = it.filter_map(|v| v.parse().ok()).collect();
    if vals.len() < 8 {
        return None;
    }
    let idle = vals[3];
    let iowait = vals[4];
    let total: u64 = vals.iter().sum();
    Some((total - idle - iowait, total))
}

pub fn load_percent(prev: (u64, u64), cur: (u64, u64)) -> f64 {
    let dtot = cur.1.saturating_sub(prev.1);
    if dtot == 0 {
        return 0.0;
    }
    let dbusy = cur.0.saturating_sub(prev.0);
    dbusy as f64 / dtot as f64 * 100.0
}

pub fn rapl_watts(prev: u64, cur: u64, max: u64, dt_sec: f64) -> f64 {
    if dt_sec <= 0.0 {
        return 0.0;
    }
    let delta = if cur < prev { cur + max - prev } else { cur - prev };
    delta as f64 / 1_000_000.0 / dt_sec
}

pub fn battery_watts(current_now: i64, voltage_now: i64) -> f64 {
    current_now as f64 * voltage_now as f64 / 1e12
}

pub fn health_pct(charge_full: u64, charge_design: u64) -> f64 {
    if charge_design == 0 {
        return 0.0;
    }
    charge_full as f64 / charge_design as f64 * 100.0
}

pub fn net_rate(prev: u64, cur: u64, dt_sec: f64) -> u64 {
    if dt_sec <= 0.0 {
        return 0;
    }
    (cur.saturating_sub(prev) as f64 / dt_sec) as u64
}

pub fn parse_proc_stat(line: &str) -> Option<(String, u64, u64)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close <= open {
        return None;
    }
    let name = line[open + 1..close].to_string();
    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
    if rest.len() < 13 {
        return None;
    }
    let utime: u64 = rest[11].parse().ok()?;
    let stime: u64 = rest[12].parse().ok()?;
    Some((name, utime, stime))
}

pub fn parse_statm_rss(line: &str, page_size_kb: u64) -> Option<u64> {
    let resident: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident * page_size_kb)
}

pub fn process_cpu_percent(prev_ticks: u64, cur_ticks: u64, dt_sec: f64, clk_tck: u64) -> f64 {
    if dt_sec <= 0.0 || clk_tck == 0 {
        return 0.0;
    }
    let d = cur_ticks.saturating_sub(prev_ticks) as f64;
    d / clk_tck as f64 / dt_sec * 100.0
}
```

- [ ] **Step 6: Run tests and confirm all PASS**

```bash
cargo test
```
Expected: 9 passed, 0 failed.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: add data model and pure parsers with tests"
```

---

### Task 2: Process scanning and app grouping

**Files:**
- Modify: `src/lib.rs`
- Create: `src/process.rs` (tests inline)

**Interfaces:**
- Consumes: `model::{AppRow, ProcRow}`, `read::{parse_proc_stat, parse_statm_rss, process_cpu_percent}` from Task 1.
- Produces:
  - `process::scan_processes(prev: &mut HashMap<u32, u64>, dt_sec: f64, clk_tck: u64, page_size_kb: u64, total_ram_kb: u64) -> Vec<ProcRow>` — scans `/proc`, computes CPU% relative to one core (can exceed 100 on 8 cores), drops dead PIDs from `prev`, returns rows sorted by CPU desc.
  - `process::group_apps(procs: &[ProcRow]) -> Vec<AppRow>` — one row per comm name, CPU and MEM summed, count of processes, sorted by CPU desc.

- [ ] **Step 1: Register the module**

`src/lib.rs`:

```rust
pub mod model;
pub mod process;
pub mod read;
```

- [ ] **Step 2: Write the failing tests (TDD red)**

`src/process.rs` — write with both function bodies as `unimplemented!()` plus the tests below:

```rust
use std::collections::HashMap;
use std::fs;

use crate::model::{AppRow, ProcRow};
use crate::read::{parse_proc_stat, parse_statm_rss, process_cpu_percent};

pub fn scan_processes(
    prev: &mut HashMap<u32, u64>,
    dt_sec: f64,
    clk_tck: u64,
    page_size_kb: u64,
    total_ram_kb: u64,
) -> Vec<ProcRow> {
    unimplemented!()
}

pub fn group_apps(procs: &[ProcRow]) -> Vec<AppRow> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_apps_aggregates_and_sorts_by_cpu() {
        let procs = vec![
            ProcRow { pid: 1, name: "chrome".into(), cpu_pct: 5.0, mem_pct: 1.0 },
            ProcRow { pid: 2, name: "chrome".into(), cpu_pct: 7.0, mem_pct: 2.0 },
            ProcRow { pid: 3, name: "code".into(), cpu_pct: 3.0, mem_pct: 0.5 },
        ];
        let apps = group_apps(&procs);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "chrome");
        assert!((apps[0].cpu_pct - 12.0).abs() < 1e-9);
        assert!((apps[0].mem_pct - 3.0).abs() < 1e-9);
        assert_eq!(apps[0].proc_count, 2);
        assert_eq!(apps[1].name, "code");
    }

    #[test]
    fn group_apps_empty_input() {
        assert!(group_apps(&[]).is_empty());
    }

    #[test]
    fn scan_reads_the_real_proc_dir() {
        let mut prev = HashMap::new();
        let procs = scan_processes(&mut prev, 1.0, 100, 4, 15_000_000);
        assert!(!procs.is_empty());
        assert!(procs.iter().all(|p| p.cpu_pct >= 0.0));
    }

    #[test]
    fn scan_removes_dead_pids() {
        let mut prev = HashMap::new();
        prev.insert(999_999, 0u64);
        let _ = scan_processes(&mut prev, 1.0, 100, 4, 15_000_000);
        assert!(!prev.contains_key(&999_999));
    }
}
```

- [ ] **Step 3: Run tests and confirm they FAIL**

```bash
cargo test
```
Expected: the 4 new tests fail (panics). The Task 1 tests still pass.

- [ ] **Step 4: Implement (TDD green)**

Replace the bodies in `src/process.rs`:

```rust
pub fn scan_processes(
    prev: &mut HashMap<u32, u64>,
    dt_sec: f64,
    clk_tck: u64,
    page_size_kb: u64,
    total_ram_kb: u64,
) -> Vec<ProcRow> {
    let live: Vec<u32> = fs::read_dir("/proc")
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .filter_map(|s| s.parse().ok())
        .collect();

    let mut out = Vec::new();
    for pid in &live {
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some((name, utime, stime)) = parse_proc_stat(&stat) else {
            continue;
        };
        let ticks = utime.saturating_add(stime);
        let cpu_pct = match prev.get(pid) {
            Some(p) => process_cpu_percent(*p, ticks, dt_sec, clk_tck),
            None => 0.0,
        };
        let rss_kb = fs::read_to_string(format!("/proc/{pid}/statm"))
            .ok()
            .and_then(|s| parse_statm_rss(&s, page_size_kb))
            .unwrap_or(0);
        let mem_pct = if total_ram_kb == 0 {
            0.0
        } else {
            rss_kb as f64 / total_ram_kb as f64 * 100.0
        };
        out.push(ProcRow { pid: *pid, name, cpu_pct, mem_pct });
        prev.insert(*pid, ticks);
    }

    prev.retain(|pid, _| live.contains(pid));
    out.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub fn group_apps(procs: &[ProcRow]) -> Vec<AppRow> {
    let mut map: HashMap<&str, (f64, f64, u32)> = HashMap::new();
    for p in procs {
        let e = map.entry(p.name.as_str()).or_insert((0.0, 0.0, 0));
        e.0 += p.cpu_pct;
        e.1 += p.mem_pct;
        e.2 += 1;
    }
    let mut rows: Vec<AppRow> = map
        .into_iter()
        .map(|(name, (cpu, mem, count))| AppRow {
            name: name.to_string(),
            cpu_pct: cpu,
            mem_pct: mem,
            proc_count: count,
        })
        .collect();
    rows.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    rows
}
```

- [ ] **Step 5: Run the full suite**

```bash
cargo test
```
Expected: all 13 tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat: scan /proc processes and aggregate into apps"
```

---

### Task 3: Sampler thread (reads everything, no UI yet)

**Files:**
- Modify: `src/lib.rs`
- Create: `src/sampler.rs` (tests inline)

**Interfaces:**
- Consumes: `model::*`, `read::*`, `process::{group_apps, scan_processes}` from Tasks 1–2.
- Produces:
  - `sampler::Sampler` with `Sampler::new() -> Self` and `fn sample(&mut self) -> Snapshot`.
  - Hardcoded constants (verified for this machine): 1 s refresh; `clk_tck = 100`; `page_size_kb = 4`; 8 cores; temp zone `thermal_zone6`; battery `BAT0`; RAPL paths `intel-rapl:0` and `intel-rapl:0:0`; net excludes `lo`.
  - `sampler::online_count() -> usize` — number of logical CPUs from `/sys/devices/system/cpu/online` (returns 8 here).

- [ ] **Step 1: Register the module**

`src/lib.rs`:

```rust
pub mod model;
pub mod process;
pub mod read;
pub mod sampler;
```

- [ ] **Step 2: Write the sampler (implementation + integration tests)**

`src/sampler.rs`:

```rust
use std::collections::HashMap;
use std::time::Instant;

use crate::model::{AppRow, Battery, Core, Cpu, Memory, Net, ProcRow, Snapshot};
use crate::process::{group_apps, scan_processes};
use crate::read::{
    battery_watts, health_pct, load_percent, net_rate, parse_cpu_line, parse_meminfo,
    rapl_watts, read_file,
};

const RAPL_PKG: &str = "/sys/class/powercap/intel-rapl:0/energy_uj";
const RAPL_CORE: &str = "/sys/class/powercap/intel-rapl:0:0/energy_uj";

pub fn online_count() -> usize {
    read_file("/sys/devices/system/cpu/online")
        .map(|s| {
            s.trim()
                .split(',')
                .map(|part| {
                    if let Some((a, b)) = part.split_once('-') {
                        b.parse::<usize>().unwrap_or(0) - a.parse::<usize>().unwrap_or(0) + 1
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(8)
}

pub struct Sampler {
    ncores: usize,
    clk_tck: u64,
    page_size_kb: u64,
    prev_cpu: Vec<(u64, u64)>,
    prev_rapl_pkg: u64,
    prev_rapl_core: u64,
    rapl_max: u64,
    prev_net: HashMap<String, (u64, u64)>,
    prev_proc: HashMap<u32, u64>,
    last: Instant,
}

impl Sampler {
    pub fn new() -> Self {
        let rapl_max = read_file("/sys/class/powercap/intel-rapl:0/max_energy_range_uj")
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let ncores = online_count();
        Sampler {
            ncores,
            clk_tck: 100,
            page_size_kb: 4,
            prev_cpu: vec![(0, 0); ncores],
            prev_rapl_pkg: 0,
            prev_rapl_core: 0,
            rapl_max,
            prev_net: HashMap::new(),
            prev_proc: HashMap::new(),
            last: Instant::now(),
        }
    }

    pub fn sample(&mut self) -> Snapshot {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;

        let cores = self.sample_cores(dt);
        let pkg_watts = rapl_sample(RAPL_PKG, &mut self.prev_rapl_pkg, self.rapl_max, dt);
        let core_watts = rapl_sample(RAPL_CORE, &mut self.prev_rapl_core, self.rapl_max, dt);
        let temp_c = read_file("/sys/class/thermal/thermal_zone6/temp")
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|t| t as f64 / 1000.0);

        let battery = self.read_battery();
        let mem = self.read_memory();
        let net = self.read_net(dt);

        let procs = scan_processes(
            &mut self.prev_proc,
            dt,
            self.clk_tck,
            self.page_size_kb,
            mem.total_kb,
        );
        let apps = group_apps(&procs);

        Snapshot {
            cpu: Cpu { cores, pkg_watts, core_watts, temp_c },
            battery,
            mem,
            net,
            apps,
            procs,
        }
    }

    fn sample_cores(&mut self, dt: f64) -> Vec<Core> {
        let stat = read_file("/proc/stat").unwrap_or_default();
        let mut cores = Vec::with_capacity(self.ncores);
        for i in 0..self.ncores {
            let cur = stat
                .lines()
                .find(|l| l.starts_with(&format!("cpu{i} ")))
                .and_then(parse_cpu_line);
            let prev = self.prev_cpu.get(i).copied().unwrap_or((0, 0));
            if let Some(c) = cur {
                self.prev_cpu[i] = c;
            }
            let load = match (prev, cur) {
                (p, Some(c)) => load_percent(p, c),
                _ => 0.0,
            };
            let freq_mhz = read_file(&format!(
                "/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_cur_freq"
            ))
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0)
                / 1000;
            cores.push(Core { load, freq_mhz });
        }
        cores
    }

    fn read_battery(&self) -> Option<Battery> {
        let base = "/sys/class/power_supply/BAT0";
        let status = read_file(&format!("{base}/status"))?.trim().to_string();
        let charge_pct = read_file(&format!("{base}/capacity"))
            .and_then(|s| s.trim().parse().ok())?;
        let full = read_file(&format!("{base}/charge_full"))
            .and_then(|s| s.trim().parse().ok())?;
        let design = read_file(&format!("{base}/charge_full_design"))
            .and_then(|s| s.trim().parse().ok())?;
        let current_now = read_file(&format!("{base}/current_now"))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let voltage_now = read_file(&format!("{base}/voltage_now"))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        Some(Battery {
            charge_pct: charge_pct as f64,
            health_pct: health_pct(full, design),
            watts: battery_watts(current_now, voltage_now),
            status,
        })
    }

    fn read_memory(&self) -> Memory {
        let s = read_file("/proc/meminfo").unwrap_or_default();
        let (total_kb, avail_kb, swap_total_kb, swap_free_kb, _zswap, _zswapped) =
            parse_meminfo(&s);
        let zram_compressed_kb = read_file("/sys/block/zram0/mm_stat")
            .and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))
            .map(|b| b / 1024)
            .unwrap_or(0);
        Memory {
            total_kb,
            avail_kb,
            swap_total_kb,
            swap_free_kb,
            zram_compressed_kb,
        }
    }

    fn read_net(&mut self, dt: f64) -> Net {
        let s = read_file("/proc/net/dev").unwrap_or_default();
        let mut down = 0u64;
        let mut up = 0u64;
        let mut cur = HashMap::new();
        for line in s.lines().skip(2) {
            let mut it = line.split(':');
            let (Some(name), Some(rest)) = (it.next(), it.next()) else {
                continue;
            };
            let name = name.trim().to_string();
            if name == "lo" {
                continue;
            }
            let vals: Vec<u64> = rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
            if vals.len() < 9 {
                continue;
            }
            let rx = vals[0];
            let tx = vals[8];
            cur.insert(name.clone(), (rx, tx));
            let p = self.prev_net.get(&name).copied().unwrap_or((rx, tx));
            down += net_rate(p.0, rx, dt);
            up += net_rate(p.1, tx, dt);
        }
        self.prev_net = cur;
        Net { down_bps: down, up_bps: up }
    }
}

fn rapl_sample(path: &str, prev: &mut u64, rapl_max: u64, dt_sec: f64) -> Option<f64> {
    let cur = read_file(path).and_then(|s| s.trim().parse::<u64>().ok())?;
    if *prev == 0 {
        *prev = cur;
        return Some(0.0);
    }
    let w = rapl_watts(*prev, cur, rapl_max, dt_sec);
    *prev = cur;
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_count_is_eight_on_this_machine() {
        assert_eq!(online_count(), 8);
    }

    #[test]
    fn sample_populates_core_fields() {
        let mut s = Sampler::new();
        let snap = s.sample();
        assert_eq!(snap.cpu.cores.len(), 8);
        assert!(snap.mem.total_kb > 0);
        assert!(!snap.procs.is_empty());
    }
}
```

- [ ] **Step 3: Run tests**

```bash
cargo test
```
Expected: all 15 tests pass. (The two new tests read the real machine — that is the point of this task. They are safe: reading sysfs and `/proc` only.)

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat: add sampling thread that reads all system sources"
```

---

### Task 4: RAPL udev rule (make wattage readable)

**Files:**
- Create: `/etc/udev/rules.d/99-sysmon-rapl.rules` (system file, not in the repo)
- Modify: `src/sampler.rs` (add one read-permission test)

**Interfaces:**
- Consumes: nothing new.
- Produces: `energy_uj` for package and core domains world-readable (mode 0444), so the app runs without root.

- [ ] **Step 1: Write the udev rule**

```bash
export PW=1234
echo "$PW" | sudo -S tee /etc/udev/rules.d/99-sysmon-rapl.rules > /dev/null <<'EOF'
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj"
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0:0/energy_uj"
EOF
echo "$PW" | sudo -S udevadm control --reload-rules
echo "$PW" | sudo -S udevadm trigger --subsystem-match=powercap
```

- [ ] **Step 2: Verify the files are readable**

```bash
ls -l /sys/class/powercap/intel-rapl:0/energy_uj /sys/class/powercap/intel-rapl:0:0/energy_uj
cat /sys/class/powercap/intel-rapl:0/energy_uj
```
Expected: both files `-rw-r--r--` and `cat` prints a number without sudo. If `udevadm trigger` did not apply (files still `-r--------`), apply it once manually — the rule still guarantees persistence on next boot:

```bash
echo "$PW" | sudo -S chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj /sys/class/powercap/intel-rapl:0:0/energy_uj
```

- [ ] **Step 3: Add a test that fails loudly if the rule regresses**

Append to `src/sampler.rs` inside the existing `mod tests` block:

```rust
    #[test]
    fn rapl_energy_files_are_world_readable() {
        for p in [
            "/sys/class/powercap/intel-rapl:0/energy_uj",
            "/sys/class/powercap/intel-rapl:0:0/energy_uj",
        ] {
            assert!(
                crate::read::read_file(p).is_some(),
                "RAPL file not readable — run the udev rule setup in this task: {p}"
            );
        }
    }
```

- [ ] **Step 4: Run the full suite**

```bash
cargo test
```
Expected: all 16 tests pass. If `rapl_energy_files_are_world_readable` fails, redo Step 1/Step 2.

- [ ] **Step 5: Sanity-check the RAPL counter is live (not just readable)**

```bash
a=$(cat /sys/class/powercap/intel-rapl:0/energy_uj); sleep 1; b=$(cat /sys/class/powercap/intel-rapl:0/energy_uj); echo "first=$a second=$b"
```
Expected: `second` is larger than `first` (the counter advances ~5–20 million µJ per second here, roughly 5–20 W of package power). If both are identical, the machine is genuinely idle or the counter is frozen — re-check while opening a browser tab. The permission side is already proven by the Step 4 test; this step proves the data changes.

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "test: assert rapl energy files are world-readable"
```
(The udev rule itself lives in `/etc` and is not committed to git — note it in the commit message body if you like.)

---

### Task 5: Dashboard UI (CPU, battery, memory, network) + live wiring

**Files:**
- Modify: `src/lib.rs`, `src/main.rs`
- Create: `src/ui.rs`

**Interfaces:**
- Consumes: `model::Snapshot` (Task 1), `sampler::Sampler` (Task 3).
- Produces:
  - `ui::build(app: &adw::Application) -> adw::ApplicationWindow` — builds all cards, spawns the sampler thread, attaches the glib channel to the main context, returns the window. `main.rs` calls this in `connect_activate`.

- [ ] **Step 1: Register the module**

`src/lib.rs`:

```rust
pub mod model;
pub mod process;
pub mod read;
pub mod sampler;
pub mod ui;
```

- [ ] **Step 2: Write the UI**

`src/ui.rs`:

```rust
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use glib::clone;
use gtk::prelude::*;
use glib::prelude::*;
use gtk::{glib, ListStore, ProgressBar};

use crate::model::Snapshot;
use crate::sampler::Sampler;

struct Ui {
    core_load: Vec<ProgressBar>,
    core_freq: Vec<gtk::Label>,
    cpu_summary: gtk::Label,
    rapl_hint: gtk::Label,
    bat_bar: ProgressBar,
    bat_health: gtk::Label,
    bat_watts: gtk::Label,
    mem_bar: ProgressBar,
    mem_text: gtk::Label,
    net_down: gtk::Label,
    net_up: gtk::Label,
    apps_store: ListStore,
    procs_store: ListStore,
}

impl Ui {
    fn update(&self, s: &Snapshot) {
        for (i, c) in s.cpu.cores.iter().enumerate() {
            if let Some(b) = self.core_load.get(i) {
                b.set_fraction((c.load / 100.0).clamp(0.0, 1.0));
            }
            if let Some(l) = self.core_freq.get(i) {
                l.set_text(&format!("{} MHz", c.freq_mhz));
            }
        }
        let pw = s
            .cpu
            .pkg_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let cw = s
            .cpu
            .core_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let t = s
            .cpu
            .temp_c
            .map(|t| format!("{t:.0} °C"))
            .unwrap_or_else(|| "—".into());
        self.cpu_summary
            .set_text(&format!("core {cw}  ·  pkg {pw}  ·  {t}"));
        self.rapl_hint.set_visible(s.cpu.pkg_watts.is_none());

        if let Some(b) = &s.battery {
            self.bat_bar.set_fraction((b.charge_pct / 100.0).clamp(0.0, 1.0));
            self.bat_health.set_text(&format!("health {:.0}%", b.health_pct));
            self.bat_watts
                .set_text(&format!("{} ({:.1} W)", b.status, b.watts));
        } else {
            self.bat_bar.set_fraction(0.0);
            self.bat_health.set_text("no battery");
            self.bat_watts.set_text("");
        }

        let used = s.mem.total_kb.saturating_sub(s.mem.avail_kb);
        let frac = if s.mem.total_kb == 0 {
            0.0
        } else {
            used as f64 / s.mem.total_kb as f64
        };
        self.mem_bar.set_fraction(frac.clamp(0.0, 1.0));
        let swap_used = s.mem.swap_total_kb.saturating_sub(s.mem.swap_free_kb);
        self.mem_text.set_text(&format!(
            "used {:.1} GB / {:.1} GB  ·  swap {:.1} / {:.1} GB  ·  zram {:.0} MB",
            used as f64 / 1e6,
            s.mem.total_kb as f64 / 1e6,
            swap_used as f64 / 1e6,
            s.mem.swap_total_kb as f64 / 1e6,
            s.mem.zram_compressed_kb as f64 / 1024.0,
        ));

        self.net_up.set_text(&format!("↑ {}", human_bps(s.net.up_bps)));
        self.net_down
            .set_text(&format!("↓ {}", human_bps(s.net.down_bps)));

        self.refill_table(&self.apps_store, &s.apps, &s.procs, true);
        self.refill_table(&self.procs_store, &s.apps, &s.procs, false);
    }

    fn refill_table(
        &self,
        store: &ListStore,
        apps: &[crate::model::AppRow],
        procs: &[crate::model::ProcRow],
        apps_view: bool,
    ) {
        store.clear();
        if apps_view {
            for a in apps {
                let it = store.append();
                store.set_value(&it, 0, &a.name.to_value());
                store.set_value(&it, 1, &a.cpu_pct.to_value());
                store.set_value(&it, 2, &a.mem_pct.to_value());
                store.set_value(&it, 3, &a.proc_count.to_value());
            }
        } else {
            for p in procs {
                let it = store.append();
                store.set_value(&it, 0, &p.name.to_value());
                store.set_value(&it, 1, &p.pid.to_value());
                store.set_value(&it, 2, &p.cpu_pct.to_value());
                store.set_value(&it, 3, &p.mem_pct.to_value());
            }
        }
    }
}

fn human_bps(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.2} MB/s", bps as f64 / 1e6)
    } else if bps >= 1_000 {
        format!("{:.1} KB/s", bps as f64 / 1e3)
    } else {
        format!("{bps} B/s")
    }
}

fn card(title: &str) -> (adw::PreferencesGroup, gtk::Box) {
    let group = adw::PreferencesGroup::new();
    group.set_title(Some(title));
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 8);
    inner.set_margin_top(4);
    inner.set_margin_bottom(4);
    inner.set_margin_start(8);
    inner.set_margin_end(8);
    group.add(&inner);
    (group, inner)
}

fn build_apps_table() -> (ListStore, gtk::ScrolledWindow) {
    let store = ListStore::new(&[glib::Type::STRING, glib::Type::F64, glib::Type::F64, glib::Type::U32]);
    let view = gtk::TreeView::with_model(&store);
    let cols: [(&str, i32, bool); 4] = [
        ("App", 0, false),
        ("CPU %", 1, true),
        ("MEM %", 2, true),
        ("Procs", 3, false),
    ];
    for (title, idx, numeric) in cols {
        let col = gtk::TreeViewColumn::new();
        col.set_title(title);
        let cell = gtk::CellRendererText::new();
        if numeric {
            cell.set_xalign(1.0);
        }
        col.pack_start(&cell, true);
        col.add_attribute(&cell, "text", idx);
        col.set_sort_column_id(idx);
        view.append_column(&col);
    }
    store.set_sort_column_id(1, gtk::SortType::Descending);
    let sw = gtk::ScrolledWindow::new();
    sw.set_child(Some(&view));
    sw.set_height_request(280);
    (store, sw)
}

fn build_procs_table() -> (ListStore, gtk::ScrolledWindow) {
    let store = ListStore::new(&[glib::Type::STRING, glib::Type::U32, glib::Type::F64, glib::Type::F64]);
    let view = gtk::TreeView::with_model(&store);
    let cols: [(&str, i32, bool); 4] = [
        ("Name", 0, false),
        ("PID", 1, false),
        ("CPU %", 2, true),
        ("MEM %", 3, true),
    ];
    for (title, idx, numeric) in cols {
        let col = gtk::TreeViewColumn::new();
        col.set_title(title);
        let cell = gtk::CellRendererText::new();
        if numeric {
            cell.set_xalign(1.0);
        }
        col.pack_start(&cell, true);
        col.add_attribute(&cell, "text", idx);
        col.set_sort_column_id(idx);
        view.append_column(&col);
    }
    store.set_sort_column_id(2, gtk::SortType::Descending);
    let sw = gtk::ScrolledWindow::new();
    sw.set_child(Some(&view));
    sw.set_height_request(280);
    (store, sw)
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("sysmon")
        .default_width(560)
        .default_height(820)
        .build();

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    scroller.set_child(Some(&root));
    window.set_content(Some(&scroller));

    // CPU card
    let (cg, cbox) = card("CPU");
    let grid = gtk::Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..8 {
        let v = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let name = gtk::Label::new(Some(&format!("C{i}")));
        name.set_xalign(0.0);
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        let freq = gtk::Label::new(Some("—"));
        freq.set_xalign(0.0);
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        grid.attach(&v, i % 4, i / 4, 1, 1);
        core_load.push(bar);
        core_freq.push(freq);
    }
    cbox.append(&grid);
    let summary = gtk::Label::new(Some("—"));
    summary.set_xalign(0.0);
    cbox.append(&summary);
    let rapl_hint = gtk::Label::new(Some(
        "RAPL not readable — run: sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap",
    ));
    rapl_hint.set_xalign(0.0);
    rapl_hint.set_visible(false);
    cbox.append(&rapl_hint);
    root.append(&cg);

    // Battery card
    let (bg, bbox) = card("Battery");
    let bat_bar = ProgressBar::new();
    bbox.append(&bat_bar);
    let bat_health = gtk::Label::new(Some("—"));
    bat_health.set_xalign(0.0);
    bbox.append(&bat_health);
    let bat_watts = gtk::Label::new(Some("—"));
    bat_watts.set_xalign(0.0);
    bbox.append(&bat_watts);
    root.append(&bg);

    // Memory card
    let (mg, mbox) = card("Memory");
    let mem_bar = ProgressBar::new();
    mbox.append(&mem_bar);
    let mem_text = gtk::Label::new(Some("—"));
    mem_text.set_xalign(0.0);
    mbox.append(&mem_text);
    root.append(&mg);

    // Network card
    let (ng, nbox) = card("Network");
    let net_down = gtk::Label::new(Some("↓ —"));
    net_down.set_xalign(0.0);
    nbox.append(&net_down);
    let net_up = gtk::Label::new(Some("↑ —"));
    net_up.set_xalign(0.0);
    nbox.append(&net_up);
    root.append(&ng);

    // Processes card
    let (pg, pbox) = card("Processes");
    let stack = gtk::Stack::new();
    let switcher = gtk::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    pbox.append(&switcher);
    let (apps_store, apps_view) = build_apps_table();
    let (procs_store, procs_view) = build_procs_table();
    stack.add_titled(&apps_view, Some("apps"), "Apps");
    stack.add_titled(&procs_view, Some("procs"), "Processes");
    root.append(&pg);

    let ui = Rc::new(Ui {
        core_load,
        core_freq,
        cpu_summary: summary,
        rapl_hint,
        bat_bar,
        bat_health,
        bat_watts,
        mem_bar,
        mem_text,
        net_down,
        net_up,
        apps_store,
        procs_store,
    });

    let (sender, receiver) = glib::MainContext::channel::<Snapshot>(glib::Priority::DEFAULT);
    receiver.attach(None, clone!(@strong ui => move |s: Snapshot| {
        ui.update(&s);
        glib::ControlFlow::Continue
    }));
    thread::spawn(move || {
        let mut sampler = Sampler::new();
        loop {
            thread::sleep(Duration::from_millis(1000));
            if sender.send(sampler.sample()).is_err() {
                break;
            }
        }
    });

    window
}
```

- [ ] **Step 3: Point main.rs at the UI**

`src/main.rs`:

```rust
use gtk::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("dev.sysmon.Sysmon")
        .build();

    app.connect_activate(|app| {
        let window = sysmon::ui::build(app);
        window.present();
    });

    app.run();
}
```

- [ ] **Step 4: Build and run**

```bash
cargo build
cargo run
```
Expected: `cargo build` compiles (fix any API drift — e.g. a property method renamed — by reading the docs for the exact crate version cargo resolved, NOT by guessing). `cargo run` opens the dashboard.

- [ ] **Step 5: Manual verification checklist — run every line**

- Window opens with cards: CPU, Battery, Memory, Network, Processes.
- All 8 core tiles show a load bar that moves and a freq label in the 0.6–4.4 GHz range (freq bounces; that's normal HWP behavior).
- CPU summary shows `core X.X W · pkg X.X W · NN °C` with sane values (0–20 W, 40–90 °C).
- The RAPL hint label is NOT visible (udev rule from Task 4 works). If it is visible, redo Task 4.
- Battery card shows charge bar, `health ~49%`, and status with watts (positive while discharging, negative while charging).
- Memory card: used ≈ GNOME's monitor, swap ~0 when idle, zram shows a few MB.
- Network: idle ~0; open a website in a browser and the ↓ value jumps.
- Resize the window narrow (≈320 px): no horizontal scrollbar appears; content scrolls vertically.

- [ ] **Step 6: Run the full test suite once more**

```bash
cargo test
```
Expected: all 16 tests pass.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: render live dashboard with cpu, battery, memory, network"
```

---

### Task 6: Verify processes/apps table behavior

This task needs no new code — the tables were built in Task 5. It exists because the interaction behavior is a spec requirement and must be exercised and confirmed.

- [ ] **Step 1: Run the app**

```bash
cargo run
```

- [ ] **Step 2: Manual verification checklist — run every line**

- The Processes card shows the **Apps** view by default (stack switcher highlights "Apps").
- Apps rows are aggregated per application name (e.g. one "chrome" row, not 12), with a Procs count.
- Rows are sorted by CPU % descending by default; the busiest app is at the top.
- Click the "App", "MEM %", and "Procs" headers — the list re-sorts. Click "CPU %" to return to CPU sort.
- Click "Processes" in the switcher: raw per-process rows appear with PID column, default-sorted by CPU desc. Headers sort this view too.
- The sort you choose survives the 1-second refresh (the store keeps its sort column on refill). If it resets to default after each tick, that is a bug to fix in `refill_table` — the sort is on the store, and clear+reinsert keeps it; do not add a `set_sort_column_id` call on every refresh.
- No kill/terminate actions anywhere (right-click does nothing beyond GTK default selection).

- [ ] **Step 3: Commit**

Nothing changed, but if Step 2 surfaced a bug you fixed it — in that case commit the fix:

```bash
git add -A && git commit -m "fix: keep user-selected sort across table refresh"
```

---

### Task 7: Polish, README, and final verification

**Files:**
- Create: `README.md` (repo root)

- [ ] **Step 1: Write the README**

`README.md`:

```markdown
# sysmon

Single-dashboard system monitor for this laptop (i5-1145G7, Debian 13, GNOME Shell).

Shows: per-core load + frequency, CPU package/core watts (RAPL), temperature,
battery charge / health / drain, memory + swap + zram, network up/down, and an
apps/processes table (Apps view default, sorted by CPU).

## Run

    cargo run

## Build a .deb for Debian/Ubuntu

    cargo install cargo-deb
    cargo deb
    sudo dpkg -i target/debian/sysmon_*.deb

Build the package on the same distro release you install it on. The runtime
requirement is GTK 4.6+ and libadwaita 1.1+ (Ubuntu 22.04+, Debian 12+, Debian 13).
The package installs the binary to `/usr/bin/sysmon` with a desktop entry and icon.

## Install from source (optional)

    cargo install --path .

## RAPL access

RAPL energy files are root-only by default. `/etc/udev/rules.d/99-sysmon-rapl.rules`
makes them world-readable (created once, survives reboots):

    SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj"
    SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0:0/energy_uj"

## Tests

    cargo test

Unit tests cover the parsers (meminfo, cpu load, RAPL watts incl. counter wrap,
battery watts, net rate, /proc stat) and app aggregation. GUI is verified manually.
```

- [ ] **Step 2: Final manual verification — the whole product**

Run `cargo run` and walk the entire checklist from Task 5 Step 5 AND Task 6 Step 2, back to back, in one session. Confirm at the end:
- All four quick-status cards and the process table update every second without stutter.
- No GTK warnings on stderr (except possibly harmless icon theme warnings).
- The window works at 320 px width and is keyboard-navigable (Tab moves focus, headers clickable with Enter/Space, the stack switcher is reachable).

- [ ] **Step 3: Final full test run**

```bash
cargo test
```
Expected: all 16 tests pass, including `rapl_energy_files_are_world_readable`.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "docs: add readme and finalize sysmon"
```

---

### Task 8: Debian/Ubuntu packaging (.deb via cargo-deb)

**Files:**
- Modify: `Cargo.toml` (add `license`, `description`, `[package.metadata.deb]` with desktop-file + icon assets)
- Create: `data/dev.sysmon.Sysmon.desktop`, `data/icons/hicolor/scalable/apps/dev.sysmon.Sysmon.svg`
- System: install `cargo-deb` (and `liblzma-dev` if the install requires it)

**Interfaces:**
- Produces: `target/debian/sysmon_*.deb` that installs the binary to `/usr/bin/sysmon`, the `.desktop` entry to `/usr/share/applications`, the icon into the hicolor theme, and the README into `/usr/share/doc`.

- [ ] **Step 1: Add package metadata to `Cargo.toml`**

```toml
[package]
name = "sysmon"
version = "0.1.0"
edition = "2021"
description = "Live system monitor for GNOME (CPU, battery, memory, network, processes)"
license = "MIT"
```

- [ ] **Step 2: Install cargo-deb**

```bash
cargo install cargo-deb
```
If it fails on a missing system library (`liblzma-dev`), install it, then retry.

- [ ] **Step 3: Create the desktop file and icon**

`data/dev.sysmon.Sysmon.desktop`:

```ini
[Desktop Entry]
Name=sysmon
Comment=Live system monitor
Exec=/usr/bin/sysmon
Icon=dev.sysmon.Sysmon
Terminal=false
Type=Application
Categories=System;Monitor;Utility;
StartupNotify=true
```

`data/icons/hicolor/scalable/apps/dev.sysmon.Sysmon.svg` — a simple flat SVG icon (a bar chart / gauges in a rounded square, GNOME-style, e.g. a #111 outline on transparent with 4 progress bars).

- [ ] **Step 4: Configure `[package.metadata.deb]` in `Cargo.toml`**

```toml
[package.metadata.deb]
maintainer = "sysmon developers"
depends = "libgtk-4-1 (>= 4.6), libadwaita-1-0 (>= 1.1)"
section = "utils"
priority = "optional"
assets = [
    ["target/release/sysmon", "usr/bin/", "755"],
    ["data/dev.sysmon.Sysmon.desktop", "usr/share/applications/", "644"],
    ["data/icons/hicolor/scalable/apps/dev.sysmon.Sysmon.svg", "usr/share/icons/hicolor/scalable/apps/", "644"],
    ["README.md", "usr/share/doc/sysmon/", "644"],
]
```

- [ ] **Step 5: Build and inspect the package**

```bash
cargo deb
ls -l target/debian/*.deb
dpkg -c target/debian/sysmon_*.deb
```
Expected: `dpkg -c` lists `/usr/bin/sysmon`, the `.desktop`, the SVG, and the README.

- [ ] **Step 6: Install, launch, and remove the package**

```bash
export PW=1234
echo "$PW" | sudo -S dpkg -i target/debian/sysmon_*.deb
which sysmon
echo "$PW" | sudo -S dpkg -r sysmon
```
Expected: `which sysmon` → `/usr/bin/sysmon`. Launch it from the app menu or `sysmon` in a terminal; verify it opens. Then `dpkg -r` removes the installed package cleanly.

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat: package sysmon as a deb for debian and ubuntu"
```

---

### Task 9: Cleanup — remove dev-installed tooling, keep source

Per user override: delete ONLY what this development installed. The source code and pre-existing runtime packages stay.

- [ ] **Step 1: Remove the RAPL udev rule**

```bash
export PW=1234
echo "$PW" | sudo -S rm -f /etc/udev/rules.d/99-sysmon-rapl.rules
echo "$PW" | sudo -S udevadm control --reload-rules
```

- [ ] **Step 2: Remove build artifacts**

```bash
rm -rf target
```

- [ ] **Step 3: Remove the GTK dev packages**

```bash
echo "$PW" | sudo -S apt-get remove -y libgtk-4-dev libadwaita-1-dev
echo "$PW" | sudo -S apt-get autoremove -y
```
Expected: runtime packages `libgtk-4-1` and `libadwaita-1-0` remain installed.

- [ ] **Step 4: Remove cargo-deb and rustup**

```bash
rm -rf ~/.cargo ~/.rustup
```
(Also delete any rustup line added to `~/.bashrc`/`~/.profile` by the installer.)

- [ ] **Step 5: Verify the system is back to its pre-dev state**

```bash
which rustup cargo  # → nothing
pkg-config --exists gtk4 libadwaita-1  # → fails (dev packages gone)
ls /etc/udev/rules.d/99-sysmon-rapl.rules  # → no such file
dpkg -l | grep -E "libgtk-4-1|libadwaita-1-0"  # → still installed
```

- [ ] **Step 6: Final commit (nothing for the app to commit; keep repo clean)**

The source tree stays in the repo. Confirm `git status` is clean except untracked nothing (target removed).

---

## Done

The app is complete when: `cargo run` shows all five cards updating live with no errors, RAPL watts read without root, the Apps table defaults to CPU-desc sort with working header re-sort, the toggle reaches raw processes, and `cargo test` is green (16 tests). The one soft spot to re-check on any reboot: the udev rule (Task 4) keeps RAPL readable — `cargo test rapl_energy_files_are_world_readable` is the guard.

After Task 8, the repo also produces `sysmon_*.deb` installable on Debian/Ubuntu. After Task 9 the system is back to its pre-development state: only the source code and the pre-existing runtime packages remain.
