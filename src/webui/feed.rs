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
use crate::db::feed::{
    catchup, dedupe, get_feed_details, reload, update_feed_details, update_feed_dupcheck,
    update_feed_exempt, update_feed_privacy, update_feed_status,
};
use crate::db::feeds::{Feed, FeedStatus};
use crate::db::rules::get_top_rules;
use crate::feeds::worker::{refresh_one, FeedOp};
use crate::filter::Rule;
use crate::webui::menu::{menus, MenuItem};
use actix_web::{routes, web, HttpResponse, Responder};
use askama::Template;
use log::{error, info};
use serde::Deserialize;
use std::error::Error;

#[derive(Template, Debug)]
#[template(path = "feed.html")]
struct FeedTemplate<'a> {
    // show: bool,
    feed: Feed,
    // info: FeedInfo,
    notices: Vec<String>,
    op: String,
    confirm: bool,
    back: String,
    ratio: f32,
    status_change_op: String,
    exempt_change_op: String,
    exempt_text: String,
    private_change_op: String,
    private_text: String,
    dupcheck_change_op: String,
    dupcheck_text: String,
    top_rules: Vec<(Rule, u32)>,

    feed_uid: u32,
    // ratings_list: String,
    // sort_desc: bool,
    // sort_list: String,
    // overload_threshold: u32,
    menu_items: &'a Vec<MenuItem<'a>>,
    search: Option<String>,
    path: String,

    deduped: u64,
    caught_up: u64,
    purged: u64,
    refresh_msg: String,
}

