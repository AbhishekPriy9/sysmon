use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use libadwaita as adw;

use gtk4::prelude::*;
use gtk4::{gio, glib, ProgressBar};
use libadwaita::prelude::*;

use crate::model::{AppRow, ProcRow, ProcSnapshot, QuickSnapshot};
use crate::sampler::{online_count, Sampler};

mod proc_item {
    use glib::prelude::*;
    use glib::subclass::prelude::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::ProcItem)]
    pub struct ProcItem {
        #[property(get, set)]
        name: RefCell<String>,
        #[property(get, set)]
        icon: RefCell<String>,
        #[property(get, set)]
        cpu: Cell<f64>,
        #[property(get, set)]
        mem: Cell<f64>,
        #[property(get, set)]
        rss: Cell<u64>,
        #[property(get, set)]
        pid: Cell<u32>,
        #[property(get, set)]
        count: Cell<u32>,
        #[property(get, set)]
        pids: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProcItem {
        const NAME: &'static str = "SysmonProcItem";
        type Type = super::ProcItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ProcItem {}
}

glib::wrapper! {
    pub struct ProcItem(ObjectSubclass<proc_item::ProcItem>);
}

impl ProcItem {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: &str,
        icon: &str,
        cpu: f64,
        mem: f64,
        rss: u64,
        pid: u32,
        count: u32,
        pids: &str,
    ) -> Self {
        glib::Object::builder()
            .property("name", name)
            .property("icon", icon)
            .property("cpu", cpu)
            .property("mem", mem)
            .property("rss", rss)
            .property("pid", pid)
            .property("count", count)
            .property("pids", pids)
            .build()
    }
}

const CSS: &str = r#"
.sysmon-card-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.08em; }
.sysmon-card-trail { font-variant-numeric: tabular-nums; }
.sysmon-row-value { font-weight: 600; font-variant-numeric: tabular-nums; }
.sysmon-card-body { background-color: var(--card-bg-color); border: 1px solid var(--border-color); border-radius: 9px; padding: 10px; }
.sysmon-core-chip { background-color: var(--card-bg-color); border: 1px solid var(--border-color); border-radius: 9px; padding: 8px 10px; }
.sysmon-core-name { font-size: 11px; font-weight: 700; opacity: 0.55; }
.sysmon-core-freq { font-size: 11.5px; opacity: 0.55; font-variant-numeric: tabular-nums; }
progressbar.sysmon-core-bar trough { min-height: 6px; border-radius: 3px; }
progressbar.sysmon-core-bar trough progress { min-height: 6px; border-radius: 3px; }
progressbar.sysmon-battery-bar trough { min-height: 8px; border-radius: 4px; }
progressbar.sysmon-battery-bar trough progress { min-height: 8px; border-radius: 4px; background-color: var(--success-bg-color); }
.sysmon-warning { background-color: alpha(var(--warning-bg-color), 0.15); border: 1px solid alpha(var(--warning-bg-color), 0.4); border-radius: 8px; padding: 8px 10px; }
.sysmon-warning image { color: var(--warning-color); }
.sysmon-freeze:checked { background-color: var(--accent-bg-color); color: var(--accent-fg-color); }
.sysmon-seg-bar { min-height: 10px; border-radius: 5px; background-color: alpha(var(--border-color), 0.4); }
.sysmon-seg-used { border-radius: 5px; background-color: var(--accent-bg-color); }
.sysmon-seg-cache { border-radius: 5px; background-color: var(--warning-bg-color); }
.sysmon-seg-free { border-radius: 5px; background-color: alpha(var(--border-color), 0.4); }
.sysmon-legend { font-size: 11px; }
.sysmon-legend-swatch { border-radius: 3px; min-width: 12px; min-height: 12px; }
.sysmon-proc-view { background: transparent; }
.sysmon-proc-view row { min-height: 30px; padding: 1px 6px; }
.sysmon-proc-icon { border-radius: 6px; background-color: alpha(var(--border-color), 0.6); padding: 3px; color: var(--fg-color); }
.sysmon-proc-name { font-weight: 600; }
.sysmon-proc-num { font-variant-numeric: tabular-nums; }
"#;

struct ProcTable {
    store: gio::ListStore,
    filter: gtk4::CustomFilter,
    #[allow(dead_code)]
    filter_model: gtk4::FilterListModel,
    #[allow(dead_code)]
    sort_model: gtk4::SortListModel,
    view: gtk4::ColumnView,
    sw: gtk4::ScrolledWindow,
    user_scrolled: Rc<Cell<bool>>,
}

struct Ui {
    core_load: Vec<ProgressBar>,
    core_freq: Vec<gtk4::Label>,
    cpu_avg: gtk4::Label,
    core_value: gtk4::Label,
    pkg_value: gtk4::Label,
    temp_value: gtk4::Label,
    cpu_name: gtk4::Label,
    cpu_max: gtk4::Label,
    cpu_boost: gtk4::Label,
    rapl_hint: gtk4::Box,
    bat_bar: ProgressBar,
    batt_pct: gtk4::Label,
    bat_health: gtk4::Label,
    bat_charge: gtk4::Label,
    bat_discharge: gtk4::Label,
    bat_cycle: gtk4::Label,
    bat_cycle_row: adw::ActionRow,
    bat_temp: gtk4::Label,
    bat_temp_row: adw::ActionRow,
    bat_design_cap: gtk4::Label,
    bat_full_cap: gtk4::Label,
    bat_remain_cap: gtk4::Label,
    mem_seg_bar: gtk4::Box,
    mem_seg_used: gtk4::Box,
    mem_seg_cache: gtk4::Box,
    mem_seg_free: gtk4::Box,
    mem_used: gtk4::Label,
    mem_cache: gtk4::Label,
    mem_free: gtk4::Label,
    mem_swap: gtk4::Label,
    mem_zram: gtk4::Label,
    mem_avail: gtk4::Label,
    net_down: gtk4::Label,
    net_up: gtk4::Label,
    net_ifaces_box: gtk4::Box,
    apps_table: ProcTable,
    procs_table: ProcTable,
    apps_items: RefCell<HashMap<String, ProcItem>>,
    procs_items: RefCell<HashMap<u32, ProcItem>>,
    net_iface_rows: RefCell<Vec<(String, adw::ActionRow, gtk4::Label)>>,
}

