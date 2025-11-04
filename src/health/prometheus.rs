use actix_cors::Cors;
use actix_web::{App, HttpResponse, HttpServer, get};
use prometheus::{Encoder, Registry, TextEncoder};

use lazy_static::lazy_static;

lazy_static! {
    pub static ref METRIC_REGISTRY: Registry = Registry::new();
}

#[get("/health-metrics")]
async fn metrics() -> impl actix_web::Responder {
    let encoder = TextEncoder::new();
    let metric_families = METRIC_REGISTRY.gather();
    let mut buffer = Vec::new();

    if let Err(e) = encoder.encode(&metric_families, &mut buffer) {
        return HttpResponse::InternalServerError().body(format!("Encoding error: {}", e));
    }

    match String::from_utf8(buffer) {
        Ok(body) => HttpResponse::Ok()
            .content_type(encoder.format_type())
            .body(body),
        Err(e) => HttpResponse::InternalServerError().body(format!("UTF-8 error: {}", e)),
    }
}

pub async fn init_prometheus_server() -> std::io::Result<()> {
    HttpServer::new(|| {
        let cors = Cors::permissive();
        App::new().wrap(cors).service(metrics)
    })
    .bind(("127.0.0.1", 4011))? // Expose metrics on port 9001
    .run()
    .await
}