#[derive(Debug, Deserialize)]
struct FeedDetails {
    feed_title: String,
    feed_html: String,
    feed_xml: String,
    feed_pubxml: String,
    feed_desc: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Params {
    Details(FeedDetails),
    Confirm { confirm: String },
}

#[routes]
#[get("/feed/{uid}{op:/?.*}")]
#[post("/feed/{uid}{op:/?.*}")]
pub async fn feed(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    uid: web::Path<(u32, Option<String>)>,
    feed_worker_q: web::Data<tokio::sync::mpsc::Sender<FeedOp>>,
    form: Option<web::Form<Params>>,
) -> impl Responder {
    let (uid, op) = uid.into_inner();
    let confirm = match &form {
        Some(f) => match &f.0 {
            Params::Confirm { confirm: h } => *h == "yes".to_string(),
            _ => false,
        },
        _ => false,
    };
    let op = op.clone().unwrap_or_default();
    info!(
        "Got /feed request op={} confirm={} details={:#?}",
        op, confirm, form
    );
    let deduped = match op == "/dedupe" && confirm {
        true => match dedupe(db.get_ref(), uid).await {
            Ok(rowcount) => rowcount,
            Err(e) => {
                error!("FEED-{} error deduplicating: {}", uid, e);
                0
            }
        },
        _ => 0,
    };
    let caught_up = match op == "/catchup" && confirm {
        true => match catchup(db.get_ref(), uid).await {
            Ok(rowcount) => rowcount,
            Err(e) => {
                error!("FEED-{} error catching up: {}", uid, e);
                0
            }
        },
        _ => 0,
    };
    let purged = match op == "/reload" && confirm {
        true => {
            match reload(db.get_ref(), uid).await {
                Ok(rowcount) => rowcount,
                Err(e) => {
                    error!("FEED-{} error reloading: {}", uid, e);
                    0
                } // XXX issue a reload here
            }
        }
        _ => 0,
    };
    // operations that don't require confirmation
    let dbop_result = match op.as_str() {
        "/suspend" => update_feed_status(db.get_ref(), uid, 1).await,
        "/activate" => update_feed_status(db.get_ref(), uid, 0).await,
        "/private" => update_feed_privacy(db.get_ref(), uid, 1).await,
        "/public" => update_feed_privacy(db.get_ref(), uid, 0).await,
        "/exempt" => update_feed_exempt(db.get_ref(), uid, 1).await,
        "/reinstate" => update_feed_exempt(db.get_ref(), uid, 0).await,
        "/dupcheck" => update_feed_dupcheck(db.get_ref(), uid, 1).await,
        "/nodupcheck" => update_feed_dupcheck(db.get_ref(), uid, 0).await,
        _ => Ok(()),
    };
    match dbop_result {
        Ok(()) => (),
        Err(e) => {
            error!("FEED-{} error applying op={}: {}", uid, op, e);
        }
    }
    let mut feed = get_feed_details(db.get_ref(), uid).await.unwrap();
    let feed_uid = feed.uid;
    let refresh_needed = match form {
        None => false,
        Some(f) => match &f.0 {
            Params::Details(fields) => {
                let changed = fields.feed_xml != feed.xml;
                if fields.feed_title != feed.title
                    || fields.feed_html != feed.html
                    || fields.feed_xml != feed.xml
                    || fields.feed_pubxml != feed.pubxml
                    || fields.feed_desc != feed.description
                {
                    match update_feed_details(
                        db.get_ref(),
                        feed_uid,
                        fields.feed_title.clone(),
                        fields.feed_html.clone(),
                        fields.feed_xml.clone(),
                        fields.feed_pubxml.clone(),
                        fields.feed_desc.clone(),
                    )
                    .await
                    {
                        Ok(()) => {
                            info!(
                                "FEED-{} updated details to {:#?}, refresh_needed: {}",
                                feed_uid, fields, changed
                            );
                            feed = get_feed_details(db.get_ref(), uid).await.unwrap();
                            changed
                        }
                        Err(e) => {
                            error!("FEED-{} error updating details: {}", feed_uid, e);
                            false
                        }
                    }
                } else {
                    false
                }
            }
            _ => false,
        },
    };
    let refresh_msg = match refresh_needed || op == "/refresh" {
        true => match refresh_one(feed_worker_q.as_ref(), feed.clone()).await {
            Ok((added, filtered)) => {
                feed = get_feed_details(db.get_ref(), uid).await.unwrap();
                format!(
                    "<p>Successfully refreshed, {} new items, {} filtered.</p>",
                    added, filtered
                )
            }
            Err(e) => {
                let source = match e.source() {
                    Some(src) => src.to_string(),
                    _ => "".to_string(),
                };

                error!("FEED-{} error refreshing: {} ({})", uid, e, source);
                format!("FEED-{} error refreshing: {} ({})", uid, e, source)
            }
        },
        _ => "".to_string(),
    };
    let top_rules = get_top_rules(db.get_ref(), uid).await.unwrap();
    let ratio = if feed.interesting + feed.uninteresting > 0 {
        feed.interesting as f32 * 100.0 / (feed.interesting + feed.uninteresting) as f32
    } else {
        0.0
    };
    let status_change_op = match feed.status {
        FeedStatus::Active => "suspend".to_string(),
        _ => "activate".to_string(),
    };
    let exempt_change_op = match feed.exempt {
        true => "reinstate".to_string(),
        _ => "exempt".to_string(),
    };
    let exempt_text = match feed.exempt {
        true => "Exempt".to_string(),
        _ => "Not exempt".to_string(),
    };
    let private_change_op = match feed.private {
        true => "public".to_string(),
        _ => "private".to_string(),
    };
    let private_text = match feed.private {
        true => "Private".to_string(),
        _ => "Public".to_string(),
    };
    let dupcheck_change_op = match feed.dupcheck {
        true => "nodupcheck".to_string(),
        _ => "dupcheck".to_string(),
    };
    let dupcheck_text = match feed.dupcheck {
        true => "Duplicate checking in effect".to_string(),
        _ => "No duplicate checking".to_string(),
    };
    let template = FeedTemplate {
        // show: true,
        feed_uid,
        feed,
        // info,
        notices: vec![],
        op,
        confirm,
        back: "".to_string(),
        ratio,
        status_change_op,
        exempt_change_op,
        exempt_text,
        private_change_op,
        private_text,
        dupcheck_change_op,
        dupcheck_text,
        top_rules,
        // ratings_list: String::new(),
        // sort_desc: false,
        // sort_list: String::new(),
        // overload_threshold: 200,
        menu_items: &menus("feed"),
        search: None,
        path: "/view".to_string(),
        deduped,
        caught_up,
        purged,
        refresh_msg,
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
