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
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;

pub async fn set_setting(db: &SqlitePool, name: String, value: String) -> Result<(), Error> {
    let _ = sqlx::query!(
        "INSERT OR REPLACE INTO setting (name, value) VALUES (?, ?)",
        name,
        value
    )
    .execute(db)
    .await?;
    Ok(())
}
