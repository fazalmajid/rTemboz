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
use crate::db::items::get_item_stats;
use crate::db::rules::{get_rule_stats, RuleStats};
use crate::webui::menu::{menus, MenuItem};
use actix_web::{get, web, HttpResponse, Responder};
use askama::Template;
use csv::Writer;
use humantime::{format_duration, format_rfc3339};
use std::env;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate<'a> {
    stats: Vec<RuleStats>,
    executable: String,
    status: String,
    started: String,
    uptime: String,
    menu_items: &'a Vec<MenuItem<'a>>,
    // dummy values to keep menubar.html happy
    path: String,
    search: Option<String>,
    feed_uid: u32,
}

#[get("/settings")]
pub async fn settings(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    started_: web::Data<std::time::SystemTime>,
) -> impl Responder {
    let stats = get_rule_stats(db.get_ref()).await.unwrap();
    let args: Vec<String> = env::args().collect();
    let executable = args.join(" ");
    let start_time = *started_.get_ref();
    let started = format_rfc3339(start_time).to_string();
    let uptime = format_duration(start_time.elapsed().unwrap()).to_string();
    let template = SettingsTemplate {
        stats,
        executable,
        started,
        uptime,
        status: "".to_string(),
        menu_items: &menus("rules"),
        path: "/settings".to_string(),
        search: None,
        feed_uid: 0,
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}

// XXX make this streaming instead of fetching all the rows first
#[get("/stats")]
pub async fn get_stats(db: web::Data<sqlx::sqlite::SqlitePool>) -> impl Responder {
    let rows = get_item_stats(db.get_ref()).await.unwrap();
    let mut wtr = Writer::from_writer(vec![]);
    for row in rows {
        let _ = wtr.serialize(row);
    }
    let _ = wtr.flush();

    HttpResponse::Ok()
        .content_type("text/csv")
        .body(wtr.into_inner().unwrap())
}
