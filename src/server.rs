use crate::db::MollySocketDb;
use futures_util::{future::join, pin_mut, select, FutureExt};
use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};
use tokio::signal;
use rocket_prometheus::{
    prometheus::{
        register_int_gauge_vec,
        IntGaugeVec
    },
};

mod connections;
mod web;

lazy_static! {
    static ref REFS: Arc<Mutex<Vec<connections::LoopRef>>> = Arc::new(Mutex::new(vec![]));
    static ref DB: MollySocketDb = MollySocketDb::new().unwrap();
    static ref TX: Arc<Mutex<connections::OptSender>> = Arc::new(Mutex::new(None));

    static ref METRIC_MOLLYSOCKET_UP: IntGaugeVec =
         register_int_gauge_vec!("mollysocket_up", "Is Mollysocket ready", &["version"]).unwrap();
    static ref METRIC_MOLLYSOCKET_SIGNAL_CONNECTED: IntGaugeVec =
         register_int_gauge_vec!("mollysocket_signal_connected", "Connected to signal", &["type","uuid"]).unwrap();
}

pub async fn run() {
    let signal_future = signal::ctrl_c().fuse();
    let joined_future = join(web::launch().fuse(), connections::run().fuse());

    pin_mut!(signal_future, joined_future);

    select!(
        _ = signal_future => log::info!("SIGINT received"),
        _ = joined_future => log::warn!("Server stopped"),
    )
}
