use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use libadwaita as adw;

use gtk4::prelude::*;
use gtk4::{glib, ListStore, ProgressBar, TreeIter};
use libadwaita::prelude::*;

use crate::model::{AppRow, ProcRow, ProcSnapshot, QuickSnapshot};
use crate::sampler::{online_count, Sampler};

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
"#;

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
    apps_store: ListStore,
    procs_store: ListStore,
    apps_iters: RefCell<HashMap<String, TreeIter>>,
    procs_iters: RefCell<HashMap<u32, TreeIter>>,
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
        }

        let t = s.mem.total_kb;
        let used = t.saturating_sub(s.mem.avail_kb);
        let cache = s.mem.cache_kb;
        let free = s.mem.free_kb;
        let swap_used = s.mem.swap_total_kb.saturating_sub(s.mem.swap_free_kb);

        let pct = |v: u64| format!(" ({:.0}%)", 100.0 * v as f64 / t as f64);
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
    }

    fn update_apps(&self, apps: &[AppRow]) {
        let mut iters = self.apps_iters.borrow_mut();
        let store = &self.apps_store;
        let mut seen = HashSet::with_capacity(apps.len());
        for a in apps {
            seen.insert(a.name.clone());
            let it = match iters.get(&a.name) {
                Some(it) => it.clone(),
                None => {
                    let it = store.append();
                    iters.insert(a.name.clone(), it.clone());
                    it
                }
            };
            store.set_value(&it, 0, &a.name.to_value());
            store.set_value(&it, 1, &(a.cpu_pct.round() as u32).to_value());
            store.set_value(&it, 2, &human_kb(a.rss_kb).to_value());
            store.set_value(&it, 3, &a.proc_count.to_value());
            store.set_value(&it, 4, &a.rss_kb.to_value());
            store.set_value(
                &it,
                5,
                &a.pids
                    .iter()
                    .map(|p| p.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
                    .to_value(),
            );
        }
        let mut gone = Vec::new();
        for (name, it) in iters.iter() {
            if !seen.contains(name) {
                store.remove(it);
                gone.push(name.clone());
            }
        }
        for n in gone {
            iters.remove(&n);
        }
    }

    fn update_procs(&self, procs: &[ProcRow]) {
        let mut iters = self.procs_iters.borrow_mut();
        let store = &self.procs_store;
        let mut seen = HashSet::with_capacity(procs.len());
        for p in procs {
            seen.insert(p.pid);
            let it = match iters.get(&p.pid) {
                Some(it) => it.clone(),
                None => {
                    let it = store.append();
                    iters.insert(p.pid, it.clone());
                    it
                }
            };
            store.set_value(&it, 0, &p.name.to_value());
            store.set_value(&it, 1, &p.pid.to_value());
            store.set_value(&it, 2, &(p.cpu_pct.round() as u32).to_value());
            store.set_value(&it, 3, &human_kb(p.rss_kb).to_value());
            store.set_value(&it, 4, &p.rss_kb.to_value());
        }
        let mut gone = Vec::new();
        for (pid, it) in iters.iter() {
            if !seen.contains(pid) {
                store.remove(it);
                gone.push(*pid);
            }
        }
        for pid in gone {
            iters.remove(&pid);
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
    provider.load_from_data(CSS);
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

fn add_text_column(
    view: &gtk4::TreeView,
    title: &str,
    model_idx: i32,
    sort_idx: i32,
    numeric: bool,
    expand: bool,
    min_width: i32,
) {
    let col = gtk4::TreeViewColumn::new();
    col.set_title(title);
    let cell = gtk4::CellRendererText::new();
    cell.set_padding(10, 8);
    if numeric {
        cell.set_xalign(1.0);
    } else {
        cell.set_xalign(0.0);
        cell.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    }
    col.pack_start(&cell, true);
    col.add_attribute(&cell, "text", model_idx);
    col.set_sort_column_id(sort_idx);
    col.set_resizable(true);
    col.set_expand(expand);
    col.set_min_width(min_width);
    view.append_column(&col);
}

fn build_apps_table() -> (ListStore, gtk4::ScrolledWindow) {
    let store = ListStore::new(&[
        glib::Type::STRING,
        glib::Type::U32,
        glib::Type::STRING,
        glib::Type::U32,
        glib::Type::U64,
        glib::Type::STRING,
    ]);
    let view = gtk4::TreeView::with_model(&store);
    view.set_headers_clickable(true);
    view.set_grid_lines(gtk4::TreeViewGridLines::Both);
    add_text_column(&view, "App", 0, 0, false, true, 180);
    add_text_column(&view, "CPU %", 1, 1, true, false, 70);
    add_text_column(&view, "MEM", 2, 4, false, false, 100);
    add_text_column(&view, "Procs", 3, 3, true, false, 70);
    store.set_sort_column_id(gtk4::SortColumn::Index(1), gtk4::SortType::Descending);
    let sw = gtk4::ScrolledWindow::new();
    sw.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    sw.set_child(Some(&view));
    sw.set_height_request(300);
    (store, sw)
}

fn build_procs_table() -> (ListStore, gtk4::ScrolledWindow) {
    let store = ListStore::new(&[
        glib::Type::STRING,
        glib::Type::U32,
        glib::Type::U32,
        glib::Type::STRING,
        glib::Type::U64,
    ]);
    let view = gtk4::TreeView::with_model(&store);
    view.set_headers_clickable(true);
    view.set_grid_lines(gtk4::TreeViewGridLines::Both);
    add_text_column(&view, "Name", 0, 0, false, true, 180);
    add_text_column(&view, "PID", 1, 1, true, false, 80);
    add_text_column(&view, "CPU %", 2, 2, true, false, 70);
    add_text_column(&view, "MEM", 3, 4, false, false, 100);
    store.set_sort_column_id(gtk4::SortColumn::Index(2), gtk4::SortType::Descending);
    let sw = gtk4::ScrolledWindow::new();
    sw.set_policy(gtk4::PolicyType::Automatic, gtk4::PolicyType::Automatic);
    sw.set_child(Some(&view));
    sw.set_height_request(300);
    (store, sw)
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
        d.set_website("https://github.com/example/sysmon");
        d.present(app_about.active_window().as_ref());
    });
    header.pack_end(&about_btn);

    // CPU page
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

    // Battery page
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
    bbody.append(&blist);

    // Memory page
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

    // Network page
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

    // Processes page
    let (proc_card, pbody, pheader) = card("view-list-symbolic", "Processes");
    let stack = gtk4::Stack::new();
    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    pheader.append(&switcher);
    let (apps_store, apps_sw) = build_apps_table();
    let (procs_store, procs_sw) = build_procs_table();
    stack.add_titled(&apps_sw, Some("apps"), "Apps");
    stack.add_titled(&procs_sw, Some("procs"), "Processes");
    pbody.append(&stack);

    // View stack + switcher
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
    view_stack.add_titled_with_icon(
        &page(&proc_card),
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
        apps_store,
        procs_store,
        apps_iters: RefCell::new(HashMap::new()),
        procs_iters: RefCell::new(HashMap::new()),
        net_iface_rows: RefCell::new(Vec::new()),
    });

    let (quick_sender, quick_receiver) = std::sync::mpsc::channel::<QuickSnapshot>();
    let (proc_sender, proc_receiver) = std::sync::mpsc::channel::<ProcSnapshot>();
    let ui2 = Rc::clone(&ui);
    let frozen_loop = Rc::clone(&frozen);
    glib::timeout_add_local(Duration::from_millis(250), move || {
        while let Ok(s) = quick_receiver.try_recv() {
            if !frozen_loop.get() {
                ui2.update_quick(&s);
            }
        }
        glib::ControlFlow::Continue
    });
    let ui3 = Rc::clone(&ui);
    let frozen_proc = Rc::clone(&frozen);
    glib::timeout_add_local(Duration::from_millis(250), move || {
        while let Ok(s) = proc_receiver.try_recv() {
            if !frozen_proc.get() {
                ui3.refill_tables(&s.apps, &s.procs);
            }
        }
        glib::ControlFlow::Continue
    });
    thread::spawn(move || {
        let mut sampler = Sampler::new();
        loop {
            thread::sleep(Duration::from_millis(1000));
            if quick_sender.send(sampler.sample_quick()).is_err()
                || proc_sender.send(sampler.sample_procs()).is_err()
            {
                break;
            }
        }
    });

    window
}
