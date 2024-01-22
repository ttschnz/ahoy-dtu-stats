use crate::ErrorKind;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::from_str;
use std::collections::HashMap;
use std::env;

#[derive(Debug, Clone)]
pub struct AhoyApi {
    endpoint: String,
    #[cfg(test)]
    offline_mode: bool,
}

impl AhoyApi {
    pub fn from_env() -> Result<Self, ErrorKind> {
        let endpoint = env::var("INVERTER_ENDPOINT").map_err(|_| ErrorKind::EnvVarError)?;
        Ok(Self::new(endpoint))
    }

    #[cfg(not(test))]
    pub fn new(endpoint: String) -> Self {
        Self { endpoint }
    }

    #[cfg(test)]
    pub fn new(endpoint: String) -> Self {
        Self {
            endpoint,
            offline_mode: false,
        }
    }
    #[cfg(test)]
    pub fn set_offline_mode(&mut self, value: bool) {
        self.offline_mode = value;
    }

    async fn _request(&self, path: String) -> Result<String, ErrorKind> {
        let client = Client::new();
        let url = format!("{}{}", self.endpoint, path);
        log::info!("requesting {}", url);
        let res = client
            .get(&url)
            .send()
            .await
            .map_err(|_| ErrorKind::NetworkError)?;
        res.text().await.map_err(|_| ErrorKind::NetworkError)
    }

    #[cfg(test)]
    async fn request(&self, path: String) -> Result<String, ErrorKind> {
        if self.offline_mode {
            Ok(match path.as_str() {
                "/api/inverter/list" => "{\"inverter\":[{\"enabled\":true,\"id\":0,\"name\":\"PV Microinverte\",\"serial\":\"114184511809\",\"channels\":2,\"version\":\"10010\",\"ch_yield_cor\":[0,0],\"ch_name\":[\"A\",\"B\"],\"ch_max_pwr\":[540,540]}],\"interval\":\"30\",\"retries\":\"5\",\"max_num_inverters\":4,\"rstMid\":false,\"rstNAvail\":false,\"rstComStop\":false,\"strtWthtTm\":false,\"yldEff\":1}",
                "/api/inverter/id/0" => "{\"id\":0,\"enabled\":true,\"name\":\"PV Microinverte\",\"serial\":\"114184511809\",\"version\":\"10010\",\"power_limit_read\":65535,\"power_limit_ack\":false,\"ts_last_success\":1705764469,\"generation\":0,\"status\":0,\"alarm_cnt\":3,\"ch\":[[239.7,0,0,49.97,0,1.9,298.886,37,1,0,0,475.3],[23.2,0.02,0.5,18,148.505,0.093,241.1],[23.2,0.02,0.5,19,150.381,0.093,262]],\"ch_name\":[\"AC\",\"A\",\"B\"],\"ch_max_pwr\":[null,540,540]}",
                "/api/live" => "{\"generic\":{\"wifi_rssi\":-68,\"ts_uptime\":1860548,\"ts_now\":1705817096,\"version\":\"0.7.36\",\"build\":\"ba218ed\",\"menu_prot\":false,\"menu_mask\":61,\"menu_protEn\":false,\"esp_type\":\"ESP8266\"},\"refresh\":30,\"ch0_fld_units\":[\"V\",\"A\",\"W\",\"Hz\",\"\",\"°C\",\"kWh\",\"Wh\",\"W\",\"%\",\"var\",\"W\"],\"ch0_fld_names\":[\"U_AC\",\"I_AC\",\"P_AC\",\"F_AC\",\"PF_AC\",\"Temp\",\"YieldTotal\",\"YieldDay\",\"P_DC\",\"Efficiency\",\"Q_AC\",\"MaxPower\"],\"fld_units\":[\"V\",\"A\",\"W\",\"Wh\",\"kWh\",\"%\",\"W\"],\"fld_names\":[\"U_DC\",\"I_DC\",\"P_DC\",\"YieldDay\",\"YieldTotal\",\"Irradiation\",\"MaxPower\"],\"iv\":[true,false,false,false]}",
                "/api/index" => "{\"generic\":{\"wifi_rssi\":-68,\"ts_uptime\":1860564,\"ts_now\":1705817112,\"version\":\"0.7.36\",\"build\":\"ba218ed\",\"menu_prot\":false,\"menu_mask\":61,\"menu_protEn\":false,\"esp_type\":\"ESP8266\"},\"ts_now\":1705817112,\"ts_sunrise\":1705820785,\"ts_sunset\":1705853693,\"ts_offset\":0,\"disNightComm\":true,\"inverter\":[{\"enabled\":true,\"id\":0,\"name\":\"PV Microinverte\",\"version\":\"10010\",\"is_avail\":false,\"is_producing\":false,\"ts_last_success\":1705764469}],\"warnings\":[],\"infos\":[]}",
                _ => "",
            }
            .to_string())
        } else {
            self._request(path).await
        }
    }