impl Ui {
    fn update_quick(&self, s: &QuickSnapshot) {
        for (i, c) in s.cpu.cores.iter().enumerate() {
            if let Some(b) = self.core_load.get(i) {
                b.set_fraction((c.load / 100.0).clamp(0.0, 1.0));
            }
            if let Some(l) = self.core_freq.get(i) {
                l.set_text(&format!("{:.0}% · {} MHz", c.load, c.freq_mhz));
            }
        }
        let n = s.cpu.cores.len();
        let avg = if n == 0 {
            0.0
        } else {
            s.cpu.cores.iter().map(|c| c.load).sum::<f64>() / n as f64
        };
        self.cpu_avg.set_text(&format!("{avg:.0}% avg"));

        let cw = s
            .cpu
            .core_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let pw = s
            .cpu
            .pkg_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let t = s
            .cpu
            .temp_c
            .map(|t| format!("{t:.0} °C"))
            .unwrap_or_else(|| "—".into());
        self.core_value.set_text(&cw);
        self.pkg_value.set_text(&pw);
        self.temp_value.set_text(&t);
        self.cpu_name.set_text(&s.cpu.name);
        let max_freq_txt = if s.cpu.max_freq_mhz > 0 {
            format!("{} MHz", s.cpu.max_freq_mhz)
        } else {
            "—".to_string()
        };
        self.cpu_max.set_text(&max_freq_txt);
        self.cpu_boost.set_text(match s.cpu.boost {
            Some(true) => "Enabled",
            Some(false) => "Disabled",
            None => "—",
        });
        self.rapl_hint.set_visible(s.cpu.pkg_watts.is_none());

        if let Some(b) = &s.battery {
            self.bat_bar.set_fraction((b.charge_pct / 100.0).clamp(0.0, 1.0));
            self.batt_pct.set_text(&format!("{:.0}%", b.charge_pct));
            self.bat_health.set_text(&format!("{:.0}%", b.health_pct));
            let charging = b.status.starts_with("Charging");
            let discharging = b.status.starts_with("Discharging");
            let charge_w = if charging { b.watts.abs() } else { 0.0 };
            let discharge_w = if discharging { b.watts.abs() } else { 0.0 };
            self.bat_charge.set_text(&format!("{charge_w:.1} W"));
            self.bat_discharge
                .set_text(&format!("{discharge_w:.1} W"));
            self.bat_cycle
                .set_text(&b.cycle_count.map(|c| c.to_string()).unwrap_or_else(|| "—".into()));
            self.bat_temp.set_text(
                &b.temp_c
                    .map(|t| format!("{t:.0} °C"))
                    .unwrap_or_else(|| "—".into()),
            );
            self.bat_cycle_row.set_visible(b.cycle_count.is_some());
            self.bat_temp_row.set_visible(b.temp_c.is_some());
            let unit = b.capacity_unit.as_deref().unwrap_or("mAh");
            self.bat_design_cap.set_text(
                &b.design_capacity
                    .map(|v| human_capacity_dual(v, unit, b.capacity_voltage_uv))
                    .unwrap_or_else(|| "—".into()),
            );
            self.bat_full_cap.set_text(
                &b.full_capacity
                    .map(|v| human_capacity_dual(v, unit, b.capacity_voltage_uv))
                    .unwrap_or_else(|| "—".into()),
            );
            self.bat_remain_cap.set_text(
                &b.remaining_capacity
                    .map(|v| human_capacity_dual(v, unit, b.capacity_voltage_uv))
                    .unwrap_or_else(|| "—".into()),
            );
        } else {
            self.bat_bar.set_fraction(0.0);
            self.batt_pct.set_text("—");
            self.bat_health.set_text("No battery");
            self.bat_charge.set_text("—");
            self.bat_discharge.set_text("—");
            self.bat_cycle.set_text("—");
            self.bat_temp.set_text("—");
            self.bat_cycle_row.set_visible(false);
            self.bat_temp_row.set_visible(false);
            self.bat_design_cap.set_text("—");
            self.bat_full_cap.set_text("—");
            self.bat_remain_cap.set_text("—");
        }

        let t = s.mem.total_kb;
        let used = t.saturating_sub(s.mem.avail_kb);
        let cache = s.mem.cache_kb;
        let free = s.mem.free_kb;
        let swap_used = s.mem.swap_total_kb.saturating_sub(s.mem.swap_free_kb);

        let pct = |v: u64| {
            if t > 0 {
                format!(" ({:.0}%)", 100.0 * v as f64 / t as f64)
            } else {
                String::new()
            }
        };
        self.mem_used.set_text(&format!("{}{}", human_kb(used), pct(used)));
        self.mem_avail.set_text(&format!(
            "{}{}",
            human_kb(s.mem.avail_kb),
            pct(s.mem.avail_kb)
        ));
        self.mem_cache.set_text(&format!("{}{}", human_kb(cache), pct(cache)));
        self.mem_free.set_text(&format!("{}{}", human_kb(free), pct(free)));
        self.mem_swap.set_text(&format!(
            "{} / {}",
            human_kb(swap_used),
            human_kb(s.mem.swap_total_kb)
        ));
        self.mem_zram.set_text(&human_kb(s.mem.zram_compressed_kb));

        let w = self.mem_seg_bar.width();
        if t > 0 && w > 0 {
            let avail_w = (w - 4).max(1);
            let used_w = (avail_w as f64 * (used as f64 / t as f64)).round() as i32;
            let cache_w = (avail_w as f64 * (cache as f64 / t as f64)).round() as i32;
            let cache_w = cache_w.min((avail_w - used_w).max(0));
            let free_w = (avail_w - used_w - cache_w).max(0);
            self.mem_seg_used.set_size_request(used_w, -1);
            self.mem_seg_cache.set_size_request(cache_w, -1);
            self.mem_seg_free.set_size_request(free_w, -1);
        }

        self.net_down.set_text(&human_bps(s.net.down_bps));
        self.net_up.set_text(&human_bps(s.net.up_bps));

        self.update_net_ifaces(&s.net.ifaces);
    }

    fn refill_tables(&self, apps: &[AppRow], procs: &[ProcRow]) {
        self.update_apps(apps);
        self.update_procs(procs);

        if let Some(sorter) = self.apps_table.view.sorter() {
            sorter.changed(gtk4::SorterChange::Different);
        }
        if let Some(sorter) = self.procs_table.view.sorter() {
            sorter.changed(gtk4::SorterChange::Different);
        }

        // Defer the scroll anchor to the next main-loop iteration so it runs
        // *after* GTK has applied the re-sort layout — otherwise the virtualized
        // list snaps back to its anchor before our scroll_to takes effect.
        if !self.apps_table.user_scrolled.get() {
            let view = self.apps_table.view.clone();
            let adj = self.apps_table.sw.vadjustment();
            glib::idle_add_local_once(move || {
                adj.set_value(0.0);
                view.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
            });
        }

        if !self.procs_table.user_scrolled.get() {
            let view = self.procs_table.view.clone();
            let adj = self.procs_table.sw.vadjustment();
            glib::idle_add_local_once(move || {
                adj.set_value(0.0);
                view.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
            });
        }
    }

    fn update_apps(&self, apps: &[AppRow]) {
        let mut items = self.apps_items.borrow_mut();
        let store = &self.apps_table.store;
        let mut seen = HashSet::with_capacity(apps.len());

        for a in apps {
            seen.insert(&a.name);
            match items.get(&a.name) {
                Some(it) => {
                    if (it.cpu() - a.cpu_pct).abs() > f64::EPSILON {
                        it.set_cpu(a.cpu_pct);
                    }
                    if it.rss() != a.rss_kb {
                        it.set_rss(a.rss_kb);
                    }
                    if it.count() != a.proc_count {
                        it.set_count(a.proc_count);
                    }
                    if (it.mem() - a.mem_pct).abs() > f64::EPSILON {
                        it.set_mem(a.mem_pct);
                    }
                }
                None => {
                    let it = ProcItem::new(
                        &a.name,
                        &icon_name_for(&a.name),
                        a.cpu_pct,
                        a.mem_pct,
                        a.rss_kb,
                        0,
                        a.proc_count,
                        "",
                    );
                    store.append(&it);
                    items.insert(a.name.clone(), it);
                }
            }
        }

        let gone: Vec<String> = items
            .keys()
            .filter(|k| !seen.contains(*k))
            .cloned()
            .collect();
        for name in gone {
            if let Some(it) = items.remove(&name)
                && let Some(pos) = store.find(&it)
            {
                store.remove(pos);
            }
        }
    }

