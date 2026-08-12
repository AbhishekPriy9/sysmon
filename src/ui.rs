use std::rc::Rc;
use std::thread;
use std::time::Duration;

use libadwaita as adw;

use gtk4::prelude::*;
use gtk4::{glib, ListStore, ProgressBar};
use libadwaita::prelude::*;

use crate::model::Snapshot;
use crate::sampler::Sampler;

struct Ui {
    core_load: Vec<ProgressBar>,
    core_freq: Vec<gtk4::Label>,
    cpu_core: gtk4::Label,
    cpu_pkg: gtk4::Label,
    cpu_temp: gtk4::Label,
    rapl_hint: gtk4::Label,
    bat_bar: ProgressBar,
    bat_health: gtk4::Label,
    bat_charge: gtk4::Label,
    bat_discharge: gtk4::Label,
    mem_bar: ProgressBar,
    mem_text: gtk4::Label,
    net_down: gtk4::Label,
    net_up: gtk4::Label,
    apps_store: ListStore,
    procs_store: ListStore,
}

impl Ui {
    fn update(&self, s: &Snapshot) {
        for (i, c) in s.cpu.cores.iter().enumerate() {
            if let Some(b) = self.core_load.get(i) {
                b.set_fraction((c.load / 100.0).clamp(0.0, 1.0));
            }
            if let Some(l) = self.core_freq.get(i) {
                l.set_text(&format!("{:.0}% · {} MHz", c.load, c.freq_mhz));
            }
        }
        let pw = s
            .cpu
            .pkg_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let cw = s
            .cpu
            .core_watts
            .map(|w| format!("{w:.1} W"))
            .unwrap_or_else(|| "no access".into());
        let t = s
            .cpu
            .temp_c
            .map(|t| format!("{t:.0} °C"))
            .unwrap_or_else(|| "—".into());
        self.cpu_core.set_text(&format!("Core {cw}"));
        self.cpu_pkg.set_text(&format!("PKG {pw}"));
        self.cpu_temp.set_text(&format!("{t}"));
        self.rapl_hint.set_visible(s.cpu.pkg_watts.is_none());

        if let Some(b) = &s.battery {
            self.bat_bar.set_fraction((b.charge_pct / 100.0).clamp(0.0, 1.0));
            self.bat_health.set_text(&format!("Health {:.0}%", b.health_pct));
            let charging = b.status.starts_with("Charging");
            let discharging = b.status.starts_with("Discharging");
            let charge_w = if charging { b.watts.abs() } else { 0.0 };
            let discharge_w = if discharging { b.watts.abs() } else { 0.0 };
            self.bat_charge.set_text(&format!("Charging: {charge_w:.1} W"));
            self.bat_discharge
                .set_text(&format!("Discharging: {discharge_w:.1} W"));
        } else {
            self.bat_bar.set_fraction(0.0);
            self.bat_health.set_text("No battery");
            self.bat_charge.set_text("Charging: —");
            self.bat_discharge.set_text("Discharging: —");
        }

        let used = s.mem.total_kb.saturating_sub(s.mem.avail_kb);
        let frac = if s.mem.total_kb == 0 {
            0.0
        } else {
            used as f64 / s.mem.total_kb as f64
        };
        self.mem_bar.set_fraction(frac.clamp(0.0, 1.0));
        let swap_used = s.mem.swap_total_kb.saturating_sub(s.mem.swap_free_kb);
        self.mem_text.set_text(&format!(
            "Used {} / {}\nSwap {} / {}\nZram {}",
            human_kb(used),
            human_kb(s.mem.total_kb),
            human_kb(swap_used),
            human_kb(s.mem.swap_total_kb),
            human_kb(s.mem.zram_compressed_kb),
        ));

        self.net_up.set_text(&format!("↑ {}", human_bps(s.net.up_bps)));
        self.net_down
            .set_text(&format!("↓ {}", human_bps(s.net.down_bps)));

        self.refill_table(&self.apps_store, &s.apps, &s.procs, true);
        self.refill_table(&self.procs_store, &s.apps, &s.procs, false);
    }

