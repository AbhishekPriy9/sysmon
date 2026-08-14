# sysmon

A live system monitor for Linux desktops, built with GTK4/libadwaita — highly
recommended for **Dell laptops with Intel CPUs running a Debian-family distro**.
It keeps an eye on your CPU, battery, memory, and network — plus an
apps/processes table — refreshed every second.

## Features

- CPU usage per core, package/core wattage (RAPL), and temperature
- Battery state and charge
- Memory usage
- Network throughput
- Apps and processes table with grouping

## Tested hardware

sysmon is developed and verified on:

- **Dell Latitude 5420** — Intel Core i5, 11th gen
- **Dell Inspiron 15** — Intel Core i5-7200U (7th gen)

Both running Debian-family distros (Debian/Ubuntu). Other distros and hardware
may work, but aren't validated — CPU temperature, RAPL wattage, and battery
sensors follow Intel/Dell conventions, so readings are most accurate there.

## Requirements

- Dell laptop with an Intel CPU — recommended and best tested
- A Debian-family distro (Debian, Ubuntu, and derivatives) — packaged as `.deb` only
- A desktop session (X11 or Wayland)
- Dependencies (GTK4, libadwaita) are installed automatically by apt

## Download & install

Download the latest `sysmon_*.deb` from the [releases page](https://github.com/AbhishekPriy9/sysmon/releases), then:

```
cd ~/Downloads
sudo apt install ./sysmon_0.1.1-1_amd64.deb
```

Use the exact filename you downloaded (the version number changes with each release).

Prefer manual install?

```
sudo dpkg -i sysmon_0.1.1-1_amd64.deb
sudo apt-get install -f   # only if it reports missing dependencies
```

`apt install ./…deb` is recommended — it resolves dependencies automatically.

### Uninstall

```
sudo apt remove sysmon
```

## Launching

- From your app menu, search for **sysmon**
- Or from a terminal: `sysmon`

## A note on CPU wattage

Package/core watts come from the Intel RAPL power-cap registers, which Linux locks
down by default. The package installs a udev rule that grants the app read access
automatically, so wattage works out of the box. If you ever see "no access" for
watts, it means that rule isn't active on your system.

## License

Source-available license (see `LICENSE`). Reading, running, and sharing verbatim
copies is permitted; modified redistribution, re-branding, and sale are forbidden.

## Support

Bugs or feature requests: open an issue at
https://github.com/AbhishekPriy9/sysmon/issues
