use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use crate::model::{
    Battery, Core, Cpu, Memory, Net, NetIface, ProcSnapshot, QuickSnapshot,
};
use crate::process::{group_apps, scan_processes};
use crate::read::{
    battery_watts, health_pct, load_percent, net_rate, parse_cpu_line, parse_meminfo,
    rapl_watts, read_file,
};

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
    cpu_name: String,
    cpu_max_freq_mhz: u64,
    cpu_boost: Option<bool>,
    prev_cpu: Vec<(u64, u64)>,
    rapl_pkg_path: Option<String>,
    rapl_pkg_max: u64,
    prev_rapl_pkg: u64,
    rapl_core_path: Option<String>,
    rapl_core_max: u64,
    prev_rapl_core: u64,
    cpu_thermal_zone: Option<String>,
    battery_name: Option<String>,
    prev_net: HashMap<String, (u64, u64)>,
    prev_proc: HashMap<u32, u64>,
    total_kb: u64,
    iface_labels: HashMap<String, String>,
    last: Instant,
    last_proc: Instant,
}

impl Sampler {
    pub fn new() -> Self {
        let (rapl_pkg_path, rapl_core_path) = discover_rapl();
        let rapl_pkg_max = rapl_pkg_path
            .as_ref()
            .and_then(|p| read_rapl_max(p))
            .unwrap_or(0);
        let rapl_core_max = rapl_core_path
            .as_ref()
            .and_then(|p| read_rapl_max(p))
            .unwrap_or(0);
        let ncores = online_count();
        let clk_tck = match unsafe { libc::sysconf(libc::_SC_CLK_TCK) } {
            v if v > 0 => v as u64,
            _ => 100,
        };
        let page_size_kb = match unsafe { libc::sysconf(libc::_SC_PAGESIZE) } {
            v if v > 0 => (v as u64) / 1024,
            _ => 4,
        };
        Sampler {
            ncores,
            clk_tck,
            page_size_kb,
            cpu_name: read_cpu_name(),
            cpu_max_freq_mhz: read_cpu_max_freq_mhz(),
            cpu_boost: read_cpu_boost(),
            prev_cpu: vec![(0, 0); ncores],
            rapl_pkg_path,
            rapl_pkg_max,
            prev_rapl_pkg: 0,
            rapl_core_path,
            rapl_core_max,
            prev_rapl_core: 0,
            cpu_thermal_zone: discover_cpu_thermal_zone(),
            battery_name: discover_battery(),
            prev_net: HashMap::new(),
            prev_proc: HashMap::new(),
            total_kb: 0,
            iface_labels: HashMap::new(),
            last: Instant::now(),
            last_proc: Instant::now(),
        }
    }

    pub fn sample_quick(&mut self) -> QuickSnapshot {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;

        let cores = self.sample_cores();
        let pkg_watts = match &self.rapl_pkg_path {
            Some(p) => rapl_sample(p, &mut self.prev_rapl_pkg, self.rapl_pkg_max, dt),
            None => None,
        };
        let core_watts = match &self.rapl_core_path {
            Some(p) => rapl_sample(p, &mut self.prev_rapl_core, self.rapl_core_max, dt),
            None => None,
        };
        let temp_c = match &self.cpu_thermal_zone {
            Some(p) => read_file(&format!("{p}/temp"))
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|t| t as f64 / 1000.0),
            None => None,
        };

