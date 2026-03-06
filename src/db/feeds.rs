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
use crate::db::since;
use chrono::naive::NaiveDateTime;
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;
use std::fmt;

#[derive(Debug, Clone)]
pub enum FeedStatus {
    Active,
    Suspended,
}

impl fmt::Display for FeedStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeedStatus::Active => write!(f, "Active"),
            _ => write!(f, "Suspended"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Feed {
    pub uid: u32,
    pub title: String,
    pub description: String,
    pub html: String,
    pub xml: String,
    pub pubxml: String,
    pub last_modified: String,
    pub interesting: u32,
    pub unread: u32,
    pub uninteresting: u32,
    pub filtered: u32,
    pub total: u32,
    pub snr: f64,
    pub status: FeedStatus,
    pub private: bool,
    pub exempt: bool,
    pub errors: u32,
    pub dupcheck: bool,
    pub etag: String,
    pub last_fetched: NaiveDateTime,
}

impl Feed {
    pub fn desc_rows(&self) -> u8 {
        1
    }
}

pub async fn get_feeds(db: &SqlitePool) -> Result<Vec<Feed>, Error> {
    let rows = sqlx::query!(
        r###"
SELECT
  uid, title, html, xml, last_modified, interesting, unread, uninteresting,
  filtered, total, snr, status, private, exempt, errors, dupcheck, etag,
  last_fetched, last_parsed, last_error
FROM v_feeds_snr
ORDER BY (unread > 0) DESC, snr DESC"###
    )
    .fetch_all(db)
    .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        result.push(Feed {
            uid: row.uid.unwrap() as u32,
            title: row.title,
            description: "".to_string(),
            html: row.html,
            xml: row.xml,
            pubxml: "".to_string(),
            last_modified: since(row.last_modified.unwrap_or(0.0)),
            interesting: row.interesting as u32,
            unread: row.unread as u32,
            uninteresting: row.uninteresting as u32,
            filtered: row.filtered as u32,
            total: row.total as u32,
            snr: row.snr,
            status: if row.status == 0 {
                FeedStatus::Active
            } else {
                FeedStatus::Suspended
            },
            private: row.private.unwrap_or(0) != 0,
            exempt: row.exempt != 0,
            errors: row.errors as u32,
            dupcheck: row.dupcheck.unwrap_or(0) != 0,
            etag: row.etag.unwrap_or("".to_string()),
            last_fetched: row
                .last_fetched
                .unwrap_or(chrono::DateTime::UNIX_EPOCH.naive_utc()),
        });
    }
    Ok(result)
}
