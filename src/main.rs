mod editor;
mod game;
mod web;

use axum::{
    Router,
    routing::{get, post},
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let address = std::env::var("RALLY_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let listener = tokio::net::TcpListener::bind(&address).await?;
    let app = Router::new()
        .route("/", get(web::home))
        .route("/style.css", get(web::stylesheet))
        .route("/editor", get(editor::open).post(editor::edit))
        .route("/host", post(web::create))
        .route("/join", post(web::join))
        .route("/room/{code}", get(web::room))
        .route("/room/{code}/advance", post(web::advance))
        .route("/room/{code}/answer", post(web::answer))
        .with_state(game::Rooms::default());
    println!("Rally Quiz listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await
}
