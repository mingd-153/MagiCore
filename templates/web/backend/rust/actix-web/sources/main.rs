mod config;
mod middleware;
mod routes;
mod services;

use actix_cors::Cors;
use actix_web::{web, App, HttpServer, middleware as aw_mw};
use tracing_subscriber::EnvFilter;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cfg = config::Config::load();
    let data = web::Data::new(cfg.clone());

    let server = HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .wrap(aw_mw::from_fn(middleware::request_id))
            .wrap(aw_mw::from_fn(middleware::logger))
            .app_data(data.clone())
            .configure(routes::health::configure)
    })
    .bind(format!("0.0.0.0:{}", cfg.port))?
    .run();

    let handle = server.handle();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.unwrap();
        handle.stop(true).await;
    });

    server.await
}
