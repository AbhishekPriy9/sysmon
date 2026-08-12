use std::collections::HashMap;
use std::time::Instant;

use crate::model::{Battery, Core, Cpu, Memory, Net, Snapshot};
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

        let cores = self.sample_cores();
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

    fn sample_cores(&mut self) -> Vec<Core> {
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
        let mut watts = battery_watts(current_now, voltage_now);
        if status.starts_with("Charging") {
            watts = -watts.abs();
        }
        Some(Battery {
            charge_pct: charge_pct as f64,
            health_pct: health_pct(full, design),
            watts,
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
}
