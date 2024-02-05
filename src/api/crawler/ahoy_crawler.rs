use crate::{AhoyApi, CrawledInverter, ErrorKind};

use chrono::{DateTime, Local};

#[cfg(feature = "db")]
use sqlx::{mysql::MySqlConnectOptions, MySqlPool};
use std::{
    collections::{hash_map::Entry, HashMap},
    env,
};

pub struct Crawler {
    api: AhoyApi,
    pub inverters: HashMap<u8, CrawledInverter>,
}
impl From<AhoyApi> for Crawler {
    fn from(api: AhoyApi) -> Self {
        Crawler {
            api,
            inverters: HashMap::new(),
        }
    }
}

impl Crawler {
    pub fn new(endpoint: String) -> Crawler {
        Crawler {
            api: AhoyApi::new(endpoint),
            inverters: HashMap::new(),
        }
    }

    /// Initialize the crawler by fetching all inverters from the API and
    /// creating a CrawledInverter for each of them.
    pub async fn init(&mut self) -> Result<(), ErrorKind> {
        let inverter_list = self.api.get_inverter_list().await?;
        for inverter in inverter_list.inverter {
            let crawled_inverter = CrawledInverter::fetch(&self.api, inverter.id).await?;
            self.inverters.insert(inverter.id, crawled_inverter);
        }
        Ok(())
    }

    async fn get_inverter(&mut self, inverter_id: u8) -> Result<&mut CrawledInverter, ErrorKind> {
        match self.inverters.entry(inverter_id) {
            Entry::Vacant(entry) => {
                log::info!("Initiating Inverter: {}", inverter_id);
                Ok(entry.insert(CrawledInverter::fetch(&self.api, inverter_id).await?))
            }
            Entry::Occupied(entry) => Ok(entry.into_mut()),
        }
    }

    pub async fn crawl_inverter(&mut self, inverter_id: u8) -> Result<(), ErrorKind> {
        let inverter = self.get_inverter(inverter_id).await?;
        inverter.crawl().await
    }

    pub async fn save_to_csv(&mut self, folder_path: &str) -> Result<(), ErrorKind> {
        for inverter in self.inverters.values_mut() {
            inverter.save_to_csv(folder_path).await?;
        }
        Ok(())
    }

    #[cfg(feature = "db")]
    pub async fn save_to_db(&mut self) -> Result<(), ErrorKind> {
        let db_url = env::var("DB_URL").expect("FATAL: DB_URL must be set.");
        let mut connection = MySqlPool::connect(&db_url)
            .await
            .map_err(|e| ErrorKind::DBConnectionFailed(e.to_string()))?;
        for inverter in self.inverters.values_mut() {
            inverter.save_to_db(&mut connection).await?;
        }
        Ok(())
    }

    pub async fn crawl_all_due_inverters(
        &mut self,
        sync_to_db: bool,
    ) -> Result<Option<DateTime<Local>>, ErrorKind> {
        #[cfg(not(feature = "db"))]
        let out_dir = env::var("OUT_DIR").unwrap_or("./out".to_string());

        #[cfg(feature = "db")]
        let mut connection = {
            let db_host = env::var("DB_HOST").expect("FATAL: DB_URL must be set.");
            let db_name = env::var("DB_NAME").expect("FATAL: DB_NAME must be set.");
            let db_user = env::var("DB_USER").expect("FATAL: DB_USER must be set.");
            let db_pass = env::var("DB_PASS").expect("FATAL: DB_PASS must be set.");
            let db_port = env::var("DB_PORT").expect("FATAL: DB_PORT must be set.");

            let options = MySqlConnectOptions::new()
                .host(&db_host)
                .database(&db_name)
                .username(&db_user)
                .password(&db_pass)
                .port(db_port.parse::<u16>().unwrap());
            MySqlPool::connect_with(options).await
        }
        .map_err(|e| ErrorKind::DBConnectionFailed(e.to_string()))?;

        let mut due_inverters = vec![];
        let mut next_due: Option<DateTime<Local>> = None;
        for (index, inverter) in &self.inverters {
            if inverter.is_due() {
                due_inverters.push(*index);
            }
        }
        for inverter_id in due_inverters {
            let inverter = self.get_inverter(inverter_id).await?;
            inverter.crawl().await?;

            if sync_to_db {
                #[cfg(feature = "db")]
                inverter.save_to_db(&mut connection).await?;
                #[cfg(not(feature = "db"))]
                inverter.save_to_csv(&out_dir).await?;
            }
            if let Some(next_crawl) = inverter.next_crawl_at {
                next_due = Some(next_due.map_or(next_crawl, |v| v.min(next_crawl)));
            }
        }

        Ok(next_due)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use dotenv::dotenv;
    use std::env;

    static IS_OFFLINE: bool = true;

    fn init() -> AhoyApi {
        dotenv().ok();

        let endpoint = env::var("INVERTER_ENDPOINT").unwrap();

        let mut api = AhoyApi::new(endpoint);
        api.set_offline_mode(IS_OFFLINE);
        api
    }

    #[tokio::test]
    async fn crawl_inverter() {
        let mut crawler = Crawler::from(init());
        crawler.crawl_inverter(0).await.unwrap();
        println!("{:#?}", crawler.inverters);
    }

    #[tokio::test]
    async fn crawl_inverter_and_save() {
        let mut crawler = Crawler::from(init());
        crawler.crawl_inverter(0).await.unwrap();
        crawler.crawl_inverter(0).await.unwrap();
        crawler.save_to_csv("./out").await.unwrap();
        crawler.crawl_inverter(0).await.unwrap();
        crawler.save_to_csv("./out").await.unwrap();
        crawler.crawl_inverter(0).await.unwrap();
        crawler.save_to_csv("./out").await.unwrap();
        crawler.crawl_inverter(0).await.unwrap();
    }
}
