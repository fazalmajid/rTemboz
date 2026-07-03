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
/// SQLite user-defined function (UDF) implementation for URL manipulation
///
use libsqlite3_sys as ffi;
use sqlx::sqlite::SqliteConnection;
use std::ffi::CString;

pub async fn register_udf(conn: &mut SqliteConnection) {
    let mut handle = conn.lock_handle().await.unwrap();
    let raw = handle.as_raw_handle().as_ptr();

    extern "C" fn normalize_url(
        ctx: *mut ffi::sqlite3_context,
        _argc: i32,
        argv: *mut *mut ffi::sqlite3_value,
    ) {
        unsafe {
            let ptr = ffi::sqlite3_value_text(*argv); // guaranteed to be UTF8
            if ptr.is_null() {
                ffi::sqlite3_result_null(ctx);
                return;
            }
            let len = ffi::sqlite3_value_bytes(*argv) as usize;
            let bytes = std::slice::from_raw_parts(ptr, len);
            let url = std::str::from_utf8(bytes).unwrap_or("");
            let norm = match url.find('#') {
                Some(idx) => &url[0..idx],
                None => url,
            };
            ffi::sqlite3_result_text(
                ctx,
                norm.as_ptr().cast(),
                norm.len() as i32,
                ffi::SQLITE_TRANSIENT(), // SQLite will copy the string
            );
        }
    }

    let name = CString::new("normalize_url").unwrap();
    unsafe {
        ffi::sqlite3_create_function_v2(
            raw,
            name.as_ptr(),
            1, // nArg
            ffi::SQLITE_UTF8 | ffi::SQLITE_DETERMINISTIC,
            std::ptr::null_mut(), // pApp
            Some(normalize_url),
            None, // xStep
            None, // xFinal
            None, // xDestroy
        );
    }
}
