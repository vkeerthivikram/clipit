mod app;
mod clipboard;
mod history;
mod i18n;

fn main() -> cosmic::iced::Result {
    i18n::localize();
    if std::env::args().any(|arg| arg == "--toggle") {
        toggle_popup();
        return Ok(());
    }
    cosmic::applet::run::<app::App>(())
}

/// Asks the running panel applet to open or close its popup. Used together
/// with a global keyboard shortcut bound to `clipit --toggle`.
fn toggle_popup() {
    let result = zbus::blocking::Connection::session().and_then(|connection| {
        zbus::blocking::Proxy::new(
            &connection,
            "dev.clipit.Clipit",
            "/dev/clipit/Clipit",
            "dev.clipit.Clipit",
        )
        .and_then(|proxy| proxy.call::<_, _, ()>("Toggle", &()))
    });
    if let Err(why) = result {
        eprintln!("clipit: cannot reach the running applet: {why}");
        std::process::exit(1);
    }
}
