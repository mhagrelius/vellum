use gtk::prelude::*;
use vellum::ui::Application;

fn main() -> gtk::glib::ExitCode {
    gtk::glib::set_application_name("Vellum");
    gtk::glib::set_prgname(Some(vellum::APP_ID));
    Application::new().run()
}
