# AGENTS.md

sysmon is a Rust + GTK4/libadwaita desktop system monitor. One binary, no library. It shows CPU, battery, memory, network, and a processes table, refreshing every second.

## Build

You need the GTK4 and libadwaita dev packages before anything compiles:
    sudo apt-get install -y libgtk-4-dev libadwaita-1-dev pkg-config
The toolchain has to support edition 2024 (see Cargo.toml). `cargo run` opens a GUI, so it needs a real X11/Wayland display and won't run headless.

## Tests

Tests live in inline `#[cfg(test)]` modules and read real /proc and /sys hardware, so they only run on Linux. Two are environment-sensitive and handled gracefully:
- `sample_populates_core_fields` uses `online_count()`, so it passes on any machine.
- `rapl_energy_files_are_world_readable` skips when RAPL isn't readable instead of failing.
CI (`.github/workflows/ci.yml`) installs the dev packages, then runs build, clippy, and `cargo test` on Ubuntu.

## RAPL / CPU wattage

The RAPL energy files under /sys/class/powercap/intel-rapl:0*/energy_uj are root-only by default, so CPU watts show "no access" until permissions are loosened. The udev rule ships at data/99-sysmon-rapl.rules and is installed by the .deb (postinst reloads udev). For local dev, copy it once:
    SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj"
    SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0:0/energy_uj"
    sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap

## Hardware discovery

Everything is discovered at `Sampler::new()` so the binary runs on any Linux box, not just the dev machine:
- Cores: `online_count()` reads /sys/devices/system/cpu/online
- RAPL: `discover_rapl()` scans /sys/class/powercap by zone name (package-0, core, ...)
- CPU temp: `discover_cpu_thermal_zone()` prefers x86_pkg_temp, then cpu_thermal, then cpu
- Battery: `discover_battery()` looks for a power_supply entry of type Battery
- clk_tck / page size: `libc::sysconf`
- Network: only `lo` is hardcoded (loopback)

## Packaging

.deb builds use cargo-deb. `depends` is `libgtk-4-1 (>= 4.6), libadwaita-1-0 (>= 1.1)`. The `[package.metadata.deb]` block in Cargo.toml installs the binary, icon, .desktop, the RAPL udev rule, and LICENSE.
License is custom source-available (see LICENSE): read/run/share verbatim copies freely, but no modified redistribution, rebranding, or sale. Same terms cover the icon.

## Releases

Pushing to main or master runs `.github/workflows/release.yml`: it takes the version from Cargo.toml, tags `v<version>`, builds the .deb, generates notes from conventional commits via git-cliff, and publishes a GitHub release with the .deb attached. If the tag already exists it skips, so bump `version` in Cargo.toml before pushing to cut a new release. Use semver (feat=minor, fix=patch, breaking=major) and keep commit messages in conventional-commit form since the changelog is generated from them.

## Layout

- `main.rs` — `adw::Application` entrypoint (app id dev.sysmon.Sysmon)
- `lib.rs` — modules: model, process, read, sampler, ui
- `read.rs` — parsers/math over /proc and /sys
- `sampler.rs` — `Sampler` reads hardware each tick; emits QuickSnapshot/ProcSnapshot
- `process.rs` — /proc scan and app grouping
- `ui.rs` — widgets, CSS, 1s refresh loop
- `model.rs` — data structs
