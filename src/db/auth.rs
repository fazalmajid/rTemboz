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
use crate::db::setting::set_setting;
use argon2::{self, verify_encoded, Config};
use sqlx::error::Error;
use sqlx::sqlite::SqlitePool;
use uuid::Uuid;

pub async fn check_cookie(db: &SqlitePool, session: &str) -> Result<(), Error> {
    let _row = sqlx::query!(
        r###"
SELECT login, user_agent
FROM session
WHERE uuid=? AND julianday('now') BETWEEN created AND expires
"###,
        session
    )
    .fetch_one(db)
    .await?;
    Ok(())
}

pub async fn check_password(
    db: &SqlitePool,
    login: &str,
    password: &str,
    user_agent: &str,
) -> Result<Option<String>, Error> {
    let row = sqlx::query!(
        r###"
SELECT login.value AS Login, password.value AS password_hash
FROM setting login, setting password
WHERE login.name='login' AND password.name='passwd'
"###,
    )
    .fetch_one(db)
    .await?;
    match verify_encoded(&row.password_hash, password.as_bytes()).unwrap_or(false) {
        true => {
            let session_uuid = Uuid::new_v4().to_string();
            let _ = sqlx::query!(
                r###"
INSERT INTO session (uuid, login, user_agent)
VALUES (?, ?, ?)
"###,
                session_uuid,
                login,
                user_agent
            )
            .execute(db)
            .await?;
            Ok(Some(session_uuid))
        }
        _ => Ok(None),
    }
}

pub async fn change_password(
    db: &SqlitePool,
    login: String,
    password: String,
) -> Result<(), Error> {
    let salt: [u8; 16] = rand::random();
    let config = Config::default();
    let password_hash = argon2::hash_encoded(password.as_bytes(), &salt, &config).unwrap();
    let _ = set_setting(db, "login".to_string(), login).await;
    let _ = set_setting(db, "passwd".to_string(), password_hash).await;
    let _ = sqlx::query!("DELETE FROM session").execute(db).await?;
    Ok(())
}
