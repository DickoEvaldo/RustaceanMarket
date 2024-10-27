use actix_web::{
    web::{self, service},
    App, HttpServer,
};
use actix_web_httpauth::middleware::HttpAuthentication;
use sqlx::{postgres::PgPoolOptions, PgPool};
mod api;

// api user
use api::{
    carts::{add_cart_item, get_cart},
    orders::{create_order, get_all_orders, get_all_user_orders, update_order_status},
    products::{
        create_product, delete_product_id, get_product_by_id, get_products, update_product_by_id,
    },
    users::{auth, create_user, get_user, get_user_by_id, validator},
};

struct AppState {
    db: PgPool,
}

#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    let port = 8080;
    dotenv::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to create pool");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migration failed");

    println!("the server is running on port {port}");

    HttpServer::new(move || {
        let bearer_middleware = HttpAuthentication::bearer(validator);
        App::new()
            .app_data(web::Data::new(AppState { db: pool.clone() }))
            .service(get_user)
            .service(get_user_by_id)
            .service(create_user)
            .service(auth)
            .service(
                web::scope("")
                    .wrap(bearer_middleware)
                    .service(get_products)
                    .service(get_product_by_id)
                    .service(create_product)
                    .service(delete_product_id)
                    .service(update_product_by_id)
                    .service(get_cart)
                    .service(add_cart_item)
                    .service(get_all_user_orders)
                    .service(create_order)
                    .service(get_all_orders)
                    .service(update_order_status),
            )
    })
    .bind(("localhost", port))?
    .workers(2)
    .run()
    .await
}
