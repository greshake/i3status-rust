use super::*;

pub(super) struct KbddBus {
    stream: layoutNameChangedStream<'static>,
    info: Info,
}

impl KbddBus {
    pub(super) async fn new() -> Result<Self> {
        let conn = new_dbus_connection().await?;
        let proxy = KbddBusInterfaceProxy::builder(&conn)
            .cache_properties(zbus::CacheProperties::No)
            .build()
            .await
            .error("Failed to create KbddBusInterfaceProxy")?;
        let stream = proxy
            .receive_layout_updated()
            .await
            .error("Failed to monitor kbdd interface")?;
        let layout_index = proxy
            .current_layout_index()
            .await
            .error("Failed to get current layout index from kbdd")?;
        let current_layout = proxy
            .current_layout(layout_index)
            .await
            .error("Failed to get current layout from kbdd")?;
        let info = Info::from_layout_variant_str(&current_layout);
        Ok(Self { stream, info })
    }
}

#[async_trait]
impl Backend for KbddBus {
    async fn get_info(&mut self) -> Result<Info> {
        Ok(self.info.clone())
    }

    async fn wait_for_change(&mut self) -> Result<()> {
        let event = self
            .stream
            .next()
            .await
            .error("Failed to receive kbdd event from dbus")?;
        let args = event
            .args()
            .error("Failed to get the args from kbdd message")?;
        self.info = Info::from_layout_variant_str(args.layout());
        Ok(())
    }
}

#[zbus::proxy(
    interface = "ru.gentoo.kbdd",
    default_service = "ru.gentoo.KbddService",
    default_path = "/ru/gentoo/KbddService"
)]
trait KbddBusInterface {
    #[zbus(signal, name = "layoutNameChanged")]
    fn layout_updated(&self, layout: String) -> zbus::Result<()>;

    #[zbus(name = "getCurrentLayout")]
    fn current_layout_index(&self) -> zbus::Result<u32>;

    #[zbus(name = "getLayoutName")]
    fn current_layout(&self, layout_id: u32) -> zbus::Result<String>;
}
