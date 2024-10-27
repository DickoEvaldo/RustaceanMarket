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

#[derive(Serialize, Deserialize, FromRow)]
struct Product {
    name: String,
    description: Option<String>,
    price: Decimal,
    stock_quantity: i32,
    category: Option<String>,
    is_available: Option<bool>,
    created_at: Option<DateTime<Utc>>,
    product_id: Uuid,
}

#[derive(Serialize, Deserialize, FromRow)]
struct ProductBody {
    name: String,
    description: Option<String>,
    price: Decimal,
    stock_quantity: i32,
}

impl Product {
    // impl to get all products from db
    async fn get_products(pool: &PgPool) -> Result<Vec<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "
            SELECT name, description, price, stock_quantity, category, 
                   is_available, created_at, product_id 
            FROM products;
            "
        )
        .fetch_all(pool)
        .await
    }

    // get single product detail
    async fn get_product_by_id(
        pool: &PgPool,
        product_id: Uuid,
    ) -> Result<Option<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "
        SELECT name, description, price, stock_quantity, category, 
               is_available, created_at, product_id 
        FROM products WHERE product_id = $1;
        ",
            product_id
        )
        .fetch_optional(pool)
        .await
    }

    // create product
    async fn create_product(
        pool: &PgPool,
        body: Json<ProductBody>,
    ) -> Result<Product, sqlx::Error> {
        let new_product = body.into_inner();
        sqlx::query_as!(Product, "INSERT INTO products (name, description, price, stock_quantity) VALUES ($1, $2, $3, $4) RETURNING *",
        new_product.name, new_product.description, new_product.price, new_product.stock_quantity
    )
        .fetch_one(pool)
        .await
    }

    // delete product
    async fn delete_product(pool: &PgPool, product_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query!("DELETE FROM products WHERE product_id = $1", product_id)
            .execute(pool)
            .await?;
        Ok(())
    }

    // edit product
    async fn edit_product_by_id(
        pool: &PgPool,
        product_id: Uuid,
        body: Json<ProductBody>,
    ) -> Result<Option<Product>, sqlx::Error> {
        let new_product = body.into_inner();
        sqlx::query_as!(
            Product,
            "UPDATE products 
            SET name = $1, description = $2,
            price = $3, stock_quantity = $4
            WHERE product_id = $5 RETURNING *
            ",
            new_product.name,
            new_product.description,
            new_product.price,
            new_product.stock_quantity,
            product_id
        )
        .fetch_optional(pool)
        .await
    }
}

// get request to get all the products
#[get("api/products")]
pub async fn get_products(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(_) => match Product::get_products(&state.db).await {
            Ok(products) => HttpResponse::Ok().json(products),
            Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
        },
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

// get request to get a product by id
#[get("api/product/{id}")]
pub async fn get_product_by_id(
    state: web::Data<AppState>,
    product_id: web::Path<Uuid>,
    req_user: Option<ReqData<TokenClaims>>,
) -> impl Responder {
    match req_user {
        Some(_) => match Product::get_product_by_id(&state.db, *product_id).await {
            Ok(Some(product)) => HttpResponse::Ok().json(product),
            Ok(None) => HttpResponse::Ok().json("product was not found"),
            Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
        },
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

// post request to create new product only admin
#[post("api/product")]
pub async fn create_product(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
    body: Json<ProductBody>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            if user.is_admin() {
                match Product::create_product(&state.db, body).await {
                    Ok(product) => HttpResponse::Ok().json(product),
                    Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
                }
            } else {
                HttpResponse::Forbidden().json("costumer cant create product")
            }
        }
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

// delete request to delete product by id
#[delete("api/product/{id}")]
pub async fn delete_product_id(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
    product_id: web::Path<Uuid>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            if user.is_admin() {
                match Product::delete_product(&state.db, *product_id).await {
                    Ok(_) => HttpResponse::Ok().json("product deleted sucessfully"),
                    Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
                }
            } else {
                HttpResponse::Forbidden().json("costumer cant delete product")
            }
        }
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}

// update product by id
#[put("api/product/{id}")]
pub async fn update_product_by_id(
    state: web::Data<AppState>,
    req_user: Option<ReqData<TokenClaims>>,
    product_id: web::Path<Uuid>,
    body: Json<ProductBody>,
) -> impl Responder {
    match req_user {
        Some(user) => {
            if user.is_admin() {
                match Product::edit_product_by_id(&state.db, *product_id, body).await {
                    Ok(Some(product)) => HttpResponse::Ok().json(product),
                    Ok(None) => HttpResponse::Ok().json("invalid product_id"),
                    Err(err) => HttpResponse::InternalServerError().json(format!("{err:?}")),
                }
            } else {
                HttpResponse::Forbidden().json("costumer cant edit product")
            }
        }
        None => HttpResponse::Unauthorized().json("unable to verify indentity"),
    }
}
