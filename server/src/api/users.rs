//----------------------------------------IMPORTS----------------------------------------//
use crate::AppState;
use actix_web::{
    dev::ServiceRequest,
    get, post,
    web::{self, Json},
    HttpMessage, HttpResponse, Responder,
};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

// for auth import
use actix_web_httpauth::{
    extractors::{
        basic::BasicAuth,
        bearer::{self, BearerAuth},
        AuthenticationError,
    },
    middleware::HttpAuthentication,
};

use argonautica::{Hasher, Verifier};
use chrono::NaiveDateTime;
use hmac::{
    digest::{core_api::CoreWrapper, KeyInit},
    Hmac, HmacCore,
};
use jwt::SignWithKey;
use jwt::VerifyWithKey;
use sha2::Sha256;
//----------------------------------------IMPORTS----------------------------------------//

// token struct
#[derive(Serialize, Deserialize, Clone)]
pub struct TokenClaims {
    pub user_id: Uuid,
    role: UserRole,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type, Clone)]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Customer,
}

// user struct
#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    user_id: Uuid,
    first_name: String,
    last_name: String,
    phone: Option<String>,
    email: String,
    role: UserRole,
}

// struct for create user body
#[derive(Deserialize)]
struct CreateUserBody {
    first_name: String,
    last_name: String,
    email: String,
    password: String,
    phone: String,
}

// struct for user response
#[derive(Serialize, FromRow)]
struct UserResponse {
    user_id: Uuid,
    first_name: String,
    last_name: String,
    email: String,
    phone: Option<String>,
}

#[derive(Serialize, FromRow)]
struct AuthResponse {
    user_id: Uuid,
    email: String,
    password_hash: String,
    role: UserRole,
}

// User implementation
impl User {
    // get all the users
    async fn get_all(pool: &PgPool) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"SELECT 
                user_id, 
                first_name, 
                last_name, 
                phone, 
                email, 
                role as "role!: UserRole"  -- Note the ! to make it non-null
            FROM users"#
        )
        .fetch_all(pool)
        .await
    }

    // get user by the id
    async fn get_by_id(pool: &PgPool, user_id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            r#"SELECT 
                user_id, 
                first_name, 
                last_name, 
                phone, 
                email, 
                role as "role!: UserRole" 
            FROM users 
            WHERE user_id = $1"#,
            user_id
        )
        .fetch_optional(pool)
        .await
    }

    async fn create_user(
        pool: &PgPool,
        body: Json<CreateUserBody>,
    ) -> Result<UserResponse, sqlx::Error> {
        // check if user already exist
        let new_user = body.into_inner();
        let existing_user =
            sqlx::query!("SELECT email FROM users WHERE email = $1", new_user.email)
                .fetch_optional(pool)
                .await?;

        // if email already exist return error
        if existing_user.is_some() {
            return Err(sqlx::Error::Protocol("Email already exist".into()));
        }

        // hash the password
        let hash_secret = std::env::var("HASH_SECRET").expect("Hash secret must be set");
        let mut hasher = Hasher::default();
        let hashed_password = hasher
            .with_password(new_user.password)
            .with_secret_key(hash_secret)
            .hash()
            .unwrap();

        // create new user
        sqlx::query_as!(UserResponse, "INSERT INTO users (first_name, last_name, email, password_hash, phone) VALUES ($1, $2, $3, $4, $5) RETURNING user_id, first_name, last_name, email, phone", new_user.first_name, new_user.last_name, new_user.email, hashed_password, new_user.phone).fetch_one(pool).await
    }
}

// validator for bearer_middleware
pub async fn validator(
    req: ServiceRequest,
    credentials: BearerAuth,
) -> Result<ServiceRequest, (actix_web::Error, ServiceRequest)> {
    // Changed Error type
    dotenv::dotenv().ok();
    let jwt_secret: String = std::env::var("JWT_SECRET").expect("JWT SECRET must be set");
    let key: Hmac<Sha256> =
        <CoreWrapper<HmacCore<_>> as KeyInit>::new_from_slice(jwt_secret.as_bytes()).unwrap();
    let token_string = credentials.token();

    let claims: Result<TokenClaims, jwt::Error> = token_string.verify_with_key(&key);

    match claims {
        Ok(value) => {
            req.extensions_mut().insert(value);
            Ok(req)
        }
        Err(_) => {
            let config = req
                .app_data::<bearer::Config>()
                .cloned()
                .unwrap_or_default()
                .scope("localhost:8080");

            Err((AuthenticationError::from(config).into(), req))
        }
    }
}

// get all user request
#[get("/users")]
pub async fn get_user(state: web::Data<AppState>) -> impl Responder {
    match User::get_all(&state.db).await {
        // return response 200 and users on sucess
        Ok(users) => HttpResponse::Ok().json(users),
        // return server error 500 on fail
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// get request to get user by id
#[get("/users/{id}")]
pub async fn get_user_by_id(
    state: web::Data<AppState>,
    user_id: web::Path<Uuid>,
) -> impl Responder {
    match User::get_by_id(&state.db, *user_id).await {
        // if id found return response 200
        Ok(Some(user)) => HttpResponse::Ok().json(user),
        // if id is not found return response 200
        Ok(None) => HttpResponse::NotFound().body(format!("User ID: {user_id} not found")),
        // if not found return response 404
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

// post request to create new user / register
#[post("/users")]
pub async fn create_user(state: web::Data<AppState>, body: Json<CreateUserBody>) -> impl Responder {
    match User::create_user(&state.db, body).await {
        // return response 200 and users on sucess
        Ok(users) => HttpResponse::Ok().json(users),
        // return server error 500 on fail
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

#[get("/auth")]
pub async fn auth(state: web::Data<AppState>, credentials: BasicAuth) -> impl Responder {
    let jwt_secret: String = std::env::var("JWT_SECRET").expect("jwt secret must be set");
    let key: Hmac<Sha256> =
        <CoreWrapper<HmacCore<_>> as KeyInit>::new_from_slice(jwt_secret.as_bytes()).unwrap();

    let email = credentials.user_id().to_string();
    let password = credentials.password();

    match password {
        None => HttpResponse::Unauthorized().json("Must provide username and password"),
        Some(pass) => {
            match sqlx::query_as!(
                AuthResponse,
                r#"SELECT user_id, email, password_hash, role as "role!: UserRole"
                FROM users WHERE email = $1"#,
                email
            )
            .fetch_one(&state.db)
            .await
            {
                Ok(user) => {
                    let hash_secret =
                        std::env::var("HASH_SECRET").expect("hash secret must be set");
                    let mut verifier = Verifier::default();
                    let is_valid = verifier
                        .with_hash(user.password_hash)
                        .with_password(pass)
                        .with_secret_key(hash_secret)
                        .verify()
                        .expect("failed to verify");

                    if is_valid {
                        let claims = TokenClaims {
                            user_id: user.user_id,
                            role: user.role,
                        };
                        let token_str = claims.sign_with_key(&key).expect("failed to sign in");
                        HttpResponse::Ok().json(token_str)
                    } else {
                        HttpResponse::Unauthorized().json("incorrect email or password")
                    }
                }
                Err(err) => HttpResponse::InternalServerError().json(format!("{:?}", err)),
            }
        }
    }
}

// Helper functions for role checking
impl TokenClaims {
    pub fn is_admin(&self) -> bool {
        matches!(self.role, UserRole::Admin)
    }

    pub fn is_customer(&self) -> bool {
        matches!(self.role, UserRole::Customer)
    }
}