    #[cfg(not(test))]
    async fn request(&self, path: String) -> Result<String, ErrorKind> {
        self._request(path).await
    }

    pub async fn get_inverter_fields(
        &self,
        inverter: Inverter,
        selected_fields: Option<Vec<String>>,
    ) -> Result<Vec<HashMap<String, UnitValue<f32>>>, ErrorKind> {
        let inverter_channel_count = inverter.channels as usize;
        let inverter_status = self.get_inverter_status(inverter).await?;
        let live = self.get_live().await?;
        let mut data = Vec::new();

        // channel 0 is a special case, i assume it is the sum of all channels, maybe the values of the inverter itself
        let mut channel_0 = HashMap::new();
        for (index, fieldname) in live.ch0_fld_names.iter().enumerate() {
            if let Some(selected_fields) = &selected_fields {
                if !selected_fields.contains(fieldname) {
                    continue;
                }
            }
            channel_0.insert(
                fieldname.clone(),
                UnitValue::new(
                    inverter_status.ch[0][index],
                    live.ch0_fld_units[index].clone(),
                ),
            );
        }
        data.push(channel_0);

        for channel in 1..=inverter_channel_count {
            let mut channel_data = HashMap::new();
            for (index, fieldname) in live.fld_names.iter().enumerate() {
                if let Some(selected_fields) = &selected_fields {
                    if !selected_fields.contains(fieldname) {
                        continue;
                    }
                }
                channel_data.insert(
                    fieldname.clone(),
                    UnitValue::new(
                        inverter_status.ch[channel][index],
                        live.fld_units[index].clone(),
                    ),
                );
            }
            data.push(channel_data);
        }

        Ok(data)
    }

    pub async fn get_inverter_status(
        &self,
        inverter: Inverter,
    ) -> Result<InverterStatus, ErrorKind> {
        let path = format!("/api/inverter/id/{}", inverter.id);
        let res = self.request(path).await?;
        from_str(&res).map_err(|_| ErrorKind::ParsingError)
    }

    pub async fn get_inverter_list(&self) -> Result<InverterList, ErrorKind> {
        let path = "/api/inverter/list".to_string();
        let res = self.request(path).await?;
        from_str(&res).map_err(|_| ErrorKind::ParsingError)
    }

    pub async fn get_live(&self) -> Result<Live, ErrorKind> {
        let path = "/api/live".to_string();
        let res = self.request(path).await?;
        from_str(&res).map_err(|_| ErrorKind::ParsingError)
    }

