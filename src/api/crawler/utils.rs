use crate::{AhoyApi, Crawler, ErrorKind};

use chrono::Local;

use dotenv::dotenv;
use env_logger::{Builder, Target};
use log::{debug, error, info, warn, LevelFilter};

use tokio::time::{sleep_until, Instant};

use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    time::Duration,
};

pub(crate) fn create_file_with_full_path(
    file_path: String,
    write: bool,
    append: bool,
) -> Result<File, ErrorKind> {
    let path = Path::new(&file_path);

    if !path.exists() {
        let parent = path
            .parent()
            .ok_or(ErrorKind::CouldNotCreateFolder(file_path.clone()))?;

        fs::create_dir_all(parent)
            .map_err(|_| ErrorKind::CouldNotCreateFolder(file_path.clone()))?;

        File::create(&file_path).map_err(|_| ErrorKind::CouldNotCreateFile(file_path.clone()))?;
    }

    OpenOptions::new()
        .write(write)
        .append(append)
        .open(&file_path)
        .map_err(|e| {
            env_logger::init();
            error!("Error opening log file: {:?}, logging to stdout instead", e);
            ErrorKind::CouldNotOpenFile(file_path.clone())
        })
}

#[cfg(not(test))]
pub async fn entrypoint() -> Result<(), ErrorKind> {
    _entrypoint(false).await
}
#[cfg(test)]
pub async fn entrypoint(offline: bool) -> Result<(), ErrorKind> {
    _entrypoint(offline).await
}

async fn _entrypoint(_offline: bool) -> Result<(), ErrorKind> {
    dotenv().ok();

    match env::var("LOGGING_TARGET") {
        Ok(target) => {
            info!("Logging to {}", target);
            if target == "stdout" {
                env_logger::init();
            } else if let Ok(target_file) = create_file_with_full_path(target, true, true) {
                let boxed = Box::new(target_file);
                Builder::new()
                    .target(Target::Pipe(boxed))
                    .filter(None, LevelFilter::Debug)
                    .format(|buf, record| {
                        writeln!(
                            buf,
                            "[{} {} {}:{}] {}",
                            Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                            record.level(),
                            record.module_path().unwrap_or("unknown module"),
                            record.line().unwrap_or(0),
                            record.args()
                        )
                    })
                    .init();
            } else {
                env_logger::init();
                error!("Error opening log file, logging to stdout instead");
            }
        }
        Err(_) => {
            env_logger::init();
            error!("No LOGGING_TARGET specified, logging to stdout");
        }
    }
    // if LOGGING_TARGET == "stdout" -> log to stdout
    #[cfg(feature = "db")]
    info!("outputting to database");
    #[cfg(not(feature = "db"))]
    info!("outputting to csv files");

    info!("Starting crawler");

    match AhoyApi::from_env() {
        // the mut is used in tests to allow offline mode
        #[allow(unused_mut)]
        Ok(mut api) => {
            info!("API configured");

            #[cfg(test)]
            api.set_offline_mode(_offline);

            let default_interval = Duration::from_secs(
                env::var("CRAWLING_INTERVAL")
                    .unwrap_or("60".to_string())
                    .parse::<u64>()
                    .unwrap_or(60),
            );

            let mut crawler = Crawler::from(api);

            loop {
                if crawler.init().await.is_ok() {
                    break;
                } else {
                    error!("Error initializing crawler");
                    tokio::time::sleep(default_interval).await;
                }
            }

            let mut next_sync: u8 = 4;
            info!("Crawler initialized");
            loop {
                match crawler.crawl_all_due_inverters(next_sync == 0).await {
                    Ok(Some(closest_due)) => {
                        if let Ok(sleep_duration) = (closest_due - Local::now()).to_std() {
                            debug!(
                                "Successfully crawled all due inverters, sleeping {:?}",
                                sleep_duration
                            );

                            sleep_until(Instant::now() + sleep_duration).await;
                        } else {
                            warn!("negative duration! sleeping for 1 minute");
                            tokio::time::sleep(default_interval).await;
                        }
                    }
                    Ok(None) => {
                        warn!("No next due inverters found, sleeping for 1 minute");
                        tokio::time::sleep(default_interval).await;
                    }
                    Err(e) => error!("Error: {:?}", e),
                }
                if next_sync == 0 {
                    next_sync = 4;
                } else {
                    next_sync -= 1;
                }
            }
        }
        Err(e) => {
            error!("Error generating Api: {:?}", e);
            Err(e)
        }
    }
}

#[cfg(all(test, feature = "db"))]
mod test {
    use std::env;
    use testcontainers::clients;
    use testcontainers_modules::mysql;

    use crate::{AhoyApi, Crawler};

    #[tokio::test]
    async fn test_db_store() {
        env_logger::init();
        let docker = clients::Cli::default();
        let mysql_instance = docker.run(mysql::Mysql::default());

        let port = mysql_instance.get_host_port_ipv4(3306);

        env::set_var("DB_HOST", "localhost");
        env::set_var("DB_PORT", port.to_string());
        env::set_var("DB_USER", "root");
        // env::set_var("DB_PASS", "");
        env::set_var("DB_NAME", "test");

        let mut api = AhoyApi::new("some_host".to_string());
        api.set_offline_mode(true);
        let mut crawler = Crawler::from(api);

        crawler.init().await.unwrap();

        crawler.crawl_all_due_inverters(true).await.unwrap();
    }
}
