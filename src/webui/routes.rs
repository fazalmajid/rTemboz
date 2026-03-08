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
use crate::webui;
use actix_web::web;
use actix_web_static_files::ResourceFiles;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub fn configure(cfg: &mut web::ServiceConfig) {
    let generated = generate();
    cfg.service(web::redirect("/", "/view"))
        .service(ResourceFiles::new("/static", generated))
        .service(webui::login::login)
        .service(webui::view::view)
        .service(webui::feeds::feeds)
        .service(webui::feed::feed)
        .service(webui::updown::enqueue)
        .service(webui::add::add)
        .service(webui::rules::rules);
}