    fn update_procs(&self, procs: &[ProcRow]) {
        let mut items = self.procs_items.borrow_mut();
        let store = &self.procs_table.store;
        let mut seen = HashSet::with_capacity(procs.len());

        for p in procs {
            seen.insert(p.pid);
            match items.get(&p.pid) {
                Some(it) => {
                    if it.name() != p.name {
                        it.set_name(p.name.clone());
                        it.set_icon(icon_name_for(&p.name));
                    }
                    if (it.cpu() - p.cpu_pct).abs() > f64::EPSILON {
                        it.set_cpu(p.cpu_pct);
                    }
                    if it.rss() != p.rss_kb {
                        it.set_rss(p.rss_kb);
                    }
                    if (it.mem() - p.mem_pct).abs() > f64::EPSILON {
                        it.set_mem(p.mem_pct);
                    }
                }
                None => {
                    let it = ProcItem::new(
                        &p.name,
                        &icon_name_for(&p.name),
                        p.cpu_pct,
                        p.mem_pct,
                        p.rss_kb,
                        p.pid,
                        1,
                        "",
                    );
                    store.append(&it);
                    items.insert(p.pid, it);
                }
            }
        }

        let gone: Vec<u32> = items
            .keys()
            .filter(|k| !seen.contains(*k))
            .copied()
            .collect();
        for pid in gone {
            if let Some(it) = items.remove(&pid)
                && let Some(pos) = store.find(&it)
            {
                store.remove(pos);
            }
        }
    }

    fn update_net_ifaces(&self, ifaces: &[crate::model::NetIface]) {
        let mut rows = self.net_iface_rows.borrow_mut();
        let mut seen = HashSet::with_capacity(ifaces.len());
        for i in ifaces {
            seen.insert(i.name.clone());
            let label = match rows.iter().find(|(n, _, _)| *n == i.name) {
                Some((_, _, label)) => label.clone(),
                None => {
                    let r = adw::ActionRow::new();
                    r.set_title(&i.label);
                    r.set_subtitle(&i.name);
                    let v = gtk4::Label::new(None);
                    v.add_css_class("sysmon-row-value");
                    v.set_xalign(1.0);
                    r.add_suffix(&v);
                    self.net_ifaces_box.append(&r);
                    rows.push((i.name.clone(), r.clone(), v.clone()));
                    v
                }
            };
            label.set_text(&format!(
                "↓ {}  ↑ {}",
                human_bps(i.down_bps),
                human_bps(i.up_bps)
            ));
        }
        rows.retain(|(name, row, _)| {
            if seen.contains(name) {
                true
            } else {
                self.net_ifaces_box.remove(row);
                false
            }
        });
    }
}

fn setup_css() {
    let provider = gtk4::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn card(icon: &str, title: &str) -> (gtk4::Box, gtk4::Box, gtk4::Box) {
    let outer = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    let image = gtk4::Image::from_icon_name(icon);
    image.set_pixel_size(16);
    image.add_css_class("dim-label");
    header.append(&image);
    let label = gtk4::Label::new(Some(title));
    label.add_css_class("dim-label");
    label.add_css_class("sysmon-card-title");
    header.append(&label);
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let body = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
    outer.append(&header);
    outer.append(&body);
    (outer, body, header)
}

fn row(title: &str, tooltip: &str) -> (adw::ActionRow, gtk4::Label) {
    let r = adw::ActionRow::new();
    r.set_title(title);
    r.set_tooltip_text(Some(tooltip));
    let value = gtk4::Label::new(Some("—"));
    value.add_css_class("sysmon-row-value");
    value.set_xalign(1.0);
    r.add_suffix(&value);
    (r, value)
}

fn boxed_list() -> gtk4::ListBox {
    let list = gtk4::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk4::SelectionMode::None);
    list
}

fn legend_item(css_class: &str, text: &str) -> gtk4::Box {
    let item = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let swatch = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    swatch.add_css_class("sysmon-legend-swatch");
    swatch.add_css_class(css_class);
    item.append(&swatch);
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.add_css_class("sysmon-legend");
    item.append(&label);
    item
}

fn trail_label() -> gtk4::Label {
    let l = gtk4::Label::new(Some("—"));
    l.add_css_class("dim-label");
    l.add_css_class("sysmon-card-trail");
    l.set_xalign(1.0);
    l
}

fn page(child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let b = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    b.set_margin_top(16);
    b.set_margin_bottom(16);
    b.set_margin_start(16);
    b.set_margin_end(16);
    b.append(child);
    b
}

fn human_bps(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.2} MB/s", bps as f64 / 1e6)
    } else if bps >= 1_000 {
        format!("{:.1} KB/s", bps as f64 / 1e3)
    } else {
        format!("{bps} B/s")
    }
}

fn human_kb(kb: u64) -> String {
    if kb < 1_000_000 {
        format!("{:.0} MB", kb as f64 / 1024.0)
    } else {
        format!("{:.1} GB", kb as f64 / 1e6)
    }
}

fn human_capacity_dual(value: u64, unit: &str, voltage_uv: Option<u64>) -> String {
    let uv = voltage_uv.filter(|v| *v > 0);
    let (mah, wh) = match unit {
        "mAh" => {
            let mah = value as f64 / 1_000.0;
            let wh = match uv {
                Some(uv) => mah / 1_000.0 * (uv as f64 / 1_000_000.0),
                None => return format!("{mah:.0} mAh"),
            };
            (mah, wh)
        }
        "Wh" => {
            let wh = value as f64 / 1_000_000.0;
            let mah = match uv {
                Some(uv) => value as f64 * 1_000.0 / uv as f64,
                None => return format!("{wh:.2} Wh"),
            };
            (mah, wh)
        }
        _ => return format!("{value}"),
    };
    format!("{mah:.0} mAh ({wh:.2} Wh)")
}

fn proc_icon_candidates(name: &str) -> Vec<String> {
    let lower = name.to_ascii_lowercase();
    let aliases: &[(&str, &str)] = &[
        ("code", "code"),
        ("chrome", "google-chrome"),
        ("chromium", "chromium"),
        ("firefox", "firefox"),
        ("gnome-shell", "gnome-shell"),
        ("gnome-terminal", "utilities-terminal"),
        ("nautilus", "org.gnome.Nautilus"),
        ("thunderbird", "thunderbird"),
        ("spotify", "spotify"),
        ("discord", "discord"),
        ("telegram", "telegram"),
    ];
    for &(k, v) in aliases {
        if lower == *k || lower.starts_with(k) {
            return vec![(*v).to_string(), "application-x-executable".to_string()];
        }
    }
    let mut candidates: Vec<String> = Vec::new();
    candidates.push(lower.clone());
    let stripped: String = lower.trim_end_matches(|c: char| c.is_ascii_digit()).to_string();
    if stripped != lower {
        candidates.push(stripped);
    }
    candidates.push("application-x-executable".to_string());
    candidates
}

fn icon_name_for(name: &str) -> String {
    let candidates = proc_icon_candidates(name);
    let Some(display) = gtk4::gdk::Display::default() else {
        return "application-x-executable".to_string();
    };
    let theme = gtk4::IconTheme::for_display(&display);
    for c in &candidates {
        if theme.has_icon(c) {
            return c.clone();
        }
    }
    "application-x-executable".to_string()
}

