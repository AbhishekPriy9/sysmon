use libadwaita as adw;

use gtk4::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("dev.sysmon.Sysmon")
        .build();

    app.connect_activate(|app| {
        let window = sysmon::ui::build(app);
        window.present();
    });

    app.run();
}
