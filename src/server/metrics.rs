
use crate::CONFIG;

use lazy_static::lazy_static;
use rocket::{
    Rocket,
    Build,
};

use rocket_prometheus::{
    PrometheusMetrics,
    prometheus::{
        register_int_gauge_vec,
        IntGaugeVec,
        register_int_counter_vec,
        IntCounterVec,
    },
};

lazy_static! {
    pub static ref METRIC_MOLLYSOCKET_UP: IntGaugeVec =
         register_int_gauge_vec!("mollysocket_up", "Is Mollysocket ready", &["version"]).unwrap();

    pub static ref METRIC_MOLLYSOCKET_SIGNAL_CONNECTED: IntGaugeVec =
         register_int_gauge_vec!("mollysocket_signal_connected", "Connected to signal", &["type","uuid","push_endpoint"]).unwrap();
    pub static ref METRIC_MOLLYSOCKET_SIGNAL_RECONNECTED: IntCounterVec =
         register_int_counter_vec!("mollysocket_signal_reconnected", "reconnected to signal", &["type","uuid","push_endpoint"]).unwrap();

    pub static ref METRIC_MOLLYSOCKET_PUSH: IntCounterVec =
         register_int_counter_vec!("mollysocket_push", "send (unified) push message", &["type","uuid","push_endpoint"]).unwrap();
}

pub fn rocket(server: Rocket<Build>) -> Rocket<Build> {
    let prometheus = PrometheusMetrics::new();
    let prom_registry = prometheus.registry();
    prom_registry
        .register(Box::new(METRIC_MOLLYSOCKET_UP.clone()))
        .unwrap();
    prom_registry
        .register(Box::new(METRIC_MOLLYSOCKET_SIGNAL_CONNECTED.clone()))
        .unwrap();
    prom_registry
        .register(Box::new(METRIC_MOLLYSOCKET_SIGNAL_RECONNECTED.clone()))
        .unwrap();
    prom_registry
        .register(Box::new(METRIC_MOLLYSOCKET_PUSH.clone()))
        .unwrap();

    // set metric values (should be an rocket guard later if multiple metrics are there)
    METRIC_MOLLYSOCKET_UP.with_label_values(&[CONFIG.version.as_str()]).set(1);

    server.attach(prometheus.clone())
        .mount("/metrics", prometheus)
}
