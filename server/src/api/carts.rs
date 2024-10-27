use crate::{api::users::TokenClaims, AppState};
use actix_web::{
    body, delete, get, post, put,
    web::{self, Json, ReqData},
    HttpMessage, HttpResponse, Responder,
};
use chrono::{DateTime, Utc};
use serde::{de::Error, Deserialize, Serialize};
use sqlx::{types::Decimal, FromRow, PgPool};
use uuid::Uuid;

#[derive(Serialize, Deserialize, FromRow)]
struct Cart {
    cart_id: Uuid,
    user_id: Option<Uuid>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, FromRow)]
struct CartItem {
    cart_item_id: Option<Uuid>,
    cart_id: Option<Uuid>,
    product_id: Option<Uuid>,
    quantity: Option<i32>,
    added_at: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, FromRow)]
struct CartItemBody {
    cart_id: Option<Uuid>,
    product_id: Uuid,
    quantity: i32,
}

#[derive(Serialize, Deserialize, FromRow)]
struct CartBody {
    user_id: Option<Uuid>,
}

#[derive(Serialize, FromRow)]
struct CartItemWithProduct {
    cart_item_id: Option<Uuid>,
    cart_id: Option<Uuid>,
    product_id: Option<Uuid>,
    quantity: Option<i32>,
    added_at: Option<DateTime<Utc>>,
    product_name: String,
    product_price: Decimal,
}

impl Cart {
    async fn get_or_create_cart(pool: &PgPool, user_id: Uuid) -> Result<Cart, sqlx::Error> {
        // First try to get existing active cart
        if let Some(cart) = sqlx::query_as!(
            Cart,
            "SELECT * FROM carts WHERE user_id = $1 LIMIT 1",
            user_id
        )
        .fetch_optional(pool)
        .await?
        {
            Ok(cart)
        } else {
            // Create new cart if none exists
            sqlx::query_as!(
                Cart,
                "INSERT INTO carts (user_id) VALUES ($1) RETURNING *",
                user_id
            )
            .fetch_one(pool)
            .await
        }
    }

    async fn get_cart_with_items(
        pool: &PgPool,
        cart_id: Uuid,
    ) -> Result<Vec<CartItemWithProduct>, sqlx::Error> {
        sqlx::query_as!(
            CartItemWithProduct,
            "
            SELECT 
            cart_items.*,
            products.name as product_name,
            products.price as product_price 
            FROM cart_items 
            JOIN products ON cart_items.product_id = products.product_id
            WHERE cart_items.cart_id = $1",
            cart_id
        )
        .fetch_all(pool)
        .await
    }

    async fn add_cart_item(pool: &PgPool, body: CartItemBody) -> Result<CartItem, sqlx::Error> {
        if let Some(cart_item) = sqlx::query_as!(
            CartItem,
            "SELECT * FROM cart_items WHERE cart_id = $1 AND product_id = $2",
            body.cart_id,
            body.product_id
        )
        .fetch_optional(pool)
        .await?
        {
            sqlx::query_as!(
                CartItem,
                "UPDATE cart_items SET quantity = $1 WHERE cart_item_id = $2 RETURNING *",
                cart_item.quantity.unwrap_or(0) + body.quantity,
                cart_item.cart_item_id
            )
            .fetch_one(pool)
            .await
        } else {
            sqlx::query_as!(
                CartItem,
                "INSERT INTO cart_items (cart_id, product_id) VALUES ($1, $2) RETURNING *",
                body.cart_id,
                body.product_id
            )
            .fetch_one(pool)
            .await
        }
    }
}

#[get("api/carts")]
pub async fn get_cart(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(user) => match Cart::get_or_create_cart(&state.db, user.user_id).await {
            Ok(cart) => match Cart::get_cart_with_items(&state.db, cart.cart_id).await {
                Ok(cart_with_items) => HttpResponse::Ok().json(cart_with_items),
                Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
            },
            Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
        },
        None => HttpResponse::Unauthorized().json("Please log in"),
    }
}

#[post("api/cart-items")]
pub async fn add_cart_item(
    state: web::Data<AppState>,
    body: Json<CartItemBody>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(user) => match Cart::get_or_create_cart(&state.db, user.user_id).await {
            Ok(cart) => match Cart::add_cart_item(
                &state.db,
                CartItemBody {
                    cart_id: Some(cart.cart_id),
                    product_id: body.product_id,
                    quantity: body.quantity,
                },
            )
            .await
            {
                Ok(cart_with_items) => HttpResponse::Ok().json(cart_with_items),
                Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
            },
            Err(e) => HttpResponse::InternalServerError().json(e.to_string()),
        },
        None => HttpResponse::Unauthorized().json("invalid user please log in"),
    }
}
