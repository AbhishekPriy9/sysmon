use libadwaita as adw;

use gtk4::prelude::*;

fn main() {
    let app = adw::Application::builder()
        .application_id("dev.sysmon.Sysmon")
        .build();

    app.connect_activate(|app| {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("sysmon")
            .default_width(560)
            .default_height(820)
            .build();
        window.present();
    });

    app.run();
}
