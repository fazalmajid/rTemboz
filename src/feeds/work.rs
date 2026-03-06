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
use crate::filter::Filters;
use fastbloom::AtomicBloomFilter;
use feedparser_rs::types::ParsedFeed;
use std::sync::Arc;

pub struct Work {
    pub feed_uid: u32,
    pub rss: ParsedFeed,
    // pub etag: String,
    pub bloom: Arc<AtomicBloomFilter>,
    pub filters: Arc<Filters>,
    pub db_q: std::sync::mpsc::Sender<DbOp>,
}
