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
use crate::feeds::worker::{feed_worker_setup, spawn_worker};
use actix_web::{web, App, HttpServer};
use clap::{Parser, Subcommand};
use log::{error, info};

mod db;
mod feeds;
mod filter;
mod webui;

const BIND_ADDRESS: &str = "0.0.0.0:9998";

#[derive(Parser)]
#[command(name = "rtemboz")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}
#[derive(Subcommand)]
enum Commands {
    Serve,
    Dump { url: String },
    Refresh,
    Rebuild,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // colog::init();
    colog::default_builder()
        // disable annoying "foster parenting not implemented" warnings
        .filter_module("html5ever", log::LevelFilter::Error)
        // disable annoying "scan_bytes;" warnings
        .filter_module("hyperscan_tokio", log::LevelFilter::Warn)
        .init();

    let cli = Cli::parse();

    match cli.command.unwrap_or(Commands::Serve) {
        Commands::Serve => serve().await,
        Commands::Dump { url } => dump(url).await,
        Commands::Refresh => refresh().await,
        Commands::Rebuild => rebuild().await,
    }
}

async fn serve() -> std::io::Result<()> {
    info!("setting up database");
    let db = db::create_db().await;
    // DB worker
    let (work_q, _) = db::worker::spawn(&db);
    // Feeds worker
    let (feed_work_q, _) = spawn_worker(db.clone(), work_q.clone());
    // Web server
    info!("starting web server");
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(db.clone()))
            .app_data(web::Data::new(work_q.clone()))
            .app_data(web::Data::new(feed_work_q.clone()))
            .configure(webui::routes::configure)
    })
    .bind(BIND_ADDRESS)?
    .run()
    .await
}

async fn dump(url: String) -> std::io::Result<()> {
    match crate::feeds::worker::dump(url).await {
        Err(e) => {
            error!("{}", e);
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn rebuild() -> std::io::Result<()> {
    info!("setting up database");
    let db = db::create_db().await;
    info!("rebuilding materialized views");
    db::views::rebuild(&db).await.unwrap();
    Ok(())
}

async fn refresh() -> std::io::Result<()> {
    info!("setting up database");
    let db = db::create_db().await;
    let (bf, filters) = feed_worker_setup(&db).await;
    let (work_q, worker_handle) = db::worker::spawn(&db);
    feeds::worker::fetch_all(&db, work_q.clone(), bf, filters).await;
    let _ = work_q.send(db::worker::DbOp::Quit);
    // Wait for the worker to finish processing all queued messages
    let _ = worker_handle.join();
    Ok(())
}
