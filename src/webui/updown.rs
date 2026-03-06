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
use crate::db::items::ItemStatus;
use crate::db::worker::DbOp;
use actix_web::{HttpResponse, Responder, get, web};
use log::error;
use std::sync::mpsc::Sender;

#[get("/xmlfeedback/{operation}/{rand}/{uid}.xml")]
pub async fn enqueue(
    work_q: web::Data<Sender<DbOp>>,
    path: web::Path<(String, u64, u64)>,
) -> impl Responder {
    let (operation, rand, uid) = path.into_inner();
    match operation.as_str() {
        "promote" => work_q
            .send(DbOp::UpDown {
                new_status: ItemStatus::Interesting,
                uid,
            })
            .unwrap(),
        "demote" => work_q
            .send(DbOp::UpDown {
                new_status: ItemStatus::Uninteresting,
                uid,
            })
            .unwrap(),
        "basic" => work_q
            .send(DbOp::UpDown {
                new_status: ItemStatus::Unread,
                uid,
            })
            .unwrap(),
        _ => error!("unexpected op: {} {} {}", operation, rand, uid),
    }
    HttpResponse::Ok()
        .content_type("text/xml")
        .body("<?xml version=\"1.0\"?><nothing />")
}
