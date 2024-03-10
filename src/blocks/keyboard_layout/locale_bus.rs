use super::*;

pub(super) struct LocaleBus {
    proxy: LocaleBusInterfaceProxy<'static>,
    stream1: zbus::PropertyStream<'static, String>,
    stream2: zbus::PropertyStream<'static, String>,
}

impl LocaleBus {
    pub(super) async fn new() -> Result<Self> {
        let conn = new_system_dbus_connection().await?;
        let proxy = LocaleBusInterfaceProxy::new(&conn)
            .await
            .error("Failed to create LocaleBusProxy")?;
        let layout_updates = proxy.receive_layout_changed().await;
        let variant_updates = proxy.receive_layout_changed().await;
        Ok(Self {
            proxy,
            stream1: layout_updates,
            stream2: variant_updates,
        })
    }
}

#[async_trait]
impl Backend for LocaleBus {
    async fn get_info(&mut self) -> Result<Info> {
        // zbus does internal caching
        let layout = self.proxy.layout().await.error("Failed to get layout")?;
        let variant = self.proxy.variant().await.error("Failed to get variant")?;
        Ok(Info {
            layout,
            variant: Some(variant),
        })
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        select! {
            _ = self.stream1.next() => (),
            _ = self.stream2.next() => (),
        }
        Ok(())
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.locale1",
    default_service = "org.freedesktop.locale1",
    default_path = "/org/freedesktop/locale1"
)]
trait LocaleBusInterface {
    #[zbus(property, name = "X11Layout")]
    fn layout(&self) -> zbus::Result<String>;

    #[zbus(property, name = "X11Variant")]
    fn variant(&self) -> zbus::Result<String>;
}