fn proc_name_factory() -> gtk4::SignalListItemFactory {
    let f = gtk4::SignalListItemFactory::new();
    f.connect_setup(|_, item| {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        row.set_hexpand(true);
        let img = gtk4::Image::new();
        img.set_pixel_size(22);
        img.set_valign(gtk4::Align::Center);
        img.add_css_class("sysmon-proc-icon");
        let label = gtk4::Label::new(None);
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.add_css_class("sysmon-proc-name");
        row.append(&img);
        row.append(&label);
        item.downcast_ref::<gtk4::ListItem>().unwrap().set_child(Some(&row));
    });
    f.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let row = li.child().unwrap().downcast::<gtk4::Box>().unwrap();
        let pi = li.item().unwrap().downcast::<ProcItem>().unwrap();
        let img = row.first_child().unwrap().downcast::<gtk4::Image>().unwrap();
        let label = row.last_child().unwrap().downcast::<gtk4::Label>().unwrap();
        img.set_icon_name(Some(pi.icon().as_str()));
        label.set_text(&pi.name());

        let label_weak = label.downgrade();
        let img_weak = img.downgrade();
        let h_name = pi.connect_notify_local(Some("name"), move |obj, _| {
            if let Some(lbl) = label_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                lbl.set_text(&pi.name());
            }
        });
        let h_icon = pi.connect_notify_local(Some("icon"), move |obj, _| {
            if let Some(im) = img_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                im.set_icon_name(Some(pi.icon().as_str()));
            }
        });
        unsafe {
            li.set_data("name_handler", h_name);
            li.set_data("icon_handler", h_icon);
        }
    });
    f.connect_unbind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        if let Some(pi) = li.item().and_then(|it| it.downcast::<ProcItem>().ok()) {
            if let Some(h) = unsafe { li.steal_data::<glib::SignalHandlerId>("name_handler") } {
                pi.disconnect(h);
            }
            if let Some(h) = unsafe { li.steal_data::<glib::SignalHandlerId>("icon_handler") } {
                pi.disconnect(h);
            }
        }
    });
    f
}

fn proc_cpu_factory() -> gtk4::SignalListItemFactory {
    let f = gtk4::SignalListItemFactory::new();
    f.connect_setup(|_, item| {
        let label = gtk4::Label::new(None);
        label.set_xalign(1.0);
        label.add_css_class("sysmon-proc-num");
        label.set_width_chars(7);
        item.downcast_ref::<gtk4::ListItem>().unwrap().set_child(Some(&label));
    });
    f.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = li.child().unwrap().downcast::<gtk4::Label>().unwrap();
        let pi = li.item().unwrap().downcast::<ProcItem>().unwrap();
        label.set_text(&format!("{:.1}%", pi.cpu()));

        let label_weak = label.downgrade();
        let handler = pi.connect_notify_local(Some("cpu"), move |obj, _| {
            if let Some(lbl) = label_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                lbl.set_text(&format!("{:.1}%", pi.cpu()));
            }
        });
        unsafe {
            li.set_data("cpu_handler", handler);
        }
    });
    f.connect_unbind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        if let Some(pi) = li.item().and_then(|it| it.downcast::<ProcItem>().ok())
            && let Some(handler) = unsafe { li.steal_data::<glib::SignalHandlerId>("cpu_handler") }
        {
            pi.disconnect(handler);
        }
    });
    f
}

fn proc_mem_factory() -> gtk4::SignalListItemFactory {
    let f = gtk4::SignalListItemFactory::new();
    f.connect_setup(|_, item| {
        let label = gtk4::Label::new(None);
        label.set_xalign(1.0);
        label.add_css_class("sysmon-proc-num");
        label.set_width_chars(9);
        item.downcast_ref::<gtk4::ListItem>().unwrap().set_child(Some(&label));
    });
    f.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = li.child().unwrap().downcast::<gtk4::Label>().unwrap();
        let pi = li.item().unwrap().downcast::<ProcItem>().unwrap();
        label.set_text(&human_kb(pi.rss()));

        let label_weak = label.downgrade();
        let handler = pi.connect_notify_local(Some("rss"), move |obj, _| {
            if let Some(lbl) = label_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                lbl.set_text(&human_kb(pi.rss()));
            }
        });
        unsafe {
            li.set_data("mem_handler", handler);
        }
    });
    f.connect_unbind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        if let Some(pi) = li.item().and_then(|it| it.downcast::<ProcItem>().ok())
            && let Some(handler) = unsafe { li.steal_data::<glib::SignalHandlerId>("mem_handler") }
        {
            pi.disconnect(handler);
        }
    });
    f
}

fn proc_pid_factory() -> gtk4::SignalListItemFactory {
    let f = gtk4::SignalListItemFactory::new();
    f.connect_setup(|_, item| {
        let label = gtk4::Label::new(None);
        label.set_xalign(1.0);
        label.add_css_class("sysmon-proc-num");
        item.downcast_ref::<gtk4::ListItem>().unwrap().set_child(Some(&label));
    });
    f.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = li.child().unwrap().downcast::<gtk4::Label>().unwrap();
        let pi = li.item().unwrap().downcast::<ProcItem>().unwrap();
        label.set_text(&pi.pid().to_string());

        let label_weak = label.downgrade();
        let handler = pi.connect_notify_local(Some("pid"), move |obj, _| {
            if let Some(lbl) = label_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                lbl.set_text(&pi.pid().to_string());
            }
        });
        unsafe {
            li.set_data("pid_handler", handler);
        }
    });
    f.connect_unbind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        if let Some(pi) = li.item().and_then(|it| it.downcast::<ProcItem>().ok())
            && let Some(handler) = unsafe { li.steal_data::<glib::SignalHandlerId>("pid_handler") }
        {
            pi.disconnect(handler);
        }
    });
    f
}

fn proc_count_factory() -> gtk4::SignalListItemFactory {
    let f = gtk4::SignalListItemFactory::new();
    f.connect_setup(|_, item| {
        let label = gtk4::Label::new(None);
        label.set_xalign(1.0);
        label.add_css_class("sysmon-proc-num");
        item.downcast_ref::<gtk4::ListItem>().unwrap().set_child(Some(&label));
    });
    f.connect_bind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let label = li.child().unwrap().downcast::<gtk4::Label>().unwrap();
        let pi = li.item().unwrap().downcast::<ProcItem>().unwrap();
        label.set_text(&pi.count().to_string());

        let label_weak = label.downgrade();
        let handler = pi.connect_notify_local(Some("count"), move |obj, _| {
            if let Some(lbl) = label_weak.upgrade() {
                let pi = obj.downcast_ref::<ProcItem>().unwrap();
                lbl.set_text(&pi.count().to_string());
            }
        });
        unsafe {
            li.set_data("count_handler", handler);
        }
    });
    f.connect_unbind(|_, item| {
        let li = item.downcast_ref::<gtk4::ListItem>().unwrap();
        if let Some(pi) = li.item().and_then(|it| it.downcast::<ProcItem>().ok())
            && let Some(handler) = unsafe { li.steal_data::<glib::SignalHandlerId>("count_handler") }
        {
            pi.disconnect(handler);
        }
    });
    f
}

