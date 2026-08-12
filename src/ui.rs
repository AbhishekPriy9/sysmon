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
    cpu_summary: gtk4::Label,
    rapl_hint: gtk4::Label,
    bat_bar: ProgressBar,
    bat_health: gtk4::Label,
    bat_watts: gtk4::Label,
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
        self.cpu_summary
            .set_text(&format!("core {cw}  ·  pkg {pw}  ·  {t}"));
        self.rapl_hint.set_visible(s.cpu.pkg_watts.is_none());

        if let Some(b) = &s.battery {
            self.bat_bar.set_fraction((b.charge_pct / 100.0).clamp(0.0, 1.0));
            self.bat_health.set_text(&format!("health {:.0}%", b.health_pct));
            self.bat_watts
                .set_text(&format!("{} ({:.1} W)", b.status, b.watts));
        } else {
            self.bat_bar.set_fraction(0.0);
            self.bat_health.set_text("no battery");
            self.bat_watts.set_text("");
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
            "used {:.1} GB / {:.1} GB  ·  swap {:.1} / {:.1} GB  ·  zram {:.0} MB",
            used as f64 / 1e6,
            s.mem.total_kb as f64 / 1e6,
            swap_used as f64 / 1e6,
            s.mem.swap_total_kb as f64 / 1e6,
            s.mem.zram_compressed_kb as f64 / 1024.0,
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
                store.set_value(&it, 1, &a.cpu_pct.to_value());
                store.set_value(&it, 2, &a.mem_pct.to_value());
                store.set_value(&it, 3, &a.proc_count.to_value());
            }
        } else {
            for p in procs {
                let it = store.append();
                store.set_value(&it, 0, &p.name.to_value());
                store.set_value(&it, 1, &p.pid.to_value());
                store.set_value(&it, 2, &p.cpu_pct.to_value());
                store.set_value(&it, 3, &p.mem_pct.to_value());
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

fn build_apps_table() -> (ListStore, gtk4::ScrolledWindow) {
    let store = ListStore::new(&[glib::Type::STRING, glib::Type::F64, glib::Type::F64, glib::Type::U32]);
    let view = gtk4::TreeView::with_model(&store);
    let cols: [(&str, i32, bool); 4] = [
        ("App", 0, false),
        ("CPU %", 1, true),
        ("MEM %", 2, true),
        ("Procs", 3, false),
    ];
    for (title, idx, numeric) in cols {
        let col = gtk4::TreeViewColumn::new();
        col.set_title(title);
        let cell = gtk4::CellRendererText::new();
        if numeric {
            cell.set_xalign(1.0);
        }
        col.pack_start(&cell, true);
        col.add_attribute(&cell, "text", idx);
        col.set_sort_column_id(idx);
        view.append_column(&col);
    }
    store.set_sort_column_id(gtk4::SortColumn::Index(1), gtk4::SortType::Descending);
    let sw = gtk4::ScrolledWindow::new();
    sw.set_child(Some(&view));
    sw.set_height_request(280);
    (store, sw)
}

fn build_procs_table() -> (ListStore, gtk4::ScrolledWindow) {
    let store = ListStore::new(&[glib::Type::STRING, glib::Type::U32, glib::Type::F64, glib::Type::F64]);
    let view = gtk4::TreeView::with_model(&store);
    let cols: [(&str, i32, bool); 4] = [
        ("Name", 0, false),
        ("PID", 1, false),
        ("CPU %", 2, true),
        ("MEM %", 3, true),
    ];
    for (title, idx, numeric) in cols {
        let col = gtk4::TreeViewColumn::new();
        col.set_title(title);
        let cell = gtk4::CellRendererText::new();
        if numeric {
            cell.set_xalign(1.0);
        }
        col.pack_start(&cell, true);
        col.add_attribute(&cell, "text", idx);
        col.set_sort_column_id(idx);
        view.append_column(&col);
    }
    store.set_sort_column_id(gtk4::SortColumn::Index(2), gtk4::SortType::Descending);
    let sw = gtk4::ScrolledWindow::new();
    sw.set_child(Some(&view));
    sw.set_height_request(280);
    (store, sw)
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("sysmon")
        .default_width(480)
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
    window.set_content(Some(&scroller));

    // CPU card
    let (cg, cbox) = card("CPU");
    let grid = gtk4::Grid::new();
    grid.set_column_spacing(8);
    grid.set_row_spacing(8);
    let mut core_load = Vec::new();
    let mut core_freq = Vec::new();
    for i in 0..8 {
        let v = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        let name = gtk4::Label::new(Some(&format!("C{i}")));
        name.set_xalign(0.0);
        let bar = ProgressBar::new();
        bar.set_fraction(0.0);
        let freq = gtk4::Label::new(Some("—"));
        freq.set_xalign(0.0);
        v.append(&name);
        v.append(&bar);
        v.append(&freq);
        grid.attach(&v, i % 4, i / 4, 1, 1);
        core_load.push(bar);
        core_freq.push(freq);
    }
    cbox.append(&grid);
    let summary = gtk4::Label::new(Some("—"));
    summary.set_xalign(0.0);
    cbox.append(&summary);
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
    bbox.append(&bat_health);
    let bat_watts = gtk4::Label::new(Some("—"));
    bat_watts.set_xalign(0.0);
    bbox.append(&bat_watts);
    root.append(&bg);

    // Memory card
    let (mg, mbox) = card("Memory");
    let mem_bar = ProgressBar::new();
    mbox.append(&mem_bar);
    let mem_text = gtk4::Label::new(Some("—"));
    mem_text.set_xalign(0.0);
    mbox.append(&mem_text);
    root.append(&mg);

    // Network card
    let (ng, nbox) = card("Network");
    let net_down = gtk4::Label::new(Some("↓ —"));
    net_down.set_xalign(0.0);
    nbox.append(&net_down);
    let net_up = gtk4::Label::new(Some("↑ —"));
    net_up.set_xalign(0.0);
    nbox.append(&net_up);
    root.append(&ng);

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
    root.append(&pg);

    let ui = Rc::new(Ui {
        core_load,
        core_freq,
        cpu_summary: summary,
        rapl_hint,
        bat_bar,
        bat_health,
        bat_watts,
        mem_bar,
        mem_text,
        net_down,
        net_up,
        apps_store,
        procs_store,
    });

    let (sender, receiver) = std::sync::mpsc::channel::<Snapshot>();
    let ui2 = Rc::clone(&ui);
    glib::timeout_add_local(Duration::from_millis(250), move || {
        while let Ok(s) = receiver.try_recv() {
            ui2.update(&s);
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
