use zbus::dbus_proxy;

#[dbus_proxy(
    interface = "org.kde.kdeconnect.device.connectivity_report",
    default_service = "org.kde.kdeconnect"
)]
trait ConnectivityDbus {
    #[dbus_proxy(signal, name = "refreshed")]
    fn refreshed(&self, network_type: String, network_strength: i32) -> zbus::Result<()>;

    #[dbus_proxy(property, name = "cellularNetworkStrength")]
    fn cellular_network_strength(&self) -> zbus::Result<i32>;

    #[dbus_proxy(property, name = "cellularNetworkType")]
    fn cellular_network_type(&self) -> zbus::Result<String>;
}
