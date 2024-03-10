#[zbus::proxy(
    interface = "org.kde.kdeconnect.device.connectivity_report",
    default_service = "org.kde.kdeconnect"
)]
trait ConnectivityDbus {
    #[zbus(signal, name = "refreshed")]
    fn refreshed(&self, network_type: String, network_strength: i32) -> zbus::Result<()>;

    #[zbus(property, name = "cellularNetworkStrength")]
    fn cellular_network_strength(&self) -> zbus::Result<i32>;

    #[zbus(property, name = "cellularNetworkType")]
    fn cellular_network_type(&self) -> zbus::Result<String>;
}
