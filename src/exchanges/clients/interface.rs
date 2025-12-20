use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async_tls_with_config, Connector, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::define_prometheus_gauge;
use crate::{state::AppState, tls::make_tls_config, ws_server::WSServer};

define_prometheus_gauge!(
    ACTIVE_WS_CLIENTS_GAUGE,
    "ACTIVE_WS_CLIENTS_GAUGE",
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
        server: Arc<WSServer>,
        pairs: Vec<String>,
        cancel_token: CancellationToken,
    );
}

#[async_trait]
pub trait ExchangeWSSession: Send + Sync {
    async fn handle_ws_session(
        &self,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        state: Arc<Mutex<AppState>>,
        server: Arc<WSServer>,
        pairs: Vec<String>,
        cancel_token: CancellationToken,
    );
}

#[async_trait]
pub trait ExchangeResyncOrderbook: Send + Sync {
    async fn resync_orderbook_loop(
        &self,
        state: Arc<Mutex<AppState>>,
        server: Arc<WSServer>,
        pair_name: String,
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
        server: Arc<WSServer>,
        pairs: Vec<String>,
        cancel_token: CancellationToken,
    ) {
        let url = self.url.clone();
        let state = Arc::clone(&state);
        let server = Arc::clone(&server);
        let session = Arc::clone(&self.session);

        tokio::spawn(async move {
            loop {
                // Check if cancelled before attempting connection
                if cancel_token.is_cancelled() {
                    println!("Subscription cancelled for {}", url);
                    break;
                }

                let tls_config = make_tls_config();
                let connector = Connector::Rustls(tls_config);

                match connect_async_tls_with_config(&url, None, false, Some(connector)).await {
                    Ok((ws_stream, _resp)) => {
                        ACTIVE_WS_CLIENTS_GAUGE.inc();

                        session
                            .handle_ws_session(
                                ws_stream,
                                Arc::clone(&state),
                                Arc::clone(&server),
                                pairs.clone(),
                                cancel_token.clone(),
                            )
                            .await;

                        ACTIVE_WS_CLIENTS_GAUGE.dec();
                        
                        // Check if cancelled before reconnecting
                        if cancel_token.is_cancelled() {
                            println!("Subscription cancelled for {}, not reconnecting", url);
                            break;
                        }
                    }
                    Err(e) => {
                        
                        if e.to_string().contains("10053") {
                            eprintln!("  → This is a Windows connection abort error. Usually caused by:");
                            eprintln!("     - TLS handshake failure");
                            eprintln!("     - Invalid certificate chain");
                            eprintln!("     - Firewall/antivirus blocking connection");
                        }
                        
                        // Check if cancelled before sleeping
                        if cancel_token.is_cancelled() {
                            println!("Subscription cancelled for {} during error handling", url);
                            break;
                        }
                    }
                }

                // Use tokio::select to make sleep cancellable
                tokio::select! {
                    _ = sleep(Duration::from_secs(2)) => {}
                    _ = cancel_token.cancelled() => {
                        println!("Subscription cancelled for {} during sleep", url);
                        break;
                    }
                }
            }
        });
    }
}