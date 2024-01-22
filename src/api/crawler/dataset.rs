use super::utils::create_file_with_full_path;
use crate::{ahoy::UnitValue, error_kind::ErrorKind, EmptyField};

use chrono::{DateTime, Local};
use csv::Writer;

use serde::{Deserialize, Serialize};

use std::{collections::HashMap, iter::once};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    fields: Vec<EmptyField>,
    values: Vec<(Vec<Option<f32>>, DateTime<Local>)>,
}

impl Dataset {
    pub fn new(field_names: &[String], field_units: &[String]) -> Self {
        let mut fields = Vec::new();
        for (index, fieldname) in field_names.iter().enumerate() {
            fields.push(EmptyField {
                name: fieldname.clone(),
                unit: field_units[index].clone(),
            });
        }

        Self {
            fields,
            values: Vec::new(),
        }
    }

    pub fn insert_row(
        &mut self,
        data: &HashMap<String, UnitValue<f32>>,
        timestamp: &DateTime<Local>,
    ) {
        let mut new_row = Vec::new();
        for key in &self.fields {
            new_row.push(data.get(&key.name).map(|entry| entry.value));
        }
        self.values.push((new_row, *timestamp));
    }

    pub fn save_to_csv(
        &mut self,
        folder_path: &str,
        inverter_name: &str,
        channel_index: u8,
    ) -> Result<(), ErrorKind> {
        let csv_path = format!("{}/{}/{}.csv", folder_path, inverter_name, channel_index);
        let file = create_file_with_full_path(csv_path, true, true)?;
        let metadata = file
            .metadata()
            .map_err(|err| ErrorKind::CouldNotWriteToCsv(err.to_string()))?;

        let mut writer = Writer::from_writer(file);

        if metadata.len() == 0 {
            writer
                .write_record(
                    once(&"timestamp".to_string())
                        .chain(self.fields.iter().map(|field| &field.name)),
                )
                .map_err(|err| ErrorKind::CouldNotWriteToCsv(err.to_string()))?;
        }
        while let Some((row, datetime)) = self.values.first() {
            writer
                .write_record(
                    once(datetime.format("%F %T").to_string()).chain(
                        row.iter()
                            .map(|value| match value {
                                Some(value) => value.to_string(),
                                None => "".to_string(),
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
                .map_err(|err| ErrorKind::CouldNotWriteToCsv(err.to_string()))?;
            self.values.drain(..1);
        }

        Ok(())
    }
}
