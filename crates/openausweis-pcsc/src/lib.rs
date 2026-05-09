use async_trait::async_trait;
use openausweis_core::CardSubsystem;

pub struct PcscSubsystem;

#[async_trait]
impl CardSubsystem for PcscSubsystem {
    async fn is_pcsc_available(&self) -> bool {
        // TODO: integrate pcsc-lite probing and reader event subscriptions.
        false
    }
}