    pub async fn get_index(&self) -> Result<Index, ErrorKind> {
        let path = "/api/index".to_string();
        let res = self.request(path).await?;
        from_str(&res).map_err(|_| ErrorKind::ParsingError)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InverterList {
    pub inverter: Vec<Inverter>,
    pub interval: String,
    pub retries: String,
    pub max_num_inverters: u8,
    #[serde(rename = "rstMid")]
    pub rst_mid: bool,
    #[serde(rename = "rstNAvail")]
    pub rst_n_avail: bool,
    #[serde(rename = "rstComStop")]
    pub rst_com_stop: bool,
    #[serde(rename = "strtWthtTm")]
    pub strt_wtht_tm: bool,
    #[serde(rename = "yldEff")]
    pub yld_eff: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Inverter {
    pub enabled: bool,
    pub id: u8,
    pub name: String,
    pub serial: String,
    pub channels: u8, // amount of channels -> used for naming in live
    pub version: String,
    pub ch_yield_cor: Vec<u8>,
    pub ch_name: Vec<String>,
    pub ch_max_pwr: Vec<Option<u16>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InverterStatus {
    pub id: u8,
    pub enabled: bool,
    pub name: String,
    pub serial: String,
    pub version: String,
    pub power_limit_read: u16,
    pub power_limit_ack: bool,
    pub ts_last_success: u64,
    pub generation: u32,
    pub status: u8,
    pub alarm_cnt: u8,
    pub ch: Vec<Vec<f32>>,
    pub ch_name: Vec<String>,
    pub ch_max_pwr: Vec<Option<u16>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Live {
    pub generic: Generic,
    pub refresh: u8,
    pub ch0_fld_units: Vec<String>,
    pub ch0_fld_names: Vec<String>,
    pub fld_units: Vec<String>,
    pub fld_names: Vec<String>,
    pub iv: Vec<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Generic {
    pub wifi_rssi: i8,
    pub ts_uptime: u64,
    pub ts_now: u64,
    pub version: String,
    pub build: String,
    pub menu_prot: bool,
    pub menu_mask: u8,
    #[serde(rename = "menu_protEn")]
    pub menu_prot_en: bool,
    pub esp_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UnitValue<T> {
    pub value: T,
    pub unit: String,
}
impl UnitValue<f32> {
    pub fn new(value: f32, unit: String) -> Self {
        Self { value, unit }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Index {
    pub generic: Generic,
    pub ts_now: u64,
    pub ts_sunrise: u64,
    pub ts_sunset: u64,
    pub ts_offset: u64,
    #[serde(rename = "disNightComm")]
    pub dis_night_comm: bool,
    pub inverter: Vec<InverterIndex>,
    pub warnings: Vec<String>,
    pub infos: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InverterIndex {
    pub enabled: bool,
    pub id: u8,
    pub name: String,
    pub version: String,
    pub is_avail: bool,
    pub is_producing: bool,
    pub ts_last_success: u64,
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // this is needed to run tests in parallel
    lazy_static::lazy_static! {
        static ref TEST_MUTEX: Arc<Mutex<()>> = Arc::new(Mutex::new(()));
    }

    static IS_OFFLINE: bool = true;
    fn init() -> Result<AhoyApi, ErrorKind> {
        dotenv::dotenv().ok();
        let mut api = AhoyApi::from_env()?;
        api.set_offline_mode(IS_OFFLINE);
        Ok(api)
    }

    #[tokio::test]
    async fn get_inverter_status() {
        let _guard = TEST_MUTEX.lock().await;

        let api = init().unwrap();
        let mut inverter_list = api.get_inverter_list().await.unwrap();
        // pretty print
        println!("{:#?}", inverter_list);
        let res = api
            .get_inverter_status(inverter_list.inverter.pop().unwrap())
            .await;
        println!("{:#?}", res);
    }

    #[tokio::test]
    async fn get_live() {
        let _guard = TEST_MUTEX.lock().await;

        let api = init().unwrap();
        let res = api.get_live().await;
        println!("{:#?}", res);
    }

    #[tokio::test]
    async fn get_inverter_fields() {
        let _guard = TEST_MUTEX.lock().await;

        let api = init().unwrap();
        let mut inverter_list = api.get_inverter_list().await.unwrap();
        let res = api
            .get_inverter_fields(
                inverter_list.inverter.pop().unwrap(),
                Some(vec!["YieldDay".to_string()]),
            )
            .await;
        println!("{:#?}", res);
    }

    #[tokio::test]
    async fn get_index() {
        let _guard = TEST_MUTEX.lock().await;

        let api = init().unwrap();
        let res = api.get_index().await;
        println!("{:#?}", res);
    }
}
