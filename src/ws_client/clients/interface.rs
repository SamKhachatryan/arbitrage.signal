use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream};

use crate::define_prometheus_gauge;
use crate::{state::AppState, tls::make_tls_config, ws_server::WSServer};

define_prometheus_gauge!(
    ACTIVE_WS_CLIENTS_GAUGE,
    "active_ws_clients_gauge",
    "Active WS Clients Gauge"
);

pub struct WSClient<T: ExchangeWSSession + Send + Sync> {
    url: String,
    session: Arc<T>,
}

#[async_trait]
pub trait ExchangeWSClient<S: ExchangeWSSession + Send + Sync> {
    fn new(session: S, url: String) -> Self;

    async fn subscribe(
        &self,
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair: String,
    );
}

#[async_trait]
pub trait ExchangeWSSession: Send + Sync {
    async fn handle_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair: String,
    );
}

#[async_trait]
impl<S> ExchangeWSClient<S> for WSClient<S>
where
    S: ExchangeWSSession + Send + Sync + 'static,
{
    fn new(session: S, url: String) -> Self {
        Self {
            session: Arc::new(session),
            url,
        }
    }

    async fn subscribe(
        &self,
        state: Arc<Mutex<AppState>>,
        server: Arc<Option<WSServer>>,
        pair: String,
    ) {
        let url = self.url.clone();
        let state = Arc::clone(&state);
        let server = Arc::clone(&server);
        let pair = pair.clone();
        let session = Arc::clone(&self.session);

        tokio::spawn(async move {
            // let mut retry_count = 0u32;

            loop {
                let tls_config = make_tls_config();
                let connector = Connector::Rustls(tls_config);

                match connect_async_tls_with_config(&url, None, false, Some(connector)).await {
                    Ok((ws_stream, _resp)) => {
                        // eprintln!("Successfully connected to {url}");
                        // retry_count = 0;
                        
                        ACTIVE_WS_CLIENTS_GAUGE.inc();

                        session
                            .handle_session(
                                ws_stream,
                                Arc::clone(&state),
                                Arc::clone(&server),
                                pair.clone(),
                            )
                            .await;

                        ACTIVE_WS_CLIENTS_GAUGE.dec();
                        // eprintln!("Session ended for {url}, reconnecting...");
                    }
                    Err(e) => {
                        // retry_count += 1;
                        // eprintln!("Failed to connect to {url} (attempt #{retry_count}): {e}");
                        
                        if e.to_string().contains("10053") {
                            eprintln!("  → This is a Windows connection abort error. Usually caused by:");
                            eprintln!("     - TLS handshake failure");
                            eprintln!("     - Invalid certificate chain");
                            eprintln!("     - Firewall/antivirus blocking connection");
                        }
                    }
                }

                sleep(Duration::from_secs(2)).await;
            }
        });
    }
}
