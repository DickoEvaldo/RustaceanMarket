use crate::{api::users::TokenClaims, AppState};
use actix_web::{
    delete, get, post, put,
    web::{self, Json, ReqData},
    HttpMessage, HttpResponse, Responder,
};
use chrono::{DateTime, Utc};
use serde::{de::Error, Deserialize, Serialize};
use sqlx::{types::Decimal, FromRow, PgPool};
use uuid::Uuid;

use super::carts::Cart;

#[derive(Debug, Serialize, Deserialize, sqlx::Type, Clone)]
#[sqlx(type_name = "order_status", rename_all = "lowercase")]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Shipped,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct Order {
    order_id: Uuid,
    user_id: Uuid,
    order_date: DateTime<Utc>,
    status: OrderStatus,
    shipping_address: String,
    created_at: DateTime<Utc>,
    total_amount: Decimal,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct UpdateBody {
    order_status: String,
    order_id: Uuid,
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
struct OrderBody {
    shipping_address: String,
}

impl Order {
    // Retrieve all orders from current_user
    async fn get_all_user_orders(pool: &PgPool, user_id: Uuid) -> Result<Vec<Order>, sqlx::Error> {
        sqlx::query_as!(
            Order,
            r#"SELECT order_id, user_id, order_date, status as "status!: OrderStatus", shipping_address, created_at, total_amount FROM orders WHERE user_id = $1 ORDER BY created_at DESC"#
        , user_id)
        .fetch_all(pool)
        .await
    }

    // admin
    // Retrieve all orders from the database
    async fn get_all_orders(pool: &PgPool) -> Result<Vec<Order>, sqlx::Error> {
        sqlx::query_as!(
                Order,
                r#"SELECT order_id, user_id, order_date, status as "status!: OrderStatus", shipping_address, created_at, total_amount FROM orders ORDER BY created_at DESC"#)
            .fetch_all(pool)
            .await
    }

    // admin
    // update order status
    async fn update_order_status(
        pool: &PgPool,
        order_id: Uuid,
        order_status: String,
    ) -> Result<(), sqlx::Error> {
        let order = sqlx::query!("SELECT order_id FROM orders WHERE order_id = $1", order_id)
            .fetch_optional(pool)
            .await?;
        if order.is_none() {
            return Err(sqlx::Error::RowNotFound);
        }

        let order_status = match order_status.as_str() {
            "Pending" => OrderStatus::Pending,
            "Confirmed" => OrderStatus::Confirmed,
            "Shipped" => OrderStatus::Shipped,
            _ => OrderStatus::Pending,
        };

        sqlx::query!(
            "UPDATE orders SET status = $1 WHERE order_id = $2",
            order_status as OrderStatus,
            order_id
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    // Create order
    async fn create_order(
        pool: &PgPool,
        shipping_address: String, // Fixed spelling
        user_id: Uuid,
    ) -> Result<Order, sqlx::Error> {
        let mut tx = pool.begin().await?;

        // Check if cart exists and has items
        let cart = sqlx::query_as!(Cart, "SELECT * FROM carts WHERE user_id = $1", user_id)
            .fetch_optional(&mut *tx)
            .await?;

        let cart = cart.ok_or(sqlx::Error::RowNotFound)?;

        // Get cart items
        let cart_items = sqlx::query!(
            r#"SELECT ci.*, p.price 
            FROM cart_items ci 
            JOIN products p ON ci.product_id = p.product_id 
            WHERE cart_id = $1"#,
            cart.cart_id
        )
        .fetch_all(&mut *tx)
        .await?;

        if cart_items.is_empty() {
            return Err(sqlx::Error::Protocol("Cart is empty".into()));
        }

        // Calculate total
        let total_amount: Decimal = cart_items
            .iter()
            .map(|item| item.price * Decimal::from(item.quantity))
            .sum();

        // Create order
        let order = sqlx::query_as!(
            Order,
            r#"INSERT INTO orders (
                user_id, 
                total_amount, 
                status, 
                shipping_address,
                order_date
            )
            VALUES ($1, $2, $3, $4, NOW())
            RETURNING 
                order_id, 
                user_id, 
                order_date, 
                status as "status!: OrderStatus",
                shipping_address,
                created_at,
                total_amount"#,
            user_id,
            total_amount,
            OrderStatus::Pending as OrderStatus,
            shipping_address
        )
        .fetch_one(&mut *tx)
        .await?;

        // Create order items
        for item in cart_items {
            sqlx::query!(
                "INSERT INTO order_details (
                    order_id, 
                    product_id, 
                    quantity, 
                    price_per_unit
                )
                VALUES ($1, $2, $3, $4)",
                order.order_id,
                item.product_id,
                item.quantity,
                item.price
            )
            .execute(&mut *tx)
            .await?;
        }

        // Clear cart
        sqlx::query!("DELETE FROM cart_items WHERE cart_id = $1", cart.cart_id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(order)
    }
}

// get request to retrieve all orders from the database
#[get("api/orders")]
pub async fn get_all_user_orders(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(user) => match Order::get_all_user_orders(&state.db, user.user_id).await {
            Ok(products) => HttpResponse::Ok().json(products),
            Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
        },
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

#[post("api/orders")]
pub async fn create_order(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
    body: Json<OrderBody>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            match Order::create_order(&state.db, body.shipping_address.clone(), user.user_id).await
            {
                Ok(order) => HttpResponse::Created().json(order),
                Err(err) => match err {
                    sqlx::Error::RowNotFound => HttpResponse::NotFound().json("Cart not found"),
                    sqlx::Error::Protocol(msg) if msg.contains("Cart is empty") => {
                        HttpResponse::BadRequest().json("Cart is empty")
                    }
                    _ => HttpResponse::InternalServerError().json(format!("{err:?}")),
                },
            }
        }
        None => HttpResponse::Unauthorized().json("unauthorized"),
    }
}

// admin only
// get request to get all orders
#[get("api/admin/orders")]
pub async fn get_all_orders(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            if user.is_admin() {
                match Order::get_all_orders(&state.db).await {
                    Ok(products) => HttpResponse::Ok().json(products),
                    Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
                }
            } else {
                HttpResponse::Unauthorized().json("customer not allowed to see all orders")
            }
        }
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

// put request to update the order status
#[put("api/admin/order")]
pub async fn update_order_status(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
    body: Json<UpdateBody>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            if user.is_admin() {
                match Order::update_order_status(
                    &state.db,
                    body.order_id,
                    body.order_status.clone(),
                )
                .await
                {
                    Ok(_) => HttpResponse::Ok().json("updated order successfully"),
                    Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
                }
            } else {
                HttpResponse::Unauthorized().json("customer not allowed to see all orders")
            }
        }
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}