fn build_apps_table(search_query: Rc<RefCell<String>>) -> ProcTable {
    let store = gio::ListStore::new::<ProcItem>();
    let user_scrolled = Rc::new(Cell::new(false));

    let sq = Rc::clone(&search_query);
    let filter = gtk4::CustomFilter::new(move |obj| {
        let q = sq.borrow();
        if q.is_empty() {
            return true;
        }
        let Some(it) = obj.downcast_ref::<ProcItem>() else {
            return true;
        };
        let q_lower = q.to_lowercase();
        it.name().to_lowercase().contains(&q_lower)
    });
    let filter_model = gtk4::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    filter_model.set_incremental(false);

    let sort_model = gtk4::SortListModel::new(Some(filter_model.clone()), None::<gtk4::Sorter>);
    sort_model.set_incremental(false);

    let selection = gtk4::NoSelection::new(Some(sort_model.clone()));
    let view = gtk4::ColumnView::new(Some(selection));
    view.add_css_class("sysmon-proc-view");
    view.add_css_class("data-table");
    view.set_show_column_separators(false);
    view.set_margin_end(8);

    let sw = gtk4::ScrolledWindow::new();
    sw.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    sw.set_vexpand(true);
    sw.set_min_content_height(160);

    let scroll_ctrl = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::KINETIC,
    );
    let us_clone = Rc::clone(&user_scrolled);
    let sw_weak = sw.downgrade();
    scroll_ctrl.connect_scroll(move |_, _, dy| {
        if let Some(sw) = sw_weak.upgrade() {
            let val = sw.vadjustment().value();
            if dy < 0.0 && val <= 5.0 {
                us_clone.set(false);
            } else if dy > 0.0 {
                us_clone.set(true);
            }
        }
        glib::Propagation::Proceed
    });
    sw.add_controller(scroll_ctrl);

    let us_clone2 = Rc::clone(&user_scrolled);
    sw.vadjustment().connect_value_changed(move |adj| {
        if adj.value() <= 2.0 {
            us_clone2.set(false);
        }
    });

    let name_col = gtk4::ColumnViewColumn::new(Some("App"), Some(proc_name_factory()));
    name_col.set_resizable(true);
    name_col.set_expand(true);
    let name_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        a.name().to_lowercase().cmp(&b.name().to_lowercase()).into()
    });
    name_col.set_sorter(Some(&name_sorter));
    view.append_column(&name_col);

    let cpu_col = gtk4::ColumnViewColumn::new(Some("% of total CPU"), Some(proc_cpu_factory()));
    cpu_col.set_resizable(true);
    cpu_col.set_fixed_width(120);
    let cpu_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.cpu().partial_cmp(&b.cpu()) {
            Some(std::cmp::Ordering::Equal) | None => {
                a.name().to_lowercase().cmp(&b.name().to_lowercase()).into()
            }
            Some(ord) => ord.into(),
        }
    });
    cpu_col.set_sorter(Some(&cpu_sorter));
    view.append_column(&cpu_col);

    let mem_col = gtk4::ColumnViewColumn::new(Some("MEM"), Some(proc_mem_factory()));
    mem_col.set_resizable(true);
    mem_col.set_fixed_width(100);
    let mem_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.rss().cmp(&b.rss()) {
            std::cmp::Ordering::Equal => {
                a.name().to_lowercase().cmp(&b.name().to_lowercase()).into()
            }
            ord => ord.into(),
        }
    });
    mem_col.set_sorter(Some(&mem_sorter));
    view.append_column(&mem_col);

    let procs_col = gtk4::ColumnViewColumn::new(Some("Procs"), Some(proc_count_factory()));
    procs_col.set_resizable(true);
    procs_col.set_fixed_width(70);
    let procs_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.count().cmp(&b.count()) {
            std::cmp::Ordering::Equal => {
                a.name().to_lowercase().cmp(&b.name().to_lowercase()).into()
            }
            ord => ord.into(),
        }
    });
    procs_col.set_sorter(Some(&procs_sorter));
    view.append_column(&procs_col);

    view.sort_by_column(Some(&cpu_col), gtk4::SortType::Descending);
    sort_model.set_sorter(view.sorter().as_ref());

    if let Some(cv_sorter) = view.sorter() {
        let view_weak = view.downgrade();
        let sw_weak = sw.downgrade();
        let us_clone = Rc::clone(&user_scrolled);
        cv_sorter.connect_notify_local(Some("primary-sort-column"), move |_, _| {
            // Set immediately so refill_tables respects it on next tick.
            us_clone.set(false);
            // Defer the actual scroll until GTK has finished applying the re-sort.
            let vw = view_weak.clone();
            let sw2 = sw_weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(sw) = sw2.upgrade() {
                    sw.vadjustment().set_value(0.0);
                }
                if let Some(v) = vw.upgrade() {
                    v.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
                }
            });
        });
        let view_weak2 = view.downgrade();
        let sw_weak2 = sw.downgrade();
        let us_clone2 = Rc::clone(&user_scrolled);
        cv_sorter.connect_notify_local(Some("primary-sort-order"), move |_, _| {
            us_clone2.set(false);
            let vw = view_weak2.clone();
            let sw2 = sw_weak2.clone();
            glib::idle_add_local_once(move || {
                if let Some(sw) = sw2.upgrade() {
                    sw.vadjustment().set_value(0.0);
                }
                if let Some(v) = vw.upgrade() {
                    v.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
                }
            });
        });
    }

    sw.set_child(Some(&view));

    ProcTable {
        store,
        filter,
        filter_model,
        sort_model,
        view,
        sw,
        user_scrolled,
    }
}

