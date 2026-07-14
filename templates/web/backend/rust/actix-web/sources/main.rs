mod config;
mod routes;
mod services;

use actix_web::{web, App, HttpServer};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let cfg = config::Config::load();
    let data = web::Data::new(cfg.clone());
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .configure(routes::health::configure)
    })
    .bind(format!("0.0.0.0:{}", cfg.port))?
    .run()
    .await
}
