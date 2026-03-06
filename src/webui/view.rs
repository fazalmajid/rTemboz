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
use crate::db::items::{Item, ItemOrder, ItemStatus, get_items};
use crate::filter::RuleType;
use crate::webui::menu::{MenuItem, menus};
use actix_web::{HttpRequest, HttpResponse, Responder, get, web};
use askama::Template;
use itertools::Itertools; // for sorted() and join()
use log::info;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Template)]
#[template(path = "view.html")]
struct ViewTemplate<'a> {
    show: ItemStatus,
    show_menu: Vec<(String, String)>,
    order: ItemOrder,
    order_menu: Vec<(String, String)>,
    //name: &'a str,
    item_desc: &'a str,
    feed_uid: u32, // feed we are filtering on, 0 means no filtering
    items: &'a Vec<Item>,
    overload_threshold: u32,
    menu_items: &'a Vec<MenuItem<'a>>,
    search: Option<String>,
    path: String,
}

impl Item {
    fn tag_info(&self) -> String {
        let mut spans: Vec<String> = self
            .tags
            .iter()
            .sorted()
            .map(|t| format!("<span class=\"item tag\">{}</span>", t))
            .collect();
        if self.creator != "Unknown" {
            spans.push(format!(
                "<span class=\"author tag\">{}</span>",
                self.creator
            ))
        }
        format!(
            "<div class=\"tag_info\" id=\"tags_{}\">{}</div>",
            self.uid,
            spans.join(" ")
        )
    }

    fn tag_call(&self) -> String {
        if self.tags.is_empty() && self.creator == "Unknown" {
            "(no tags)".to_string()
        } else {
            format!("<a href=\"javascript:toggle_tags({});\">tags</a>", self.uid)
        }
    }

    fn rule_explanation(&self) -> String {
        match &self.rule {
            Some(r) => match r.rule_type {
                RuleType::Author => format!(
                    "\n<br><hr><br><p>Filtered for author <span class=\"author tag highlighted\">{}</span> (rule {})</p>",
                    r.text, r.uid
                ),
                RuleType::Tag => format!(
                    "\n<br><hr><br><p>Filtered for tag <span class=\"item tag highlighted\">{}</span> (rule {})</p>",
                    r.text, r.uid
                ),
                _ => format!(
                    "\n<br><hr><br><p>Filtered for {}:{} (rule {})</p>",
                    r.rule_type, r.text, r.uid
                ),
            },
            _ => "".to_string(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
struct Params {
    feed_uid: Option<u32>,
    show: Option<String>,
    order: Option<String>,
    search: Option<String>,
    search_in: Option<String>,
}

fn gen_show_menu(mut qs: web::Query<Params>) -> Vec<(String, String)> {
    let mut result = Vec::with_capacity(5);
    for val in ["All", "Unread", "Interesting", "Uninteresting", "Filtered"] {
        qs.show = Some(val.to_string());
        result.push((
            val.to_string(),
            serde_urlencoded::to_string(&*qs).unwrap_or_default(),
        ));
    }
    result
}

fn gen_order_menu(mut qs: web::Query<Params>) -> Vec<(String, String)> {
    let mut result = Vec::with_capacity(6);
    for val in ["published", "seen", "rated", "SNR", "oldest", "random"] {
        qs.order = Some(val.to_string());
        result.push((
            val.to_string(),
            serde_urlencoded::to_string(&*qs).unwrap_or_default(),
        ));
    }
    result
}

#[get("/view")]
pub async fn view(
    db: web::Data<sqlx::sqlite::SqlitePool>,
    qs: web::Query<Params>,
    req: HttpRequest,
) -> impl Responder {
    info!("GET {}", req.uri());
    let show = ItemStatus::from_str(&qs.show.clone().unwrap_or("unread".to_string())).unwrap();
    let show_menu = gen_show_menu(qs.clone());
    let order = ItemOrder::from_str(&qs.order.clone().unwrap_or("seen".to_string())).unwrap();
    let order_menu = gen_order_menu(qs.clone());
    let items = get_items(
        db.get_ref(),
        show,
        qs.feed_uid,
        qs.search.clone(),
        qs.search_in.clone(),
        order,
    )
    .await
    .unwrap();
    let feed_uid = qs.feed_uid.unwrap_or(0);
    let template = ViewTemplate {
        show,
        show_menu,
        order,
        order_menu,
        item_desc: "DESC",
        feed_uid,
        items: &items,
        overload_threshold: 200,
        menu_items: &menus("view"),
        search: qs.search.clone(),
        path: "/view".to_string(),
    };
    HttpResponse::Ok()
        .content_type("text/html")
        .body(template.render().unwrap())
}
