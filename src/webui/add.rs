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
use crate::db::feed::{add_feed, Message};
use crate::feeds::worker::FeedOp;
use crate::webui::menu::{menus, MenuItem};
use actix_web::{routes, web, HttpResponse, Responder};
use askama::Template;
use log::error;
use std::collections::HashMap;
use std::error::Error;

#[derive(Template, Debug)]
#[template(path = "add.html")]
struct AddTemplate<'a> {
    menu_items: &'a Vec<MenuItem<'a>>,
    search: Option<String>,
    path: String,
    feed_uid: u32,
    msg: Option<Message>,
}

#[routes]
#[get("/add")]
#[post("/add")]
pub async fn add(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    feed_worker_q: web::Data<tokio::sync::mpsc::Sender<FeedOp>>,
    form: Option<web::Form<HashMap<String, String>>>,
) -> impl Responder {
    let feed_xml = form.and_then(|f| f.into_inner().remove("feed_xml"));
    let msg = match feed_xml {
        Some(url) => match add_feed(db.get_ref(), feed_worker_q.get_ref(), url).await {
            Err(e) => {
                let source = match e.source() {
                    Some(src) => src.to_string(),
                    _ => "".to_string(),
                };
                error!("Could not add feed: {} ({})", e, source);
                Some(Message {
                    feed_uid: 0,
                    feed_title: format!("Could not add feed: {} ({})", e, source),
                    added: 0,
                    filtered: 0,
                })
            }
            Ok(result) => Some(result),
        },
        _ => None,
    };
    let template = AddTemplate {
        menu_items: &menus("add"),
        search: None,
        path: "/add".to_string(),
        feed_uid: 0, // dummy
        msg,
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
