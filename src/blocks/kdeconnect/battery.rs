#[zbus::proxy(
    interface = "org.kde.kdeconnect.device.battery",
    default_service = "org.kde.kdeconnect"
)]
trait BatteryDbus {
    #[zbus(signal, name = "refreshed")]
    fn refreshed(&self, is_charging: bool, charge: i32) -> zbus::Result<()>;

    #[zbus(property, name = "charge")]
    fn charge(&self) -> zbus::Result<i32>;

    #[zbus(property, name = "isCharging")]
    fn is_charging(&self) -> zbus::Result<bool>;
}
