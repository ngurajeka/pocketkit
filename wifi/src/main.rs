use wifi::{app::App, backend::Backend, worker};

/// Desktop preview: simulated networks ("password123" is the right password).
#[cfg(feature = "desktop")]
fn backend() -> Box<dyn Backend> {
    Box::new(wifi::backend::Fake::new(std::time::Duration::from_millis(600)))
}

#[cfg(not(feature = "desktop"))]
fn backend() -> Box<dyn Backend> {
    let conf = std::env::var_os("WIFI_CONF").unwrap_or_else(|| "/appconfigs/wpa_supplicant.conf".into());
    let pak_dir = std::env::current_dir().unwrap_or_default();
    Box::new(wifi::device::Device::new(conf.into(), pak_dir))
}

fn main() {
    let (tx, rx) = worker::spawn(backend());
    let mut app = App::new(tx, rx);
    minui_common::run("Wi-Fi", &mut app);
}
