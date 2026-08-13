# AGENTS.md

Rust + GTK4/libadwaita desktop system monitor (`sysmon`), edition 2024.
Single binary, no separate library. Live readout of CPU, battery, memory, network, and an apps/processes table, refreshed every 1 s.

## Build & run prerequisites

- Requires system dev packages before `cargo build`/`cargo test` will even compile:
  `sudo apt-get install -y libgtk-4-dev libadwaita-1-dev` (plus `pkg-config`).
- Toolchain must be recent enough for `edition = "2024"` (see `Cargo.toml`).
- `cargo run` launches the GUI and **needs a display** (X11/Wayland session). It will not run headless.

## Tests

- Tests are inline `#[cfg(test)]` modules in the source files; the `tests/` dir is empty. Run with `cargo test`.
- Tests read **real Linux hardware** files (`/proc`, `/sys`): `scan_processes`, `sample_quick`, `sample_procs` hit the live system. They are not portable:
  - `sample_populates_core_fields` asserts `cores.len() == online_count()` (discovered at runtime), so it passes on any machine, not just the 8-core target.
  - `rapl_energy_files_are_world_readable` discovers the RAPL zones and skips (instead of failing) when `energy_uj` isn't readable, so it passes on machines/CI without RAPL access; install `data/99-sysmon-rapl.rules` for full wattage.
- CI: `.github/workflows/ci.yml` builds, runs clippy, and runs `cargo test` on Ubuntu (installs `libgtk-4-dev`/`libadwaita-1-dev`).

## RAPL / CPU wattage setup (not in repo)

RAPL energy files under `/sys/class/powercap/intel-rapl:0*/energy_uj` are root-only by default. Without this, CPU package/core watts show "no access". The rule is shipped as `data/99-sysmon-rapl.rules` and installed to `/etc/udev/rules.d/` by the `.deb` (with a `postinst` that reloads udev). For a local setup, copy it once:

```
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj"
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0:0/energy_uj"
```

Then `sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap`. Survives reboots; re-check after reboot with `cargo test rapl_energy_files_are_world_readable`.

## Hardware discovery (not machine-pinned)

All hardware is auto-discovered at `Sampler::new()` time so the binary runs on arbitrary Linux hardware, not just the dev laptop:

- **Cores**: `online_count()` reads `/sys/devices/system/cpu/online`.
- **RAPL package/core watts**: `discover_rapl()` scans `/sys/class/powercap`, reads each zone's `name` (`package-0`, `core`, …) and `max_energy_range_uj`. Works for Intel layouts; other vendors are picked up if the kernel exposes them.
- **CPU temperature**: `discover_cpu_thermal_zone()` scans `/sys/class/thermal` `type` files, preferring `x86_pkg_temp`, then `cpu_thermal`, then `cpu`.
- **Battery**: `discover_battery()` scans `/sys/class/power_supply` for a `type == "Battery"` entry.
- **`clk_tck` / page size**: read via `libc::sysconf(_SC_CLK_TCK)` / `_SC_PAGESIZE` (no longer hardcoded).
- **Network**: `lo` is the only still-hardcoded exclusion (universal loopback, safe to keep).

## Packaging

`.deb` builds use `cargo-deb`; the control `depends` line is `libgtk-4-1 (>= 4.6), libadwaita-1-0 (>= 1.1)` (broad runtime compatibility, not the build machine's 4.18/1.7). The `[package.metadata.deb]` block in `Cargo.toml` installs the binary, the hicolor icon (`data/icons/hicolor/512x512/apps/dev.sysmon.Sysmon.png`), the `.desktop` file (`dev.sysmon.Sysmon.desktop`), and `LICENSE`.

Licensing: this project ships under a **custom source-available license** (`LICENSE`), not an OSI open-source license — it permits reading/running/sharing verbatim copies but forbids modified redistribution, re-labeling/re-branding, and commercial sale. The icon is covered by the same terms.

## Releases & versioning

- Releases are automatic: pushing to `main`/`master` triggers `.github/workflows/release.yml`, which tags `v<Cargo.toml version>`, builds the `.deb`, generates changelog notes from conventional commits (git-cliff), and publishes a GitHub release with the `.deb` attached.
- **Keep the version current:** whenever you make user-facing changes (features, fixes, breaking changes), bump `version` in `Cargo.toml` before pushing. If you don't, the release step sees the tag already exists and skips — so a forgotten bump means no release.
- Use semver: `feat` → minor, `fix` → patch, breaking change → major. Commit messages must stay conventional-commit style (`feat:`, `fix:`, `chore:`, …) since the changelog is generated from them.

## Architecture (quick map)

- `main.rs` — `adw::Application` entrypoint, app id `dev.sysmon.Sysmon`.
- `lib.rs` — module list: `model`, `process`, `read`, `sampler`, `ui`.
- `read.rs` — pure parsers/math over `/proc` & `/sys` strings (the well-tested core).
- `sampler.rs` — `Sampler` reads hardware each tick; emits `QuickSnapshot`/`ProcSnapshot`.
- `process.rs` — `/proc` scan + app grouping.
- `ui.rs` — GTK4/libadwaita widgets, CSS, 1 s refresh loop. Core chips are built from `online_count()`, not a hardcoded count.
- `model.rs` — plain data structs.
