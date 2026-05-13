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
use crate::db::rules::{get_rules, save_rule, RuleData};
use crate::feeds::worker::FeedOp;
use crate::filter::Rule;
use crate::webui::menu::{menus, MenuItem};
use actix_web::{get, post, web, HttpResponse, Responder};
use askama::Template;
use diacritics::remove_diacritics;
use itertools::join;
use log::error;
use rust_stemmers::{Algorithm, Stemmer};
use serde::Deserialize;
use std::ops::Deref;

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

#[post("/rule/add")]
pub async fn rule_add(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    form: Option<web::Form<RuleData>>,
    feed_worker_q: web::Data<tokio::sync::mpsc::Sender<FeedOp>>,
) -> impl Responder {
    // log::info!("/rule/add {:#?}", form);
    match form {
        Some(ref f) => match save_rule(&db, f.deref()).await {
            Ok(uid) => {
                let _ = feed_worker_q.send(FeedOp::InvalidateFilters).await;
                uid
            }
            Err(e) => {
                error!("could not save rule {:#?}: {}", form, e);
                0
            }
        },
        _ => 0,
    };
    HttpResponse::Ok()
        .content_type("application/json")
        .body(r###"{"status": "ok"}"###)
}

#[derive(Deserialize)]
struct Term {
    q: String,
}

// @app.route("/stem")
// def stem():
//   term = flask.request.args.get('q', '')
//   stem = ' '.join(normalize.stem(normalize.get_words(term)))
// strip_tags_re = re.compile('<[^>]*>')
// def get_words(s):
//   return set([
//     word for word
//     in lower(str(strip_tags_re.sub('', str(s)))
//              ).translate(punct_map).split()
//     if word not in stop_words])
// def stem(words):
//   return {porter2.stem(word) for word in words}
//   return (stem, 200, {'Content-Type': 'text/plain'})

#[get("/stem")]
pub async fn stem(params: web::Query<Term>) -> impl Responder {
    let en_stemmer: Stemmer = Stemmer::create(Algorithm::English);
    let stop_words = stop_words::get(stop_words::LANGUAGE::English);
    // strip punctuation
    let cleaned: String = remove_diacritics(params.q.as_str())
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    HttpResponse::Ok().content_type("text/plain").body(join(
        cleaned
            .as_str()
            .split_whitespace()
            .filter(|w| !&stop_words.contains(&w.to_lowercase().as_str()))
            .map(|w| en_stemmer.stem(w).into_owned()),
        " ",
    ))
}
