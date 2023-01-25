use async_trait::async_trait;
use eyre::Result;
use futures_channel::mpsc;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::time;
use tokio_tungstenite::tungstenite;
use rocket_prometheus::prometheus::IntCounter;

use super::tls;
use super::websocket_connection::WebSocketConnection;
use super::websocket_message::{
    webSocketMessage::Type, WebSocketMessage, WebSocketRequestMessage, WebSocketResponseMessage,
};
use crate::{
    db::Strategy,
    utils::post_allowed::post_allowed,
};

const PUSH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct SignalWebSocket {
    connect_addr: url::Url,
    push_endpoint: url::Url,
    strategy: Strategy,
    metric_push: Option<IntCounter>,
    tx: Option<mpsc::UnboundedSender<tungstenite::Message>>,
    push_instant: Arc<Mutex<Instant>>,
    last_keepalive: Arc<Mutex<Instant>>,
}

#[async_trait(?Send)]
impl WebSocketConnection for SignalWebSocket {
    fn get_url(&self) -> &url::Url {
        &self.connect_addr
    }

    fn get_tx(&self) -> &Option<mpsc::UnboundedSender<tungstenite::Message>> {
        &self.tx
    }

    fn set_tx(&mut self, tx: Option<mpsc::UnboundedSender<tungstenite::Message>>) {
        self.tx = tx;
    }

    fn get_last_keepalive(&self) -> Arc<Mutex<Instant>> {
        Arc::clone(&self.last_keepalive)
    }

    async fn on_message(&self, message: WebSocketMessage) {
        if let Some(type_int) = message.r#type {
            if let Some(type_) = Type::from_i32(type_int) {
                match type_ {
                    Type::RESPONSE => self.on_response(message.response),
                    Type::REQUEST => self.on_request(message.request).await,
                    _ => (),
                };
            }
        }
    }
}

impl SignalWebSocket {
    pub fn new(connect_addr: String, push_endpoint: String, strategy: Strategy, metric_push: Option<IntCounter>) -> Result<Self> {
        let connect_addr = url::Url::parse(&connect_addr)?;
        let push_endpoint = url::Url::parse(&push_endpoint)?;
        Ok(Self {
            connect_addr,
            push_endpoint,
            strategy,
            metric_push,
            tx: None,
            push_instant: Arc::new(Mutex::new(
                Instant::now().checked_sub(PUSH_TIMEOUT).unwrap(),
            )),
            last_keepalive: Arc::new(Mutex::new(Instant::now())),
        })
    }

    pub async fn connection_loop(&mut self) -> Result<()> {
        let mut count = 0;
        loop {
            let instant = Instant::now();
            {
                let mut keepalive = self.last_keepalive.lock().unwrap();
                *keepalive = Instant::now();
            }
            self.connect(tls::build_tls_connector()?).await?;
            if let Some(duration) = Instant::now().checked_duration_since(instant) {
                if duration > Duration::from_secs(60) {
                    count = 0;
                }
            }
            count += 1;
            log::info!("Retrying to connect in {}0 secondes.", count);
            time::sleep(Duration::from_secs(count * 10)).await;
        }
    }

    fn on_response(&self, response: Option<WebSocketResponseMessage>) {
        log::debug!("New response");
        if let Some(_) = response {
            let mut keepalive = self.last_keepalive.lock().unwrap();
            *keepalive = Instant::now();
        }
    }

    /**
     * That's when we must send a notification
     */
    async fn on_request(&self, request: Option<WebSocketRequestMessage>) {
        log::debug!("New request");
        if let Some(request) = request {
            if self.read_or_empty(request).await {
                if self.waiting_timeout_reached() {
                    self.send_push().await;
                } else {
                    log::debug!("The waiting timeout is not reached: the request is ignored.");
                }
            }
        }
    }

    async fn read_or_empty(&self, request: WebSocketRequestMessage) -> bool {
        // dbg!(&request.path);
        let response = self.create_websocket_response(&request);
        // dbg!(&response);
        if self.is_signal_service_envelope(&request) {
            self.send_response(response).await;
            return true;
        }
        false
    }

    fn is_signal_service_envelope(
        &self,
        WebSocketRequestMessage {
            verb,
            path,
            body: _,
            headers: _,
            id: _,
        }: &WebSocketRequestMessage,
    ) -> bool {
        if let Some(verb) = verb {
            if let Some(path) = path {
                return verb.eq("PUT") && path.eq("/api/v1/message");
            }
        }
        false
    }

    fn create_websocket_response(
        &self,
        request: &WebSocketRequestMessage,
    ) -> WebSocketResponseMessage {
        if self.is_signal_service_envelope(request) {
            return WebSocketResponseMessage {
                id: request.id,
                status: Some(200),
                message: Some(String::from("OK")),
                headers: Vec::new(),
                body: None,
            };
        }
        WebSocketResponseMessage {
            id: request.id,
            status: Some(400),
            message: Some(String::from("Unknown")),
            headers: Vec::new(),
            body: None,
        }
    }

    async fn send_push(&self) {
        log::debug!("Sending the notification.");
        {
            let mut instant = self.push_instant.lock().unwrap();
            *instant = Instant::now();
        }

        let url = self.push_endpoint.clone();
        let _ = post_allowed(url, &[("type", "message")]).await;
        match &self.metric_push {
            Some(metric) => metric.inc(),
            None => {},
        }
    }

    fn waiting_timeout_reached(&self) -> bool {
        if let Strategy::Rest = self.strategy {
            return true;
        }
        let instant = self.push_instant.lock().unwrap();
        instant.elapsed() > PUSH_TIMEOUT
    }
}
