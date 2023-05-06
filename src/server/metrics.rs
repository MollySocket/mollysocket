use std::fmt::Display;

use eyre::Result;
use rocket::{http::uri::Origin, Build, Rocket};
use rocket_prometheus::{
    prometheus::{
        register_histogram, register_int_counter, register_int_gauge, Histogram, IntCounter,
        IntGauge,
    },
    PrometheusMetrics,
};

pub struct Metrics {
    pub connections: IntGauge,
    pub forbiddens: IntGauge,
    pub reconnections: IntCounter,
    pub messages: IntCounter,
    pub pushs: IntCounter,
    pub previous_msg: Histogram,
}

impl Metrics {
    pub fn new() -> Result<Self> {
        let connections =
            register_int_gauge!("mollysocket_connections", "Connections to Signal server")?;
        let forbiddens = register_int_gauge!(
            "mollysocket_forbiddens",
            "Forbidden connections to Signal server"
        )?;
        let reconnections =
            register_int_counter!("mollysocket_reconnections", "Reconnections since the start")?;
        let messages =
            register_int_counter!("mollysocket_messages", "Messages received from Signal")?;
        let pushs = register_int_counter!(
            "mollysocket_pushs",
            "Push messages sent to UnifiedPush endpoint"
        )?;
        let previous_msg = register_histogram!(
            "mollysocket_previous_msg",
            "Time since last message",
            vec![
                1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
                16.0, 17.0, 18.0, 19.0, 20.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0,
                30.0, 40.0, 50.0, 60.0
            ]
        )?;

        Ok(Self {
            connections,
            forbiddens,
            reconnections,
            messages,
            pushs,
            previous_msg,
        })
    }
}

pub trait MountMetrics {
    fn mount_metrics<'a, B>(self, base: B, metrics: &Metrics) -> Self
    where
        B: TryInto<Origin<'a>> + Clone + Display,
        B::Error: Display;
}

impl MountMetrics for Rocket<Build> {
    fn mount_metrics<'a, B>(self, base: B, metrics: &Metrics) -> Self
    where
        B: TryInto<Origin<'a>> + Clone + Display,
        B::Error: Display,
    {
        let prometheus = PrometheusMetrics::new();
        let prom_registry = prometheus.registry();
        prom_registry
            .register(Box::new(metrics.connections.clone()))
            .unwrap();
        prom_registry
            .register(Box::new(metrics.forbiddens.clone()))
            .unwrap();
        prom_registry
            .register(Box::new(metrics.reconnections.clone()))
            .unwrap();
        prom_registry
            .register(Box::new(metrics.messages.clone()))
            .unwrap();
        prom_registry
            .register(Box::new(metrics.pushs.clone()))
            .unwrap();
        prom_registry
            .register(Box::new(metrics.previous_msg.clone()))
            .unwrap();

        self.attach(prometheus.clone()).mount(base, prometheus)
    }
}