    fn refill_table(
        &self,
        store: &ListStore,
        apps: &[crate::model::AppRow],
        procs: &[crate::model::ProcRow],
        apps_view: bool,
    ) {
        store.clear();
        if apps_view {
            for a in apps {
                let it = store.append();
                store.set_value(&it, 0, &a.name.to_value());
                store.set_value(&it, 1, &(a.cpu_pct.round() as u32).to_value());
                store.set_value(&it, 2, &human_kb(a.rss_kb).to_value());
                store.set_value(&it, 3, &a.proc_count.to_value());
                store.set_value(&it, 4, &a.rss_kb.to_value());
            }
        } else {
            for p in procs {
                let it = store.append();
                store.set_value(&it, 0, &p.name.to_value());
                store.set_value(&it, 1, &p.pid.to_value());
                store.set_value(&it, 2, &(p.cpu_pct.round() as u32).to_value());
                store.set_value(&it, 3, &human_kb(p.rss_kb).to_value());
                store.set_value(&it, 4, &p.rss_kb.to_value());
            }
        }
    }
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

fn card(title: &str) -> (adw::PreferencesGroup, gtk4::Box) {
    let group = adw::PreferencesGroup::new();
    group.set_title(title);
    let inner = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    inner.set_margin_top(4);
    inner.set_margin_bottom(4);
    inner.set_margin_start(8);
    inner.set_margin_end(8);
    group.add(&inner);
    (group, inner)
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
    ]);
    let view = gtk4::TreeView::with_model(&store);
    view.set_headers_clickable(true);
    view.set_grid_lines(gtk4::TreeViewGridLines::Horizontal);
    view.set_fixed_height_mode(true);
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
    view.set_grid_lines(gtk4::TreeViewGridLines::Horizontal);
    view.set_fixed_height_mode(true);
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
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Sysmon")
        .default_width(864)
        .default_height(640)
        .build();

    let scroller = gtk4::ScrolledWindow::new();
    scroller.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);
    scroller.set_child(Some(&root));

    let header = adw::HeaderBar::new();
    let frozen = Rc::new(std::cell::Cell::new(false));
    let freeze_btn = gtk4::ToggleButton::new();
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
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));
    window.set_content(Some(&toolbar));

    // CPU card
    let (cg, cbox) = card("CPU");
    let flow = gtk4::FlowBox::new();
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(4);
    flow.set_homogeneous(true);
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_activate_on_single_click(false);
    flow.set_hexpand(true);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..8 {
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        v.set_size_request(170, -1);
        let name = gtk4::Label::new(Some(&format!("CPU{i}")));
        name.set_xalign(0.0);
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        let freq = gtk4::Label::new(Some("—"));
        freq.set_xalign(0.0);
        freq.set_tooltip_text(Some("CPU load % and current clock frequency (MHz)"));
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        flow.append(&v);
        core_load.push(bar);
        core_freq.push(freq);
    }
    cbox.append(&flow);
    let core_label = gtk4::Label::new(Some("—"));
    core_label.set_xalign(0.0);
    core_label.set_tooltip_text(Some(
        "Combined power draw of the CPU cores (RAPL core domain)",
    ));
    let pkg_label = gtk4::Label::new(Some("—"));
    pkg_label.set_xalign(0.0);
    pkg_label.set_tooltip_text(Some(
        "Total CPU package power, incl. cores, cache, and GPU (RAPL package domain)",
    ));
    let temp_label = gtk4::Label::new(Some("—"));
    temp_label.set_xalign(0.0);
    temp_label.set_tooltip_text(Some("CPU package temperature"));
    let summary_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    summary_row.append(&core_label);
    summary_row.append(&pkg_label);
    summary_row.append(&temp_label);
    cbox.append(&summary_row);
    let rapl_hint = gtk4::Label::new(Some(
        "RAPL not readable — run: sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=powercap",
    ));
    rapl_hint.set_xalign(0.0);
    rapl_hint.set_visible(false);
    cbox.append(&rapl_hint);
    root.append(&cg);

    // Battery card
    let (bg, bbox) = card("Battery");
    let bat_bar = ProgressBar::new();
    bbox.append(&bat_bar);
    let bat_health = gtk4::Label::new(Some("—"));
    bat_health.set_xalign(0.0);
    bat_health.set_tooltip_text(Some("Battery capacity vs. its design capacity"));
    bbox.append(&bat_health);
    let bat_charge = gtk4::Label::new(Some("Charging: —"));
    bat_charge.set_xalign(0.0);
    bat_charge.set_tooltip_text(Some("Current charging power draw"));
    bbox.append(&bat_charge);
    let bat_discharge = gtk4::Label::new(Some("Discharging: —"));
    bat_discharge.set_xalign(0.0);
    bat_discharge.set_tooltip_text(Some("Current discharging power draw"));
    bbox.append(&bat_discharge);

    // Memory card
    let (mg, mbox) = card("Memory");
    let mem_bar = ProgressBar::new();
    mbox.append(&mem_bar);
    let mem_text = gtk4::Label::new(Some("—"));
    mem_text.set_xalign(0.0);
    mem_text.set_tooltip_text(Some("RAM in use, swap usage, and zram compressed swap size"));
    mbox.append(&mem_text);

    // Network card
    let (ng, nbox) = card("Network");
    let net_down = gtk4::Label::new(Some("↓ —"));
    net_down.set_xalign(0.0);
    net_down.set_tooltip_text(Some("Download rate"));
    nbox.append(&net_down);
    let net_up = gtk4::Label::new(Some("↑ —"));
    net_up.set_xalign(0.0);
    net_up.set_tooltip_text(Some("Upload rate"));
    nbox.append(&net_up);

    // Stat cards in a fixed 2-column grid; Network spans the full second row
    let stat_grid = gtk4::Grid::new();
    stat_grid.set_column_spacing(12);
    stat_grid.set_row_spacing(12);
    bg.set_hexpand(true);
    mg.set_hexpand(true);
    ng.set_hexpand(true);
    stat_grid.attach(&bg, 0, 0, 1, 1);
    stat_grid.attach(&mg, 1, 0, 1, 1);
    stat_grid.attach(&ng, 0, 1, 2, 1);
    root.append(&stat_grid);

    // Processes card
    let (pg, pbox) = card("Processes");
    let stack = gtk4::Stack::new();
    let switcher = gtk4::StackSwitcher::new();
    switcher.set_stack(Some(&stack));
    pbox.append(&switcher);
    let (apps_store, apps_view) = build_apps_table();
    let (procs_store, procs_view) = build_procs_table();
    stack.add_titled(&apps_view, Some("apps"), "Apps");
    stack.add_titled(&procs_view, Some("procs"), "Processes");
    pbox.append(&stack);
    root.append(&pg);

    let ui = Rc::new(Ui {
        core_load,
        core_freq,
        cpu_core: core_label,
        cpu_pkg: pkg_label,
        cpu_temp: temp_label,
        rapl_hint,
        bat_bar,
        bat_health,
        bat_charge,
        bat_discharge,
        mem_bar,
        mem_text,
        net_down,
        net_up,
        apps_store,
        procs_store,
    });

    let (sender, receiver) = std::sync::mpsc::channel::<Snapshot>();
    let ui2 = Rc::clone(&ui);
    let frozen_loop = Rc::clone(&frozen);
    glib::timeout_add_local(Duration::from_millis(250), move || {
        while let Ok(s) = receiver.try_recv() {
            if !frozen_loop.get() {
                ui2.update(&s);
            }
        }
        glib::ControlFlow::Continue
    });
    thread::spawn(move || {
        let mut sampler = Sampler::new();
        loop {
            thread::sleep(Duration::from_millis(1000));
            if sender.send(sampler.sample()).is_err() {
                break;
            }
        }
    });

    window
}
