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
#[derive(Debug)]
pub struct MenuItem<'a> {
    pub name: &'a str,
    pub link: &'a str,
    pub new_window: bool,
}

pub fn menus(me: &str) -> Vec<MenuItem<'_>> {
    let mut result = Vec::with_capacity(4);

    if me != "view" {
        result.push(MenuItem {
            name: "All unread",
            link: "view",
            new_window: false,
        })
    }
    if me != "feeds" {
        result.push(MenuItem {
            name: "All feeds",
            link: "feeds",
            new_window: true,
        })
    }
    if me != "add" {
        result.push(MenuItem {
            name: "Add feed",
            link: "add",
            new_window: true,
        })
    }
    if me != "rules" {
        result.push(MenuItem {
            name: "Filters",
            link: "rules",
            new_window: true,
        })
    }
    result
}
