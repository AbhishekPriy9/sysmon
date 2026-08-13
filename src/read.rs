use std::fs;

pub fn read_file(path: &str) -> Option<String> {
    fs::read_to_string(path).ok()
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meminfo_parses_values() {
        let s = "MemTotal:       15755096 kB\nMemFree:         1234567 kB\nMemAvailable:    9876543 kB\nBuffers:         45678 kB\nCached:          789012 kB\nSwapTotal:       20775844 kB\nSwapFree:        19000000 kB\nZswap:           100 kB\nZswapped:        20 kB\n";
        assert_eq!(
            parse_meminfo(s),
            (15755096, 1234567, 9876543, 45678, 789012, 20775844, 19000000, 100, 20)
        );
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
