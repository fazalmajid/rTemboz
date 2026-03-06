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
use crate::db::rules::get_rules;
use crate::filter::Rule;
use crate::webui::menu::{MenuItem, menus};
use actix_web::{HttpResponse, Responder, get, web};
use askama::Template;

#[derive(Template)]
#[template(path = "rules.html")]
struct RulesTemplate<'a> {
    menu_items: &'a Vec<MenuItem<'a>>,
    rules: Vec<Rule>,
    // dummy values to keep menubar.html happy
    path: String,
    search: Option<String>,
    feed_uid: u32,
}

#[get("/rules")]
pub async fn rules(db: web::Data<sqlx::sqlite::SqlitePool>) -> impl Responder {
    let template = RulesTemplate {
        menu_items: &menus("rules"),
        rules: get_rules(db.get_ref()).await.unwrap_or_default(),

        path: "/rules".to_string(),
        search: None,
        feed_uid: 0,
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
