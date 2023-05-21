use zbus::dbus_proxy;

#[dbus_proxy(
    interface = "org.kde.kdeconnect.device.battery",
    default_service = "org.kde.kdeconnect"
)]
trait BatteryDbus {
    #[dbus_proxy(signal, name = "refreshed")]
    fn refreshed(&self, is_charging: bool, charge: i32) -> zbus::Result<()>;

    #[dbus_proxy(property, name = "charge")]
    fn charge(&self) -> zbus::Result<i32>;

    #[dbus_proxy(property, name = "isCharging")]
    fn is_charging(&self) -> zbus::Result<bool>;
}
