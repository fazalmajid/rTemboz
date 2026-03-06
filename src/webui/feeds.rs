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
use crate::db::feeds::{Feed, get_feeds};
use crate::webui::menu::{MenuItem, menus};
use actix_web::{HttpResponse, Responder, get, web};
use askama::Template;

#[derive(Template)]
#[template(path = "feeds.html")]
struct FeedsTemplate<'a> {
    feed_uid: u32,
    feeds: &'a Vec<Feed>,
    sum_unread: u32,
    sum_filtered: u32,
    sum_interesting: u32,
    sum_total: u32,
    menu_items: &'a Vec<MenuItem<'a>>,
    search: Option<String>,
    path: String,
}

#[get("/feeds")]
pub async fn feeds(db: web::Data<sqlx::sqlite::SqlitePool>) -> impl Responder {
    let feeds = get_feeds(db.get_ref()).await.unwrap();
    let sum_unread: u32 = feeds.iter().map(|f| f.unread).sum();
    let sum_filtered: u32 = feeds.iter().map(|f| f.filtered).sum();
    let sum_interesting: u32 = feeds.iter().map(|f| f.interesting).sum();
    let sum_total: u32 = feeds.iter().map(|f| f.total).sum();

    let template = FeedsTemplate {
        feed_uid: 0,
        feeds: &feeds,
        sum_unread,
        sum_filtered,
        sum_interesting,
        sum_total,
        menu_items: &menus("feeds"),
        search: None,
        path: "/view".to_string(),
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