fn build_procs_table(search_query: Rc<RefCell<String>>) -> ProcTable {
    let store = gio::ListStore::new::<ProcItem>();
    let user_scrolled = Rc::new(Cell::new(false));

    let sq = Rc::clone(&search_query);
    let filter = gtk4::CustomFilter::new(move |obj| {
        let q = sq.borrow();
        if q.is_empty() {
            return true;
        }
        let Some(it) = obj.downcast_ref::<ProcItem>() else {
            return true;
        };
        let q_lower = q.to_lowercase();
        it.name().to_lowercase().contains(&q_lower) || it.pid().to_string().contains(&q_lower)
    });
    let filter_model = gtk4::FilterListModel::new(Some(store.clone()), Some(filter.clone()));
    filter_model.set_incremental(false);

    let sort_model = gtk4::SortListModel::new(Some(filter_model.clone()), None::<gtk4::Sorter>);
    sort_model.set_incremental(false);

    let selection = gtk4::NoSelection::new(Some(sort_model.clone()));
    let view = gtk4::ColumnView::new(Some(selection));
    view.add_css_class("sysmon-proc-view");
    view.add_css_class("data-table");
    view.set_show_column_separators(false);
    view.set_margin_end(8);

    let sw = gtk4::ScrolledWindow::new();
    sw.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    sw.set_vexpand(true);
    sw.set_min_content_height(160);

    let scroll_ctrl = gtk4::EventControllerScroll::new(
        gtk4::EventControllerScrollFlags::VERTICAL | gtk4::EventControllerScrollFlags::KINETIC,
    );
    let us_clone = Rc::clone(&user_scrolled);
    let sw_weak = sw.downgrade();
    scroll_ctrl.connect_scroll(move |_, _, dy| {
        if let Some(sw) = sw_weak.upgrade() {
            let val = sw.vadjustment().value();
            if dy < 0.0 && val <= 5.0 {
                us_clone.set(false);
            } else if dy > 0.0 {
                us_clone.set(true);
            }
        }
        glib::Propagation::Proceed
    });
    sw.add_controller(scroll_ctrl);

    let us_clone2 = Rc::clone(&user_scrolled);
    sw.vadjustment().connect_value_changed(move |adj| {
        if adj.value() <= 2.0 {
            us_clone2.set(false);
        }
    });

    let name_col = gtk4::ColumnViewColumn::new(Some("Name"), Some(proc_name_factory()));
    name_col.set_resizable(true);
    name_col.set_expand(true);
    let name_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.name().to_lowercase().cmp(&b.name().to_lowercase()) {
            std::cmp::Ordering::Equal => a.pid().cmp(&b.pid()).into(),
            ord => ord.into(),
        }
    });
    name_col.set_sorter(Some(&name_sorter));
    view.append_column(&name_col);

    let pid_col = gtk4::ColumnViewColumn::new(Some("PID"), Some(proc_pid_factory()));
    pid_col.set_resizable(true);
    pid_col.set_fixed_width(80);
    let pid_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        a.pid().cmp(&b.pid()).into()
    });
    pid_col.set_sorter(Some(&pid_sorter));
    view.append_column(&pid_col);

    let cpu_col = gtk4::ColumnViewColumn::new(Some("% of total CPU"), Some(proc_cpu_factory()));
    cpu_col.set_resizable(true);
    cpu_col.set_fixed_width(120);
    let cpu_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.cpu().partial_cmp(&b.cpu()) {
            Some(std::cmp::Ordering::Equal) | None => {
                match a.name().to_lowercase().cmp(&b.name().to_lowercase()) {
                    std::cmp::Ordering::Equal => a.pid().cmp(&b.pid()).into(),
                    ord => ord.into(),
                }
            }
            Some(ord) => ord.into(),
        }
    });
    cpu_col.set_sorter(Some(&cpu_sorter));
    view.append_column(&cpu_col);

    let mem_col = gtk4::ColumnViewColumn::new(Some("MEM"), Some(proc_mem_factory()));
    mem_col.set_resizable(true);
    mem_col.set_fixed_width(100);
    let mem_sorter = gtk4::CustomSorter::new(|a, b| {
        let a = a.downcast_ref::<ProcItem>().unwrap();
        let b = b.downcast_ref::<ProcItem>().unwrap();
        match a.rss().cmp(&b.rss()) {
            std::cmp::Ordering::Equal => {
                match a.name().to_lowercase().cmp(&b.name().to_lowercase()) {
                    std::cmp::Ordering::Equal => a.pid().cmp(&b.pid()).into(),
                    ord => ord.into(),
                }
            }
            ord => ord.into(),
        }
    });
    mem_col.set_sorter(Some(&mem_sorter));
    view.append_column(&mem_col);

    view.sort_by_column(Some(&cpu_col), gtk4::SortType::Descending);
    sort_model.set_sorter(view.sorter().as_ref());

    if let Some(cv_sorter) = view.sorter() {
        let view_weak = view.downgrade();
        let sw_weak = sw.downgrade();
        let us_clone = Rc::clone(&user_scrolled);
        cv_sorter.connect_notify_local(Some("primary-sort-column"), move |_, _| {
            us_clone.set(false);
            let vw = view_weak.clone();
            let sw2 = sw_weak.clone();
            glib::idle_add_local_once(move || {
                if let Some(sw) = sw2.upgrade() {
                    sw.vadjustment().set_value(0.0);
                }
                if let Some(v) = vw.upgrade() {
                    v.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
                }
            });
        });
        let view_weak2 = view.downgrade();
        let sw_weak2 = sw.downgrade();
        let us_clone2 = Rc::clone(&user_scrolled);
        cv_sorter.connect_notify_local(Some("primary-sort-order"), move |_, _| {
            us_clone2.set(false);
            let vw = view_weak2.clone();
            let sw2 = sw_weak2.clone();
            glib::idle_add_local_once(move || {
                if let Some(sw) = sw2.upgrade() {
                    sw.vadjustment().set_value(0.0);
                }
                if let Some(v) = vw.upgrade() {
                    v.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
                }
            });
        });
    }

    sw.set_child(Some(&view));

    ProcTable {
        store,
        filter,
        filter_model,
        sort_model,
        view,
        sw,
        user_scrolled,
    }
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    setup_css();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Sysmon")
        .default_width(864)
        .default_height(640)
        .build();
    window.set_icon_name(Some("dev.sysmon.Sysmon"));

    let toast_overlay = adw::ToastOverlay::new();

    let header = adw::HeaderBar::new();
    let frozen = Rc::new(std::cell::Cell::new(false));
    let freeze_btn = gtk4::ToggleButton::new();
    freeze_btn.add_css_class("sysmon-freeze");
    freeze_btn.set_icon_name("media-playback-pause-symbolic");
    freeze_btn.set_tooltip_text(Some("Freeze the live refresh"));
    let frozen_btn = Rc::clone(&frozen);
    freeze_btn.connect_toggled(move |b| {
        let active = b.is_active();
        frozen_btn.set(active);
        b.set_icon_name(if active {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });
        b.set_tooltip_text(Some(if active {
            "Resume the live refresh"
        } else {
            "Freeze the live refresh"
        }));
    });
    header.pack_start(&freeze_btn);

    let about_btn = gtk4::Button::from_icon_name("help-about-symbolic");
    about_btn.set_tooltip_text(Some("About Sysmon"));
    let app_about = app.clone();
    about_btn.connect_clicked(move |_| {
        let d = adw::AboutDialog::new();
        d.set_application_icon("dev.sysmon.Sysmon");
        d.set_application_name("Sysmon");
        d.set_version(env!("CARGO_PKG_VERSION"));
        d.set_comments(
            "Live system monitor for CPU, battery, memory, network, and processes.",
        );
        d.set_license_type(gtk4::License::Custom);
        d.set_license(
            "Source-available. Free to read/run/share verbatim; no modification and \
             redistribution, no re-labeling/re-branding, no commercial sale. See LICENSE.",
        );
        d.set_website("https://github.com/AbhishekPriy9/sysmon");
        d.present(app_about.active_window().as_ref());
    });
    header.pack_end(&about_btn);

    let (cpu_card, cbody, cheader) = card("view-grid-symbolic", "CPU");
    let cpu_avg = trail_label();
    cheader.append(&cpu_avg);

    let chip_wrap = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    chip_wrap.add_css_class("sysmon-card-body");
    cbody.append(&chip_wrap);
    let flow = gtk4::FlowBox::new();
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(4);
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_activate_on_single_click(false);
    flow.set_hexpand(true);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..online_count() {
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        v.set_size_request(170, -1);
        v.add_css_class("sysmon-core-chip");
        let name = gtk4::Label::new(Some(&format!("CPU {i}")));
        name.set_xalign(0.0);
        name.add_css_class("sysmon-core-name");
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        bar.set_show_text(false);
        bar.set_hexpand(true);
        bar.add_css_class("sysmon-core-bar");
        let freq = gtk4::Label::new(Some("—"));
        freq.set_xalign(0.0);
        freq.add_css_class("sysmon-core-freq");
        freq.set_tooltip_text(Some("CPU load % and current clock frequency (MHz)"));
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        flow.append(&v);
        core_load.push(bar);
        core_freq.push(freq);
    }
    chip_wrap.append(&flow);

    let summary = boxed_list();
    let (core_row, core_value) = row(
        "Core",
        "Combined power draw of the CPU cores (RAPL core domain)",
    );
    summary.append(&core_row);
    let (pkg_row, pkg_value) = row(
        "Package",
        "Total CPU package power, incl. cores, cache, and GPU (RAPL package domain)",
    );
    summary.append(&pkg_row);
    let (temp_row, temp_value) = row("Temperature", "CPU package temperature");
    summary.append(&temp_row);
    let (model_row, cpu_name) = row("Model", "CPU model name from /proc/cpuinfo");
    summary.append(&model_row);
    let (max_row, cpu_max) = row("Max frequency", "Hardware max CPU frequency (cpuinfo_max_freq)");
    summary.append(&max_row);
    let (boost_row, cpu_boost) = row("Turbo boost", "Whether frequency boosting is allowed");
    summary.append(&boost_row);
    cbody.append(&summary);

    let rapl_hint = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    rapl_hint.add_css_class("sysmon-warning");
    let warn_icon = gtk4::Image::from_icon_name("dialog-warning-symbolic");
    warn_icon.set_pixel_size(16);
    let hint_label = gtk4::Label::new(Some(
        "RAPL not readable — run: sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap",
    ));
    hint_label.set_wrap(true);
    hint_label.set_xalign(0.0);
    rapl_hint.append(&warn_icon);
    rapl_hint.append(&hint_label);
    rapl_hint.set_visible(false);
    cbody.append(&rapl_hint);

    let (batt_card, bbody, bheader) = card("battery-symbolic", "Battery");
    let batt_pct = trail_label();
    bheader.append(&batt_pct);
    let bat_bar = ProgressBar::new();
    bat_bar.add_css_class("sysmon-battery-bar");
    bat_bar.set_fraction(0.0);
    bat_bar.set_show_text(false);
    bat_bar.set_hexpand(true);
    bbody.append(&bat_bar);
    let blist = boxed_list();
    let (health_row, bat_health) =
        row("Health", "Battery capacity vs. its design capacity");
    blist.append(&health_row);
    let (charge_row, bat_charge) = row("Charging", "Current charging power draw");
    blist.append(&charge_row);
    let (discharge_row, bat_discharge) =
        row("Discharging", "Current discharging power draw");
    blist.append(&discharge_row);
    let (cycle_row, bat_cycle) = row("Cycle count", "Battery charge/discharge cycles");
    blist.append(&cycle_row);
    let (temp_row_bat, bat_temp) = row("Temperature", "Battery temperature");
    blist.append(&temp_row_bat);
    let (design_cap_row, bat_design_cap) = row(
        "Design capacity",
        "Rated capacity when the battery was new (charge_full_design). Shown in mAh for \
         charge-based batteries or Wh for energy-based ones.",
    );
    blist.append(&design_cap_row);
    let (full_cap_row, bat_full_cap) = row(
        "Full capacity",
        "Current maximum capacity when fully charged (charge_full). Degrades below the \
         design capacity as the battery ages; the gap is the health loss.",
    );
    blist.append(&full_cap_row);
    let (remain_cap_row, bat_remain_cap) = row(
        "Remaining",
        "Charge currently stored in the battery (charge_now). Drops while discharging and \
         rises while charging.",
    );
    blist.append(&remain_cap_row);
    bbody.append(&blist);

    let (mem_card, mbody, _mheader) = card("drive-harddisk-solidstate-symbolic", "Memory");
    let mem_seg_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    mem_seg_bar.add_css_class("sysmon-seg-bar");
    mem_seg_bar.set_hexpand(true);
    let mem_seg_used = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_used.add_css_class("sysmon-seg-used");
    mem_seg_used.set_size_request(0, -1);
    let mem_seg_cache = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_cache.add_css_class("sysmon-seg-cache");
    mem_seg_cache.set_size_request(0, -1);
    let mem_seg_free = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    mem_seg_free.add_css_class("sysmon-seg-free");
    mem_seg_free.set_size_request(0, -1);
    mem_seg_bar.append(&mem_seg_used);
    mem_seg_bar.append(&mem_seg_cache);
    mem_seg_bar.append(&mem_seg_free);
    mbody.append(&mem_seg_bar);

    let legend = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    legend.set_margin_top(2);
    legend.append(&legend_item("sysmon-seg-used", "Used"));
    legend.append(&legend_item("sysmon-seg-cache", "Cache"));
    legend.append(&legend_item("sysmon-seg-free", "Free"));
    mbody.append(&legend);

    let mlist = boxed_list();
    let (used_row, mem_used) =
        row("Used", "MemTotal − MemAvailable, as System Monitor counts it");
    mlist.append(&used_row);
    let (avail_row, mem_avail) =
        row("Available", "MemAvailable — memory usable by apps without swapping");
    mlist.append(&avail_row);
    let (cache_row, mem_cache) = row("Cache", "Buffers + Cached (reclaimable page cache)");
    mlist.append(&cache_row);
    let (free_row, mem_free) = row("Free", "MemFree (completely unused)");
    mlist.append(&free_row);
    let (swap_row, mem_swap) = row("Swap", "Swap used / total");
    mlist.append(&swap_row);
    let (zram_row, mem_zram) = row("Zram", "Compressed zram swap in use");
    mlist.append(&zram_row);
    mbody.append(&mlist);

    let (net_card, nbody, _nheader) = card("network-wireless-symbolic", "Network");
    let nlist = boxed_list();
    let (down_row, net_down) = row("↓ Download", "Download rate");
    nlist.append(&down_row);
    let (up_row, net_up) = row("↑ Upload", "Upload rate");
    nlist.append(&up_row);
    nbody.append(&nlist);
    let net_ifaces_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    net_ifaces_box.add_css_class("boxed-list");
    net_ifaces_box.set_margin_top(10);
    nbody.append(&net_ifaces_box);

    let (proc_card, pbody, pheader) = card("view-list-symbolic", "Processes");
    let search_query = Rc::new(RefCell::new(String::new()));
    let apps_table = build_apps_table(Rc::clone(&search_query));
    let procs_table = build_procs_table(Rc::clone(&search_query));

    let search_entry = gtk4::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search processes…"));
    search_entry.set_max_width_chars(20);
    let apps_filter_clone = apps_table.filter.clone();
    let procs_filter_clone = procs_table.filter.clone();
    let sq_clone = Rc::clone(&search_query);
    let apps_view_clone = apps_table.view.clone();
    let procs_view_clone = procs_table.view.clone();
    let apps_us_clone = Rc::clone(&apps_table.user_scrolled);
    let procs_us_clone = Rc::clone(&procs_table.user_scrolled);
    let apps_adj_clone = apps_table.sw.vadjustment();
    let procs_adj_clone = procs_table.sw.vadjustment();
    search_entry.connect_search_changed(move |entry| {
        let text = entry.text().trim().to_string();
        *sq_clone.borrow_mut() = text;
        apps_filter_clone.changed(gtk4::FilterChange::Different);
        procs_filter_clone.changed(gtk4::FilterChange::Different);
        apps_us_clone.set(false);
        procs_us_clone.set(false);
        apps_adj_clone.set_value(0.0);
        procs_adj_clone.set_value(0.0);
        apps_view_clone.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
        procs_view_clone.scroll_to(0, None, gtk4::ListScrollFlags::NONE, None);
    });

    let stack = gtk4::Stack::new();
    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));

    pheader.append(&switcher);
    pheader.append(&search_entry);

    stack.add_titled(&apps_table.sw, Some("apps"), "Apps");
    stack.add_titled(&procs_table.sw, Some("procs"), "Processes");
    pbody.append(&stack);

    proc_card.set_vexpand(true);
    pbody.set_vexpand(true);
    stack.set_vexpand(true);

    let view_stack = adw::ViewStack::new();
    view_stack.set_vhomogeneous(false);
    view_stack.add_titled_with_icon(&page(&cpu_card), Some("cpu"), "CPU", "view-grid-symbolic");
    view_stack.add_titled_with_icon(
        &page(&batt_card),
        Some("battery"),
        "Battery",
        "battery-symbolic",
    );
    view_stack.add_titled_with_icon(
        &page(&mem_card),
        Some("memory"),
        "Memory",
        "drive-harddisk-solidstate-symbolic",
    );
    view_stack.add_titled_with_icon(
        &page(&net_card),
        Some("network"),
        "Network",
        "network-wireless-symbolic",
    );
    let proc_page = page(&proc_card);
    proc_page.set_vexpand(true);
    view_stack.add_titled_with_icon(
        &proc_page,
        Some("process"),
        "Process",
        "view-list-symbolic",
    );
    let view_switcher = adw::ViewSwitcher::new();
    view_switcher.set_stack(Some(&view_stack));
    view_switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    view_switcher.set_halign(gtk4::Align::Center);

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroller.set_child(Some(&view_stack));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&view_switcher);
    toolbar.set_content(Some(&scroller));
    toast_overlay.set_child(Some(&toolbar));
    window.set_content(Some(&toast_overlay));

    let ui = Rc::new(Ui {
        core_load,
        core_freq,
        cpu_avg,
        core_value,
        pkg_value,
        temp_value,
        cpu_name,
        cpu_max,
        cpu_boost,
        rapl_hint,
        bat_bar,
        batt_pct,
        bat_health,
        bat_charge,
        bat_discharge,
        bat_cycle,
        bat_cycle_row: cycle_row,
        bat_temp,
        bat_temp_row: temp_row_bat,
        bat_design_cap,
        bat_full_cap,
        bat_remain_cap,
        mem_seg_bar,
        mem_seg_used,
        mem_seg_cache,
        mem_seg_free,
        mem_used,
        mem_cache,
        mem_free,
        mem_swap,
        mem_zram,
        mem_avail,
        net_down,
        net_up,
        net_ifaces_box,
        apps_table,
        procs_table,
        apps_items: RefCell::new(HashMap::new()),
        procs_items: RefCell::new(HashMap::new()),
        net_iface_rows: RefCell::new(Vec::new()),
    });

    enum WorkerMsg {
        Snapshot(Box<QuickSnapshot>, ProcSnapshot),
        Error(String),
    }

    let (sender, receiver) = std::sync::mpsc::channel::<WorkerMsg>();
    let ui_receiver = Rc::clone(&ui);
    let frozen_recv = Rc::clone(&frozen);
    let toast_overlay_err = toast_overlay.clone();

    glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(msg) = receiver.try_recv() {
            match msg {
                WorkerMsg::Snapshot(quick, procs) => {
                    if !frozen_recv.get() {
                        ui_receiver.update_quick(&quick);
                        ui_receiver.refill_tables(&procs.apps, &procs.procs);
                    }
                }
                WorkerMsg::Error(err) => {
                    let toast = adw::Toast::new(&format!("Monitoring stopped: {err}"));
                    toast.set_timeout(0);
                    toast_overlay_err.add_toast(toast);
                }
            }
        }
        glib::ControlFlow::Continue
    });

    thread::spawn(move || {
        let mut sampler =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(Sampler::new)) {
                Ok(s) => s,
                Err(e) => {
                    let _ = sender.send(WorkerMsg::Error(format!("sampler init failed: {e:?}")));
                    return;
                }
            };
        loop {
            thread::sleep(Duration::from_millis(1000));
            let sampled = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (sampler.sample_quick(), sampler.sample_procs())
            }));
            match sampled {
                Ok((quick, procs)) => {
                    if sender.send(WorkerMsg::Snapshot(Box::new(quick), procs)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    let _ = sender.send(WorkerMsg::Error(format!("sampler crashed: {e:?}")));
                    break;
                }
            }
        }
    });

    window
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_capacity_dual_formats_charge_and_energy() {
        assert_eq!(
            human_capacity_dual(52_046_000, "mAh", Some(4_200_000)),
            "52046 mAh (218.59 Wh)"
        );
        assert_eq!(
            human_capacity_dual(82_000_000, "Wh", Some(13_000_000)),
            "6308 mAh (82.00 Wh)"
        );
    }

    #[test]
    fn human_capacity_dual_edge_cases() {
        assert_eq!(human_capacity_dual(52_046_000, "mAh", None), "52046 mAh");
        assert_eq!(human_capacity_dual(82_000_000, "Wh", None), "82.00 Wh");
        assert_eq!(human_capacity_dual(52_046_000, "mAh", Some(0)), "52046 mAh");
        assert_eq!(human_capacity_dual(123, "bogus", Some(4_200_000)), "123");
        assert_eq!(human_capacity_dual(0, "mAh", Some(4_200_000)), "0 mAh (0.00 Wh)");
    }

    #[test]
    fn proc_icon_candidates_resolves_aliases_and_falls_back() {
        let c = proc_icon_candidates("Chrome");
        assert!(c.contains(&"google-chrome".to_string()));
        let c2 = proc_icon_candidates("zzz-totally-not-real");
        assert!(c2.contains(&"zzz-totally-not-real".to_string()));
        assert_eq!(c2.last().unwrap(), "application-x-executable");
        let c3 = proc_icon_candidates("python3");
        assert!(c3.contains(&"python".to_string()));
    }

    #[test]
    fn process_table_sorting_and_filtering() {
        if gtk4::init().is_err() || !gtk4::is_initialized_main_thread() {
            eprintln!("skipping: GTK could not be initialized on main thread");
            return;
        }

        // Test 1: Sorting and in-place refresh with ColumnView
        let store: gio::ListStore = gio::ListStore::new::<ProcItem>();
        let a = ProcItem::new("a", "", 5.0, 0.0, 0, 1, 1, "1");
        let b = ProcItem::new("b", "", 3.0, 0.0, 0, 2, 1, "2");
        let c = ProcItem::new("c", "", 1.0, 0.0, 0, 3, 1, "3");
        for it in [&a, &b, &c] {
            store.append(it);
        }
        let sort_model = gtk4::SortListModel::new(Some(store.clone()), None::<gtk4::Sorter>);
        sort_model.set_incremental(false);
        let selection = gtk4::NoSelection::new(Some(sort_model.clone()));
        let view = gtk4::ColumnView::new(Some(selection));

        let cpu_sorter = gtk4::CustomSorter::new(|a, b| {
            let a = a.downcast_ref::<ProcItem>().unwrap();
            let b = b.downcast_ref::<ProcItem>().unwrap();
            a.cpu().partial_cmp(&b.cpu()).unwrap_or(std::cmp::Ordering::Equal).into()
        });
        let cpu_col = gtk4::ColumnViewColumn::new(Some("CPU"), Some(proc_cpu_factory()));
        cpu_col.set_sorter(Some(&cpu_sorter));
        view.append_column(&cpu_col);

        view.sort_by_column(Some(&cpu_col), gtk4::SortType::Descending);
        sort_model.set_sorter(view.sorter().as_ref());

        let order = |sm: &gtk4::SortListModel| -> Vec<String> {
            (0..sm.n_items())
                .map(|i| sm.item(i).unwrap().downcast::<ProcItem>().unwrap().name())
                .collect()
        };

        assert_eq!(order(&sort_model), vec!["a", "b", "c"]);

        a.set_cpu(1.0);
        b.set_cpu(5.0);
        c.set_cpu(3.0);
        assert_eq!(order(&sort_model), vec!["a", "b", "c"]);

        if let Some(sorter) = view.sorter() {
            sorter.changed(gtk4::SorterChange::Different);
        }
        assert_eq!(order(&sort_model), vec!["b", "c", "a"]);

        // Test 2: Search filtering
        let fstore: gio::ListStore = gio::ListStore::new::<ProcItem>();
        let fa = ProcItem::new("Firefox", "", 5.0, 0.0, 0, 100, 1, "100");
        let fb = ProcItem::new("Chrome", "", 3.0, 0.0, 0, 200, 1, "200");
        let fc = ProcItem::new("Terminal", "", 1.0, 0.0, 0, 300, 1, "300");
        for it in [&fa, &fb, &fc] {
            fstore.append(it);
        }

        let query = Rc::new(RefCell::new(String::new()));
        let q_clone = Rc::clone(&query);
        let filter = gtk4::CustomFilter::new(move |obj| {
            let q = q_clone.borrow();
            if q.is_empty() {
                return true;
            }
            let Some(it) = obj.downcast_ref::<ProcItem>() else {
                return true;
            };
            let q_lower = q.to_lowercase();
            it.name().to_lowercase().contains(&q_lower) || it.pid().to_string().contains(&q_lower)
        });
        let filter_model = gtk4::FilterListModel::new(Some(fstore.clone()), Some(filter.clone()));
        filter_model.set_incremental(false);

        assert_eq!(filter_model.n_items(), 3);

        *query.borrow_mut() = "fire".to_string();
        filter.changed(gtk4::FilterChange::Different);
        assert_eq!(filter_model.n_items(), 1);
        let item = filter_model.item(0).unwrap().downcast::<ProcItem>().unwrap();
        assert_eq!(item.name(), "Firefox");

        *query.borrow_mut() = "200".to_string();
        filter.changed(gtk4::FilterChange::Different);
        assert_eq!(filter_model.n_items(), 1);
        let item = filter_model.item(0).unwrap().downcast::<ProcItem>().unwrap();
        assert_eq!(item.name(), "Chrome");

        *query.borrow_mut() = "".to_string();
        filter.changed(gtk4::FilterChange::Different);
        assert_eq!(filter_model.n_items(), 3);
    }
}
