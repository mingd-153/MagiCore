mod config;
mod routes;
mod services;

use actix_web::{web, App, HttpResponse, HttpServer};
use config::Config;
use serde_json::json;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = Config::load();
    println!("Starting {} (actix-web) on 0.0.0.0:{}", cfg.name, cfg.port);

    let addr = format!("0.0.0.0:{}", cfg.port);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(cfg.clone()))
            .route("/health", web::get().to(routes::health::health_handler))
            .route("/", web::get().to(root_handler))
    })
    .bind(&addr)?
    .run()
    .await
}

async fn root_handler(cfg: web::Data<Config>) -> HttpResponse {
    HttpResponse::Ok().json(json!({
        "service": cfg.name,
        "framework": cfg.framework,
        "message": "{{project_name}} backend scaffold ready"
    }))
}
