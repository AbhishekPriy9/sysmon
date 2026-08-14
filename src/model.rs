pub struct QuickSnapshot {
    pub cpu: Cpu,
    pub battery: Option<Battery>,
    pub mem: Memory,
    pub net: Net,
}

pub struct ProcSnapshot {
    pub apps: Vec<AppRow>,
    pub procs: Vec<ProcRow>,
}

pub struct Cpu {
    pub cores: Vec<Core>,
    pub pkg_watts: Option<f64>,
    pub core_watts: Option<f64>,
    pub temp_c: Option<f64>,
    pub name: String,
    pub max_freq_mhz: u64,
    pub boost: Option<bool>,
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
    pub cycle_count: Option<u64>,
    pub temp_c: Option<f64>,
    pub design_capacity: Option<u64>,
    pub full_capacity: Option<u64>,
    pub remaining_capacity: Option<u64>,
    pub capacity_unit: Option<String>,
    pub capacity_voltage_uv: Option<u64>,
}

pub struct Memory {
    pub total_kb: u64,
    pub free_kb: u64,
    pub avail_kb: u64,
    pub cache_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
    pub zram_compressed_kb: u64,
}

pub struct Net {
    pub down_bps: u64,
    pub up_bps: u64,
    pub ifaces: Vec<NetIface>,
}

pub struct NetIface {
    pub name: String,
    pub label: String,
    pub down_bps: u64,
    pub up_bps: u64,
}

pub struct ProcRow {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub rss_kb: u64,
}

pub struct AppRow {
    pub name: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub rss_kb: u64,
    pub proc_count: u32,
    pub pids: Vec<u32>,
}
