use crate::{AhoyApi, Dataset, ErrorKind, Inverter, UnitValue};

use chrono::{DateTime, Local};

use std::{collections::HashMap, env, time::Duration};

#[derive(Debug, Clone)]
pub struct CrawledInverter {
    api: AhoyApi,
    original_inverter: Inverter,

    pub id: u8,       // InverterIndex.id or InverterStatus.id
    pub name: String, // InverterIndex.name or InverterStatus.name

    pub is_enabled: bool,   // InverterIndex.enabled or InverterStatus.enabled
    pub is_producing: bool, // InverterIndex.is_producing
    pub is_available: bool, // InverterIndex.is_avail

    pub crawled_at: Option<DateTime<Local>>,
    pub next_crawl_at: Option<DateTime<Local>>,
    pub crawling_interval: Option<Duration>,

    pub channel_count: u8, // Inverter.channels
    // pub channel_fields: Vec<EmptyField>, // live.fld_names combined with Inverter.fld_units
    // pub channel_values: Vec<Vec<Vec<Option<f32>>>>, // live.fld_values collected over time
    pub channel_datasets: Vec<Dataset>,

    // summary = channel 0
    // pub summary_fields: Vec<EmptyField>, // live.ch0_fld_names combined with Inverter.ch0_fld_units
    // pub summary_values: Vec<Vec<Option<f32>>>, // live.ch0_fld_values collected over time
    pub summary_dataset: Dataset,
}

impl CrawledInverter {
    pub async fn fetch(api: &AhoyApi, index: u8) -> Result<Self, ErrorKind> {
        let inverter_index = &api.get_index().await?.inverter[index as usize];
        let inverter = &api.get_inverter_list().await?.inverter[index as usize];
        let live = &api.get_live().await?;

        Ok(CrawledInverter {
            api: api.clone(),
            original_inverter: inverter.clone(),

            id: inverter.id,
            name: inverter.name.clone(),

            is_enabled: inverter_index.enabled,
            is_producing: inverter_index.is_producing,
            is_available: inverter_index.is_avail,

            crawled_at: None,
            next_crawl_at: None,
            crawling_interval: None,

            channel_count: inverter.channels,
            channel_datasets: (0..inverter.channels)
                .map(|_| Dataset::new(&live.fld_names, &live.fld_units))
                .collect(),
            summary_dataset: Dataset::new(&live.ch0_fld_names, &live.ch0_fld_units),
        })
    }

    pub async fn save_to_csv(&mut self, folder_path: &str) -> Result<(), ErrorKind> {
        for (channel_index, dataset) in self.channel_datasets.iter_mut().enumerate() {
            dataset.save_to_csv(folder_path, &self.name, channel_index as u8)?;
        }
        self.summary_dataset
            .save_to_csv(folder_path, &self.name, "summary")?;
        Ok(())
    }

    pub fn is_due(&self) -> bool {
        match self.next_crawl_at {
            Some(next_crawl_at) => next_crawl_at < Local::now(),
            None => true,
        }
    }

    pub async fn crawl(&mut self) -> Result<(), ErrorKind> {
        let default_interval = Duration::from_secs(
            env::var("CRAWLING_INTERVAL")
                .unwrap_or("60".to_string())
                .parse::<u64>()
                .unwrap_or(60),
        );
        log::info!("Crawling Inverter: {}", self.id);
        let fields: Vec<HashMap<String, UnitValue<f32>>> = self
            .api
            .get_inverter_fields(self.original_inverter.clone(), None)
            .await?;

        let interval = self.crawling_interval.unwrap_or(default_interval);

        let crawling_time = Local::now();
        self.crawled_at = Some(crawling_time);
        self.next_crawl_at = Some(Local::now() + interval);
        self.crawling_interval = Some(interval);

        self.summary_dataset.insert_row(&fields[0], &crawling_time);

        for channel_index in 1..=self.channel_count {
            self.channel_datasets[channel_index as usize - 1]
                .insert_row(&fields[channel_index as usize], &crawling_time)
        }
        Ok(())
    }
}