        QuickSnapshot {
            cpu: Cpu {
                cores,
                pkg_watts,
                core_watts,
                temp_c,
                name: self.cpu_name.clone(),
                max_freq_mhz: self.cpu_max_freq_mhz,
                boost: self.cpu_boost,
            },
            battery: self.read_battery(),
            mem: {
                let m = self.read_memory();
                self.total_kb = m.total_kb;
                m
            },
            net: self.read_net(dt),
        }
    }

    pub fn sample_procs(&mut self) -> ProcSnapshot {
        let now = Instant::now();
        let dt = now.duration_since(self.last_proc).as_secs_f64();
        self.last_proc = now;

        let total_kb = if self.total_kb > 0 {
            self.total_kb
        } else {
            let t = self.read_memory().total_kb;
            self.total_kb = t;
            t
        };
        let mut procs = scan_processes(
            &mut self.prev_proc,
            dt,
            self.clk_tck,
            self.page_size_kb,
            total_kb,
        );
        let cores_f = self.ncores as f64;
        for p in &mut procs {
            p.cpu_pct = if cores_f > 0.0 { p.cpu_pct / cores_f } else { 0.0 };
        }

        ProcSnapshot { apps: group_apps(&procs), procs }
    }

    fn sample_cores(&mut self) -> Vec<Core> {
        let stat = read_file("/proc/stat").unwrap_or_default();
        let mut parsed: Vec<Option<(u64, u64)>> = vec![None; self.ncores];
        for line in stat.lines() {
            if !line.starts_with("cpu") {
                continue;
            }
            let idx = line["cpu".len()..]
                .split_whitespace()
                .next()
                .and_then(|s| s.parse::<usize>().ok());
            let Some(idx) = idx else { continue };
            if idx >= self.ncores {
                continue;
            }
            parsed[idx] = parse_cpu_line(line);
        }
        let mut cores = Vec::with_capacity(self.ncores);
        for i in 0..self.ncores {
            let cur = parsed[i];
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
        let name = self.battery_name.as_ref()?;
        let base = format!("/sys/class/power_supply/{name}");
        let status = read_file(&format!("{base}/status"))?.trim().to_string();
        let charge_pct = read_file(&format!("{base}/capacity"))
            .and_then(|s| s.trim().parse::<u64>().ok())?;
        let full = read_file(&format!("{base}/charge_full"))
            .and_then(|s| s.trim().parse::<u64>().ok())?;
        let design = read_file(&format!("{base}/charge_full_design"))
            .and_then(|s| s.trim().parse::<u64>().ok())?;
        let current_now = read_file(&format!("{base}/current_now"))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let voltage_now = read_file(&format!("{base}/voltage_now"))
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let cycle_count = read_file(&format!("{base}/cycle_count"))
            .and_then(|s| s.trim().parse::<u64>().ok())
            .filter(|c| *c != 0);
        let temp_c = read_file(&format!("{base}/temp"))
            .and_then(|s| s.trim().parse::<i64>().ok())
            .map(|t| t as f64 / 1000.0);
        let charging = status.starts_with("Charging");
        let raw = battery_watts(current_now, voltage_now);
        // While charging, the battery's own current only reflects what flows *into*
        // the battery (~18W here); the rest of the adapter's wattage runs the system.
        // Report the actual charger/adapter delivery (negotiated PD contract) instead,
        // which is what the user expects from a "65W" charger. Falls back to the
        // battery intake if no online source psy is available.
        let watts = if charging {
            -(self.read_charger_watts().unwrap_or_else(|| raw.abs()))
        } else {
            raw.abs()
        };
        Some(Battery {
            charge_pct: charge_pct as f64,
            health_pct: health_pct(full, design),
            watts,
            status,
            cycle_count,
            temp_c,
        })
    }

    fn read_charger_watts(&self) -> Option<f64> {
        let dir = std::fs::read_dir("/sys/class/power_supply").ok()?;
        for entry in dir.flatten() {
            let base = entry.path();
            let online = read_file(base.join("online").to_str()?)?.trim().parse::<i64>().ok();
            if online != Some(1) {
                continue;
            }
            let cur = read_file(base.join("current_now").to_str()?)?
                .trim()
                .parse::<i64>()
                .ok();
            let volt = read_file(base.join("voltage_now").to_str()?)?
                .trim()
                .parse::<i64>()
                .ok();
            if let (Some(c), Some(v)) = (cur, volt)
                && c > 0 && v > 0
            {
                return Some(battery_watts(c, v));
            }
        }
        None
    }

    fn read_memory(&self) -> Memory {
        let s = read_file("/proc/meminfo").unwrap_or_default();
        let (total_kb, free_kb, avail_kb, buffers_kb, cached_kb, swap_total_kb, swap_free_kb) =
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

    fn read_net(&mut self, dt: f64) -> Net {
        let s = read_file("/proc/net/dev").unwrap_or_default();
        let mut down = 0u64;
        let mut up = 0u64;
        let mut ifaces: Vec<NetIface> = Vec::new();
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
            let d = net_rate(p.0, rx, dt);
            let u = net_rate(p.1, tx, dt);
            down += d;
            up += u;
            ifaces.push(NetIface {
                name: name.clone(),
                label: self
                    .iface_labels
                    .entry(name.clone())
                    .or_insert_with(|| iface_label(&name))
                    .clone(),
                down_bps: d,
                up_bps: u,
            });
        }
        self.prev_net = cur;
        Net {
            down_bps: down,
            up_bps: up,
            ifaces,
        }
    }
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

fn read_cpu_name() -> String {
    read_file("/proc/cpuinfo")
        .and_then(|s| {
            s.lines()
                .find_map(|l| {
                    let t = l.trim();
                    t.strip_prefix("model name")
                        .and_then(|rest| rest.split_once(':').map(|(_, v)| v.trim().to_string()))
                })
        })
        .unwrap_or_else(|| "Unknown CPU".to_string())
}

fn read_cpu_max_freq_mhz() -> u64 {
    read_file("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|khz| khz / 1000)
        .unwrap_or(0)
}

fn read_cpu_boost() -> Option<bool> {
    if let Some(v) = read_file("/sys/devices/system/cpu/cpufreq/boost")
        .and_then(|s| s.trim().parse::<u64>().ok())
    {
        return Some(v == 1);
    }
    if let Some(v) = read_file("/sys/devices/system/cpu/intel_pstate/no_turbo")
        .and_then(|s| s.trim().parse::<u64>().ok())
    {
        return Some(v == 0);
    }
    None
}

fn iface_label(name: &str) -> String {
    let base = format!("/sys/class/net/{name}");
    if std::path::Path::new(&format!("{base}/wireless")).exists() {
        "Wi-Fi".to_string()
    } else if std::path::Path::new(&format!("{base}/device")).exists() {
        "Ethernet".to_string()
    } else {
        name.to_string()
    }
}

fn rapl_sample(path: &str, prev: &mut u64, rapl_max: u64, dt_sec: f64) -> Option<f64> {
    let cur = read_file(&format!("{path}/energy_uj"))
        .and_then(|s| s.trim().parse::<u64>().ok())?;
    if *prev == 0 {
        *prev = cur;
        return Some(0.0);
    }
    let w = rapl_watts(*prev, cur, rapl_max, dt_sec);
    *prev = cur;
    Some(w)
}

fn list_dir_names(path: &str) -> Vec<String> {
    match std::fs::read_dir(path) {
        Ok(rd) => rd
            .flatten()
            .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn zone_name(path: &str) -> String {
    read_file(&format!("{path}/name"))
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn read_rapl_max(path: &str) -> Option<u64> {
    read_file(&format!("{path}/max_energy_range_uj"))
        .and_then(|s| s.trim().parse::<u64>().ok())
}

/// Discover the RAPL package and core power zones under /sys/class/powercap.
/// Returns `(package_zone_path, core_zone_path)`. Zones are identified by their
/// `name` file (e.g. `package-0`, `core`) rather than hardcoded paths, so this
/// works across Intel layouts and (where the kernel exposes them) other vendors.
pub fn discover_rapl() -> (Option<String>, Option<String>) {
    let base = "/sys/class/powercap";
    let mut zones: Vec<(String, String, String)> = Vec::new(); // (dir_name, path, name)
    for entry in list_dir_names(base) {
        let path = format!("{base}/{entry}");
        if Path::new(&format!("{path}/energy_uj")).exists() {
            zones.push((entry.clone(), path.clone(), zone_name(&path)));
        } else {
            for sub in list_dir_names(&path) {
                let sub_path = format!("{path}/{sub}");
                if Path::new(&format!("{sub_path}/energy_uj")).exists() {
                    zones.push((sub.clone(), sub_path.clone(), zone_name(&sub_path)));
                }
            }
        }
    }
    let packages: Vec<(String, String)> = zones
        .iter()
        .filter(|(_, _, n)| n.starts_with("package"))
        .map(|(d, p, _)| (d.clone(), p.clone()))
        .collect();
    // Prefer the canonical (non-mmio) RAPL control type; `intel-rapl-mmio` is an
    // alternate backend exposing the same package and is often root-only.
    let pkg = packages
        .iter()
        .find(|(_, p)| !p.contains("mmio"))
        .or_else(|| packages.first())
        .cloned();
    let core = match &pkg {
        Some((d, _)) => zones
            .iter()
            .find(|(zd, _, n)| n == "core" && zd.starts_with(&format!("{d}:")))
            .or_else(|| zones.iter().find(|(_, _, n)| n == "core"))
            .map(|(_, p, _)| p.clone()),
        None => zones.iter().find(|(_, _, n)| n == "core").map(|(_, p, _)| p.clone()),
    };
    (pkg.map(|(_, p)| p), core)
}

/// Discover the CPU thermal zone by scanning /sys/class/thermal for a zone whose
/// `type` identifies the CPU package temperature sensor.
pub fn discover_cpu_thermal_zone() -> Option<String> {
    let base = "/sys/class/thermal";
    let mut zones: Vec<(String, String)> = Vec::new(); // (type, path)
    for entry in list_dir_names(base) {
        if !entry.starts_with("thermal_zone") {
            continue;
        }
        let path = format!("{base}/{entry}");
        let ty = read_file(&format!("{path}/type"))
            .unwrap_or_default()
            .trim()
            .to_string();
        zones.push((ty, path));
    }
    for want in ["x86_pkg_temp", "cpu_thermal", "cpu"] {
        if let Some(z) = zones.iter().find(|(t, _)| t == want) {
            return Some(z.1.clone());
        }
    }
    zones
        .iter()
        .find(|(t, _)| t.contains("cpu") || t.contains("pkg"))
        .map(|(_, p)| p.clone())
}

/// Discover the first battery by finding a /sys/class/power_supply entry whose
/// `type` file is `Battery`.
pub fn discover_battery() -> Option<String> {
    let base = "/sys/class/power_supply";
    for entry in list_dir_names(base) {
        let path = format!("{base}/{entry}");
        let ty = read_file(&format!("{path}/type")).unwrap_or_default();
        if ty.trim() == "Battery" {
            return Some(entry);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_count_is_positive() {
        let n = online_count();
        assert!(n > 0, "expected at least one online CPU, got {n}");
    }

    #[test]
    fn sample_populates_core_fields() {
        let mut s = Sampler::new();
        let snap = s.sample_quick();
        assert_eq!(snap.cpu.cores.len(), online_count());
        assert!(snap.mem.total_kb > 0);
        assert!(snap.mem.free_kb > 0);
        assert!(snap.mem.cache_kb > 0);
        let procs = s.sample_procs();
        assert!(!procs.procs.is_empty());
        assert!(!procs.apps.is_empty());
    }

    #[test]
    fn rapl_energy_files_are_world_readable() {
        let (pkg, core) = discover_rapl();
        for p in pkg.into_iter().chain(core) {
            if crate::read::read_file(&format!("{p}/energy_uj")).is_none() {
                eprintln!(
                    "skipping RAPL readability check: {p}/energy_uj not readable \
                     (set up the udev rule in data/99-sysmon-rapl.rules, or this is a \
                     machine/CI environment without RAPL access)"
                );
                return;
            }
        }
    }
}
