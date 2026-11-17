use wifi::{app::App, backend::Backend, worker};

/// Desktop preview: simulated networks ("password123" is the right password).
#[cfg(feature = "desktop")]
fn backend() -> Box<dyn Backend> {
    Box::new(wifi::backend::Fake::new(std::time::Duration::from_millis(600)))
}

#[cfg(not(feature = "desktop"))]
fn backend() -> Box<dyn Backend> {
    let conf = std::env::var_os("WIFI_CONF").unwrap_or_else(|| "/appconfigs/wpa_supplicant.conf".into());
    // The pak folder (for 8188fu.ko and the config backup), also when started
    // from elsewhere, e.g. `wifi --up` from auto.sh at boot.
    let pak_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    Box::new(wifi::device::Device::new(conf.into(), pak_dir))
}

fn main() {
    // `wifi --up`: turn Wi-Fi on with the saved networks and exit, no screen.
    // Settings.pak puts this in auto.sh for "Wi-Fi at startup".
    if std::env::args().any(|a| a == "--up") {
        match backend().power_on() {
            Ok(()) => eprintln!("wifi --up: on"),
            Err(e) => {
                eprintln!("wifi --up: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    let (tx, rx) = worker::spawn(backend());
    let mut app = App::new(tx, rx);
    minui_common::run("Wi-Fi", &mut app);
}
