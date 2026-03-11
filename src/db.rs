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
use chrono::prelude::*;
use sqlx::sqlite::{Sqlite, SqliteConnectOptions, SqlitePool};
use sqlx::Pool;

pub mod auth;
pub mod feed;
pub mod feeds;
pub mod fts5;
pub mod items;
pub mod rules;
pub mod setting;
pub mod views;
pub mod worker;

const DB_FILENAME: &str = "temboz.db";

pub async fn create_db() -> Pool<Sqlite> {
    let db = SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(DB_FILENAME)
            .create_if_missing(true),
    )
    .await
    .unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    db
}

fn unix_ts(ts: Option<i64>) -> chrono::DateTime<Local> {
    chrono::Local.timestamp_nanos(ts.unwrap() * 1_000_000_000)
}

// delta is expressed in days
pub fn since(delta_t: f64) -> String {
    if delta_t == 0.0 {
        "never".to_string()
    } else if delta_t < 2.0 / 24.0 {
        format!("{} minutes ago", (delta_t * 24.0 * 60.0) as i32)
    } else if delta_t < 1.0 {
        format!("{} hours ago", (delta_t * 24.0) as i32)
    } else if delta_t < 2.0 {
        "one day ago".to_string()
    } else if delta_t < 3.0 {
        format!("{} days ago", delta_t as i32)
    } else {
        let secs = (delta_t * 86_400.0) as i64;
        let target_time = chrono::Local::now() - chrono::Duration::seconds(secs);
        format!("{}", target_time.format("%F"))
    }
}

// String::truncate can panic, so use this roundabout way instead
pub fn safe_truncate(s: String, max_len: usize) -> String {
    s.chars().take(max_len).collect()
}
