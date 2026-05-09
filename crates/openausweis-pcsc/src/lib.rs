use async_trait::async_trait;
use openausweis_core::{CardReaderSnapshot, CardSubsystem, CardSubsystemSnapshot};
use pcsc::{Context, Protocols, Scope, ShareMode};

pub struct PcscSubsystem;

#[async_trait]
impl CardSubsystem for PcscSubsystem {
    async fn snapshot(&self) -> CardSubsystemSnapshot {
        let context = match Context::establish(Scope::User) {
            Ok(context) => context,
            Err(err) => {
                return CardSubsystemSnapshot {
                    pcsc_available: false,
                    readers: Vec::new(),
                    diagnostics: vec![format!("PC/SC context establishment failed: {err}")],
                    last_error: Some(err.to_string()),
                };
            }
        };

        let mut buffer = [0; 2048];
        let readers = match context.list_readers(&mut buffer) {
            Ok(readers) => readers,
            Err(err) => {
                return CardSubsystemSnapshot {
                    pcsc_available: true,
                    readers: Vec::new(),
                    diagnostics: vec![format!("Failed to enumerate PC/SC readers: {err}")],
                    last_error: Some(err.to_string()),
                };
            }
        };

        let mut snapshots = Vec::new();
        let mut diagnostics = Vec::new();

        for reader in readers {
            let reader_name = reader.to_string_lossy().to_string();
            let connection_result = context.connect(reader, ShareMode::Shared, Protocols::ANY);

            match connection_result {
                Ok(_) => snapshots.push(CardReaderSnapshot {
                    name: reader_name,
                    card_present: true,
                    error: None,
                }),
                Err(err) => {
                    let err_text = err.to_string();
                    let normalized = err_text.to_ascii_lowercase();
                    let is_no_card = normalized.contains("no smart card")
                        || normalized.contains("card is not present")
                        || normalized.contains("removed card");

                    if is_no_card {
                        snapshots.push(CardReaderSnapshot {
                            name: reader_name,
                            card_present: false,
                            error: None,
                        });
                    } else {
                        diagnostics.push(format!(
                            "Reader {reader_name} status probe failed: {err_text}"
                        ));
                        snapshots.push(CardReaderSnapshot {
                            name: reader_name,
                            card_present: false,
                            error: Some(err_text),
                        });
                    }
                }
            }
        }

        let last_error = diagnostics.last().cloned();
        CardSubsystemSnapshot {
            pcsc_available: true,
            readers: snapshots,
            diagnostics,
            last_error,
        }
    }
}
