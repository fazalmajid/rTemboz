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
use crate::db::worker::DbOp;
use actix_web::{HttpResponse, Responder, get, web};
use log::error;
use serde::Serialize;
use std::sync::mpsc::Sender;
use tokio::sync::oneshot;

#[derive(Serialize)]
struct Status {
    status: &'static str,
}

#[get("/sync")]
pub async fn sync(work_q: web::Data<Sender<DbOp>>) -> impl Responder {
    let (callback, response) = oneshot::channel();
    let result = work_q.send(DbOp::Sync { callback });
    if let Err(e) = result {
        error!("sync op failed: {}", e);
    }
    let ack = response.await;
    if let Err(e) = ack {
        error!("sync ack failed: {}", e);
    }
    HttpResponse::Ok().json(Status { status: "ok" })
}
