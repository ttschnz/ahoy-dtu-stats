use super::utils::create_file_with_full_path;
use crate::{ahoy::UnitValue, error_kind::ErrorKind, EmptyField};

use chrono::{DateTime, Local};
use csv::Writer;

use serde::{Deserialize, Serialize};
#[cfg(feature = "db")]
use sqlx::{mysql::MySqlArguments, query::Query, MySql, MySqlPool};
#[cfg(feature = "db")]
use std::any::type_name;

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
        channel_index: impl ToString,
    ) -> Result<(), ErrorKind> {
        let csv_path = format!(
            "{}/{}/{}.csv",
            folder_path,
            inverter_name,
            channel_index.to_string()
        );
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

    #[cfg(feature = "db")]
    pub async fn save_to_db<T: ToString>(
        &mut self,
        db_pool: &MySqlPool,
        inverter_name: &str,
        channel_index: T,
    ) -> Result<(), ErrorKind> {
        // if channel_index is u8, then it is a channel, otherwise it is a summary

        let table_name = format!("{}::{}", inverter_name, channel_index.to_string());
        let fields = if type_name::<T>() == "u8" {
            vec![
                ("timestamp", "timestamp"),
                ("U_DC", "float"),
                ("I_DC", "float"),
                ("P_DC", "float"),
                ("YieldDay", "float"),
                ("YieldTotal", "float"),
                ("Irradiation", "float"),
                ("MaxPower", "float"),
            ]
        } else {
            vec![
                ("timestamp", "timestamp"),
                ("U_AC", "float"),
                ("I_AC", "float"),
                ("P_AC", "float"),
                ("F_AC", "float"),
                ("PF_AC", "float"),
                ("Temp", "float"),
                ("YieldTotal", "float"),
                ("YieldDay", "float"),
                ("P_DC", "float"),
                ("Efficiency", "float"),
                ("Q_AC", "float"),
                ("MaxPower", "float"),
            ]
        };

        let create_table_query = format!(
            "CREATE TABLE IF NOT EXISTS ? (
                {},
                PRIMARY KEY (`{}`)
            ) ENGINE=InnoDB DEFAULT CHARSET=utf8;",
            fields
                .iter()
                .map(|(name, datatype)| format!("`{}` {} NOT NULL", name, datatype))
                .collect::<Vec<_>>()
                .join(",\n"),
            fields[0].0
        );

        sqlx::query(&create_table_query)
            .bind(&table_name)
            .execute(db_pool)
            .await
            .map_err(|err| ErrorKind::CouldNotWriteToDB(err.to_string()))?;

        let insert = format!(
            "INSERT INTO ? ({}) VALUES ({})",
            fields
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>()
                .join(", "),
            self.values
                .iter()
                .map(|_| format!(
                    "({})",
                    fields.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
                ))
                .collect::<Vec<_>>()
                .join(", ")
        );
        let mut query: Query<'_, MySql, MySqlArguments> = sqlx::query(&insert).bind(&table_name);

        while let Some((row, datetime)) = self.values.first() {
            query = query.bind(datetime.format("%F %T").to_string());
            for value in row {
                query = query.bind(value.clone());
            }
            self.values.drain(..1);
        }

        query
            .execute(db_pool)
            .await
            .map_err(|err| ErrorKind::CouldNotWriteToDB(err.to_string()))?;

        Ok(())
    }
}
