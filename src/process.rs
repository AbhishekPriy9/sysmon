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
        out.push(ProcRow { pid: *pid, name, cpu_pct, mem_pct, rss_kb });
        prev.insert(*pid, ticks);
    }

    prev.retain(|pid, _| live.contains(pid));
    out.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub fn group_apps(procs: &[ProcRow]) -> Vec<AppRow> {
    let mut map: HashMap<&str, (f64, f64, u64, u32)> = HashMap::new();
    for p in procs {
        let e = map.entry(p.name.as_str()).or_insert((0.0, 0.0, 0, 0));
        e.0 += p.cpu_pct;
        e.1 += p.mem_pct;
        e.2 += p.rss_kb;
        e.3 += 1;
    }
    let mut rows: Vec<AppRow> = map
        .into_iter()
        .map(|(name, (cpu, mem, rss, count))| AppRow {
            name: name.to_string(),
            cpu_pct: cpu,
            mem_pct: mem,
            rss_kb: rss,
            proc_count: count,
        })
        .collect();
    rows.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_apps_aggregates_and_sorts_by_cpu() {
        let procs = vec![
            ProcRow { pid: 1, name: "chrome".into(), cpu_pct: 5.0, mem_pct: 1.0, rss_kb: 100 },
            ProcRow { pid: 2, name: "chrome".into(), cpu_pct: 7.0, mem_pct: 2.0, rss_kb: 200 },
            ProcRow { pid: 3, name: "code".into(), cpu_pct: 3.0, mem_pct: 0.5, rss_kb: 50 },
        ];
        let apps = group_apps(&procs);
        assert_eq!(apps.len(), 2);
        assert_eq!(apps[0].name, "chrome");
        assert!((apps[0].cpu_pct - 12.0).abs() < 1e-9);
        assert!((apps[0].mem_pct - 3.0).abs() < 1e-9);
        assert_eq!(apps[0].rss_kb, 300);
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
