use crate::cli_opts::XgmLedSubCommand;
use rog_dbus::zbus_xgm_led::XgmLedProxyBlocking;

pub fn handle_xgm_led(cmd: &XgmLedSubCommand) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = XgmLedProxyBlocking::new(&zbus::blocking::Connection::system()?)
        .map_err(|e| format!("Failed to connect to XG Mobile LED interface: {e}"))?;

    match cmd {
        XgmLedSubCommand::Get(_) => {
            let enabled = proxy.xgm_led_enabled()?;
            println!("XG Mobile LED: {}", if enabled { "ON" } else { "OFF" });
        }
        XgmLedSubCommand::Set(cmd) => {
            let enabled = cmd.value != 0;
            proxy.set_xgm_led_enabled(enabled)?;
            println!(
                "XG Mobile LED set to {}",
                if enabled { "ON" } else { "OFF" }
            );
        }
    }

    Ok(())
}
