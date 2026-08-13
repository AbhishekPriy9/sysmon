# sysmon

A Rust + GTK4/libadwaita desktop system monitor for Linux. Single binary, no separate library. Live readout of CPU, battery, memory, network, and an apps/processes table, refreshed every 1 second.

## Features

- CPU usage per-core, package/core wattage (RAPL), and CPU temperature
- Battery state and charge
- Memory usage
- Network throughput
- Apps/processes table with grouping

## Build prerequisites

```
sudo apt-get install -y libgtk-4-dev libadwaita-1-dev pkg-config
```

You also need a recent Rust toolchain (edition 2024).

## Build & run

```
cargo build
cargo run
```

`cargo run` launches the GUI and requires a display (X11/Wayland session); it will not run headless.

## Tests

Tests are inline `#[cfg(test)]` modules in the source files. They read real Linux hardware files (`/proc`, `/sys`) and are not portable, so they must be run on Linux:

```
cargo test
```

## CPU wattage (RAPL) setup

RAPL energy files under `/sys/class/powercap/intel-rapl:0*/energy_uj` are root-only by default, so package/core watts show "no access" until you relax permissions. The shipped rule `data/99-sysmon-rapl.rules` fixes this:

```
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0/energy_uj"
SUBSYSTEM=="powercap", KERNEL=="intel-rapl:0:0", RUN+="/bin/chmod 0444 /sys/class/powercap/intel-rapl:0:0/energy_uj"
```

Install it once:

```
sudo cp data/99-sysmon-rapl.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap
```

Survives reboots. The `.deb` package installs this rule automatically via a `postinst` script.

## Packaging

`.deb` builds use `cargo-deb`. The package installs the binary, the hicolor icon, the `.desktop` file, and `LICENSE`.

## License

Source-available license (see `LICENSE`). Reading, running, and sharing verbatim copies is permitted; modified redistribution, re-labeling/re-branding, and commercial sale are forbidden.
