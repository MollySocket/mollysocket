use crate::{
    db::{Connection, OptTime, Strategy},
    CONFIG,
};
use eyre::Result;
use rocket::{
    get, post, routes,
    serde::{json::Json, Deserialize, Serialize},
};
use rocket_prometheus::PrometheusMetrics;

use std::{collections::HashMap, str::FromStr, time::SystemTime};

use super::{
    DB, TX,
    METRIC_MOLLYSOCKET_UP,
    METRIC_MOLLYSOCKET_SIGNAL_CONNECTED,
    METRIC_MOLLYSOCKET_SIGNAL_RECONNECTED,
};

#[derive(Serialize)]
struct Response {
    mollysocket: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct ConnectionData {
    pub uuid: String,
    pub device_id: u32,
    pub password: String,
    pub endpoint: String,
    pub strategy: String,
}

enum RegistrationStatus {
    New,
    Updated,
    Running,
    Forbidden,
    InvalidUuid,
    InvalidEndpoint,
    InternalError,
}

impl From<RegistrationStatus> for String {
    fn from(r: RegistrationStatus) -> Self {
        match r {
            RegistrationStatus::New | RegistrationStatus::Updated | RegistrationStatus::Running => {
                "ok"
            }
            RegistrationStatus::Forbidden => "forbidden",
            RegistrationStatus::InvalidUuid => "invalid_uuid",
            RegistrationStatus::InvalidEndpoint => "invalid_endpoint",
            RegistrationStatus::InternalError => "internal_error",
        }
        .into()
    }
}

#[get("/")]
fn discover() -> Json<Response> {
    gen_rep(HashMap::new())
}

#[post("/", format = "application/json", data = "<co_data>")]
async fn register(co_data: Json<ConnectionData>) -> Json<Response> {
    let mut status = registration_status(&co_data).await;
    match status {
        RegistrationStatus::Updated | RegistrationStatus::New => {
            if let Err(_) = new_connection(co_data) {
                status = RegistrationStatus::InternalError;
            }
        }
        RegistrationStatus::Forbidden => {
            if let Ok(co) = DB.get(&co_data.uuid) {
                if co.device_id != co_data.device_id || co.password != co_data.password {
                    if let Ok(_) = new_connection(co_data) {
                        status = RegistrationStatus::Updated;
                    } else {
                        status = RegistrationStatus::InternalError;
                    }
                }
            } else {
                status = RegistrationStatus::InternalError;
            }
        }
        //TODO: Update last registration for ::Running

        // If the connection is "Running" then the device creds still exists,
        // if the user register on another server or delete the linked device,
        // then the connection ends with a 403 Forbidden
        // If the connection is for an invalid uuid or an error occured : we ignore it
        _ => {}
    }
    gen_rep(HashMap::from([(
        String::from("status"),
        String::from(status),
    )]))
}

fn new_connection(co_data: Json<ConnectionData>) -> Result<()> {
    let co = Connection {
        uuid: co_data.uuid.clone(),
        device_id: co_data.device_id,
        password: co_data.password.clone(),
        endpoint: co_data.endpoint.clone(),
        strategy: Strategy::from_str(co_data.strategy.as_str())?,
        forbidden: false,
        last_registration: OptTime::from(SystemTime::now()),
    };
    DB.add(&co)?;
    if let Some(tx) = &*TX.lock().unwrap() {
        let _ = tx.unbounded_send(co);
    }
    Ok(())
}

async fn registration_status(co_data: &ConnectionData) -> RegistrationStatus {
    let endpoint_valid = CONFIG.is_endpoint_valid(&co_data.endpoint).await;
    let uuid_valid = CONFIG.is_uuid_valid(&co_data.uuid);

    if !uuid_valid {
        return RegistrationStatus::InvalidUuid;
    }

    if !endpoint_valid {
        return RegistrationStatus::InvalidEndpoint;
    }

    let co = match DB.get(&co_data.uuid) {
        Ok(co) => co,
        Err(_) => {
            return RegistrationStatus::New;
        }
    };

    if co.device_id == co_data.device_id && co.password == co_data.password {
        // Credentials are not updated
        if co.forbidden {
            return RegistrationStatus::Forbidden;
        } else if co.endpoint != co_data.endpoint {
            return RegistrationStatus::Updated;
        } else {
            return RegistrationStatus::Running;
        }
    } else {
        return RegistrationStatus::Updated;
    }
}

fn gen_rep(mut map: HashMap<String, String>) -> Json<Response> {
    map.insert(String::from("version"), CONFIG.version.clone());
    Json(Response { mollysocket: map })
}


pub async fn launch() {
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
    
    // set metric values (should be an rocket guard later if multiple metrics are there)
    METRIC_MOLLYSOCKET_UP.with_label_values(&[CONFIG.version.as_str()]).set(1);
    
    let _ = rocket::build()
        .attach(prometheus.clone())
        .mount("/metrics", prometheus)
        .mount("/", routes![discover, register])
        .launch()
        .await;
}
