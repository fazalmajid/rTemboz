/// Copyright (C) 2026 Fazal Majid
///
/// This program is free software: you can redistribute it and/or modify
/// it under the terms of the GNU Affero General Public License as published by
/// the Free Software Foundation, either version 3 of the License, or
/// (at your option) any later version.
///
/// This program is distributed in the hope that it will be useful,
/// but WITHOUT ANY WARRANTY; without even the implied warranty of
/// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
/// GNU Affero General Public License for more details.
///
/// You should have received a copy of the GNU Affero General Public License
/// along with this program.  If not, see <https://www.gnu.org/licenses/>.
///
// based on https://actix.rs/docs/middleware
use crate::db::auth::check_cookie;
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    web, Error, HttpResponse,
};
use futures_util::future::LocalBoxFuture;
use sqlx::sqlite::SqlitePool;
use std::future::{ready, Ready};

pub struct Authentication;

impl<S, B> Transform<S, ServiceRequest> for Authentication
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = AuthMiddleware<S>;
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(AuthMiddleware { service }))
    }
}

pub struct AuthMiddleware<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for AuthMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let cookie_data = req.cookie("auth").map(|c| {
            let session_id = c.value().to_owned();
            let db = req.app_data::<web::Data<SqlitePool>>().unwrap().clone();
            (session_id, db)
        });

        let path = req.path().to_owned();
        let fut = self.service.call(req);

        Box::pin(async move {
            if path.as_str() == "/login" {
                return fut.await;
            }
            if let Some((session_id, db)) = cookie_data {
                let pool: &SqlitePool = db.get_ref();
                if let Ok(()) = check_cookie(pool, &session_id).await {
                    return fut.await;
                }
            }
            let encoded = serde_urlencoded::to_string([("back", &path)]).unwrap_or_default();
            let redirect = HttpResponse::Found()
                .insert_header(("Location", format!("/login?{}", encoded)))
                .finish();
            Err(actix_web::error::InternalError::from_response("Unauthorized", redirect).into())
        })
    }
}
