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
use crate::db::auth::check_password;
use actix_web::cookie::{time::Duration, Cookie};
use actix_web::{
    http::header, http::header::USER_AGENT, routes, web, HttpRequest, HttpResponse, Responder,
};
use askama::Template;
use serde::Deserialize;

#[derive(Template, Debug)]
#[template(path = "login.html")]
struct FeedTemplate {
    error_msg: String,
}

#[derive(Debug, Deserialize)]
struct Credentials {
    login: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct WhereFrom {
    back: String,
}

#[routes]
#[get("/login")]
#[post("/login")]
pub async fn login(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    form: Option<web::Form<Credentials>>,
    where_from: Option<web::Query<WhereFrom>>,
    req: HttpRequest,
) -> impl Responder {
    let error_message = match form {
        Some(c) => {
            match check_password(
                &db,
                &c.login,
                &c.password,
                req.headers()
                    .get(USER_AGENT)
                    .and_then(|val| val.to_str().ok())
                    .unwrap_or("Unknown"),
            )
            .await
            {
                Ok(Some(session_uuid)) => {
                    let redir = match where_from {
                        None => "/view",
                        Some(w) => &w.back.clone(),
                    };
                    let mut auth_cookie = Cookie::new("auth", session_uuid);
                    auth_cookie.set_max_age(Duration::days(14));
                    return HttpResponse::Found()
                        .cookie(auth_cookie)
                        .insert_header((header::LOCATION, redir))
                        .finish();
                }
                Ok(None) => "invalid login and/or password".to_string(),
                Err(_) => "internal error".to_string(),
            }
        }
        None => String::new(),
    };
    let template = FeedTemplate {
        error_msg: error_message,
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
