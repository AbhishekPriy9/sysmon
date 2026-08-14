use std::collections::HashMap;
use std::fs;
use std::io::Read;

use crate::model::{AppRow, ProcRow};
use crate::read::{parse_proc_stat, parse_statm_rss, process_cpu_percent};

pub fn scan_processes(
    prev: &mut HashMap<u32, u64>,
    dt_sec: f64,
    clk_tck: u64,
    page_size_kb: u64,
    total_ram_kb: u64,
) -> Vec<ProcRow> {
    let Ok(dir) = fs::read_dir("/proc") else {
        return Vec::new();
    };

    let mut next_prev = HashMap::with_capacity(prev.len());
    let mut out = Vec::with_capacity(prev.len());

    let mut stat_buf = String::with_capacity(512);
    let mut statm_buf = String::with_capacity(128);

    for entry in dir.flatten() {
        let file_name = entry.file_name();
        let Some(name_str) = file_name.to_str() else {
            continue;
        };
        if name_str.is_empty() || !name_str.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(pid) = name_str.parse::<u32>() else {
            continue;
        };

        // Read /proc/{pid}/stat
        let stat_path = format!("/proc/{pid}/stat");
        stat_buf.clear();
        let Ok(mut stat_file) = fs::File::open(&stat_path) else {
            continue;
        };
        if stat_file.read_to_string(&mut stat_buf).is_err() {
            continue;
        }

        let Some((name, utime, stime)) = parse_proc_stat(&stat_buf) else {
            continue;
        };
        let ticks = utime.saturating_add(stime);
        let cpu_pct = match prev.get(&pid) {
            Some(&p) => process_cpu_percent(p, ticks, dt_sec, clk_tck),
            None => 0.0,
        };

        // Read /proc/{pid}/statm
        let statm_path = format!("/proc/{pid}/statm");
        statm_buf.clear();
        let rss_kb = if let Ok(mut statm_file) = fs::File::open(&statm_path) {
            if statm_file.read_to_string(&mut statm_buf).is_ok() {
                parse_statm_rss(&statm_buf, page_size_kb).unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };

        let mem_pct = if total_ram_kb == 0 {
            0.0
        } else {
            rss_kb as f64 / total_ram_kb as f64 * 100.0
        };

        out.push(ProcRow { pid, name, cpu_pct, mem_pct, rss_kb });
        next_prev.insert(pid, ticks);
    }

    *prev = next_prev;
    out.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    out
}

pub fn group_apps(procs: &[ProcRow]) -> Vec<AppRow> {
    let mut map: HashMap<&str, (f64, f64, u64, u32, Vec<u32>)> =
        HashMap::with_capacity(procs.len().min(128));
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
        assert_eq!(apps[0].pids, vec![1, 2]);
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
