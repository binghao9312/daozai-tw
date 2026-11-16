//! 台灣中央氣象署 (CWA) 開放資料平台 API 客戶端
//! 支援：一般天氣預報-今明 36 小時 (F-C0032-001)、現在天氣觀測 (O-A0003-001)、鄉鎮未來 1 週 (F-D0047-091)

use crate::weather::{Day, Position, Snapshot, SunTimes};
use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}, mpsc};
use std::sync::mpsc::Receiver;
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

/// CWA 開放資料平台基礎 URL
const CWA_BASE_URL: &str = "https://opendata.cwa.gov.tw/api/v1/rest/datastore";

/// 資料集代碼
const DATASET_36HR: &str = "F-C0032-001";   // 一般天氣預報-今明 36 小時
const DATASET_OBS: &str = "O-A0003-001";    // 現在天氣觀測報告
const DATASET_WEEK: &str = "F-D0047-091";   // 鄉鎮未來 1 週天氣預報

/// CWA 錯誤類型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CwaError {
    /// 未設定授權碼
    NoKey,
    /// 授權碼無效
    InvalidKey,
    /// 請求參數無效
    InvalidParam,
    /// 請求頻率超限
    RateLimited,
    /// 月度額度用盡
    QuotaExceeded,
    /// 網路錯誤
    Network,
    /// 回應格式錯誤
    InvalidResponse,
    /// 非台灣地區
    OutOfRegion,
    /// 已停用
    Disabled,
}

impl std::fmt::Display for CwaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CwaError::NoKey => write!(f, "尚未設定 CWA 授權碼"),
            CwaError::InvalidKey => write!(f, "CWA 授權碼無效"),
            CwaError::InvalidParam => write!(f, "請求參數無效"),
            CwaError::RateLimited => write!(f, "請求過於頻繁，請稍後再試"),
            CwaError::QuotaExceeded => write!(f, "月度額度已用盡"),
            CwaError::Network => write!(f, "網路連線失敗"),
            CwaError::InvalidResponse => write!(f, "伺服器回應格式異常"),
            CwaError::OutOfRegion => write!(f, "CWA 僅提供台灣地區天氣資料"),
            CwaError::Disabled => write!(f, "CWA 天氣來源已停用"),
        }
    }
}

impl std::error::Error for CwaError {}

/// CWA 回應結構
#[derive(Debug, Deserialize)]
struct CwaResponse<T> {
    success: Option<String>,
    result: Option<CwaResult<T>>,
    records: Option<CwaRecords<T>>,
}

#[derive(Debug, Deserialize)]
struct CwaResult<T> {
    resource_id: String,
    fields: Option<Vec<serde_json::Value>>,
    records: Option<T>,
}

#[derive(Debug, Deserialize)]
struct CwaRecords<T> {
    #[serde(flatten)]
    data: T,
}

/// 36 小時預報資料結構
#[derive(Debug, Deserialize, Default)]
struct Forecast36hrRecords {
    locations: Vec<ForecastLocation>,
}

#[derive(Debug, Deserialize)]
struct ForecastLocation {
    location_name: String,
    weather_element: Vec<WeatherElement>,
}

#[derive(Debug, Deserialize)]
struct WeatherElement {
    element_name: String, // Wx, PoP, MinT, CI, MaxT
    time: Vec<TimeSlot>,
}

#[derive(Debug, Deserialize)]
struct TimeSlot {
    start_time: String,
    end_time: String,
    parameter: Parameter,
}

#[derive(Debug, Deserialize)]
struct Parameter {
    parameter_name: String,
    parameter_value: Option<String>,
    parameter_unit: Option<String>,
}

/// 觀測資料結構
#[derive(Debug, Deserialize, Default)]
struct ObservationRecords {
    location: Vec<ObservationLocation>,
}

#[derive(Debug, Deserialize)]
struct ObservationLocation {
    location_name: String,
    lat: String,
    lon: String,
    station_id: String,
    time: ObservationTime,
    weather_element: Vec<ObservationElement>,
}

#[derive(Debug, Deserialize)]
struct ObservationTime {
    obs_time: String,
}

#[derive(Debug, Deserialize)]
struct ObservationElement {
    element_name: String,
    element_value: String,
}

/// 1 週預報資料結構
#[derive(Debug, Deserialize, Default)]
struct WeekForecastRecords {
    locations: Vec<WeekForecastLocation>,
}

#[derive(Debug, Deserialize)]
struct WeekForecastLocation {
    location_name: String,
    weather_element: Vec<WeekWeatherElement>,
}

#[derive(Debug, Deserialize)]
struct WeekWeatherElement {
    element_name: String,
    time: Vec<WeekTimeSlot>,
}

#[derive(Debug, Deserialize)]
struct WeekTimeSlot {
    start_time: String,
    end_time: String,
    element_value: Vec<WeekElementValue>,
}

#[derive(Debug, Deserialize)]
struct WeekElementValue {
    value: String,
    measures: Option<String>,
}

/// CWA 客戶端
#[derive(Clone)]
pub struct CwaClient {
    agent: Agent,
    api_key: String,
    gate: Arc<AtomicBool>,
}

impl CwaClient {
    pub fn new(api_key: String) -> Self {
        let gate = Arc::new(AtomicBool::new(true));
        let agent = AgentBuilder::new()
            .timeout_read(Duration::from_secs(10))
            .timeout_write(Duration::from_secs(10))
            .build();
        Self { agent, api_key, gate }
    }

    pub fn api_key(&self) -> &str {
        &self.api_key
    }

    fn check_gate(&self) -> Result<(), CwaError> {
        if !self.gate.load(Ordering::SeqCst) {
            return Err(CwaError::Disabled);
        }
        Ok(())
    }

    /// 建構請求 URL
    fn build_url(&self, dataset: &str, params: &[(&str, &str)]) -> url::Url {
        let mut url = url::Url::parse(&format!("{}/{}", CWA_BASE_URL, dataset)).unwrap();
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("Authorization", &self.api_key);
            query.append_pair("format", "JSON");
            for (k, v) in params {
                query.append_pair(k, v);
            }
        }
        url
    }
    /// 發送請求並解析回應
    async fn request<T: for<'de> Deserialize<'de> + Default>(&self, dataset: &str, params: &[(&str, &str)]) -> Result<T, CwaError> {
        self.check_gate()?;
        
        let url = self.build_url(dataset, params);
        
        let response = self.agent.get(url.as_str()).call().map_err(|_| CwaError::Network)?;
        
        let status = response.status();
        if status == 401 || status == 403 {
            return Err(CwaError::InvalidKey);
        }
        if status == 429 {
            return Err(CwaError::RateLimited);
        }
        if !(200..=299).contains(&status) {
            return Err(CwaError::Network);
        }

        let text = response.into_string().map_err(|_| CwaError::InvalidResponse)?;
        let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| CwaError::InvalidResponse)?;

        // 檢查 CWA 特定錯誤代碼
        if let Some(success) = value.get("success").and_then(|v| v.as_str()) {
            match success {
                "false" => {
                    if let Some(msg) = value.get("msg").and_then(|v| v.as_str()) {
                        if msg.contains("授權碼") || msg.contains("authorization") {
                            return Err(CwaError::InvalidKey);
                        }
                        if msg.contains("額度") || msg.contains("quota") {
                            return Err(CwaError::QuotaExceeded);
                        }
                    }
                    return Err(CwaError::InvalidResponse);
                }
                _ => {}
            }
        }

        let response_data: CwaResponse<T> = serde_json::from_value(value).map_err(|_| CwaError::InvalidResponse)?;
        
        if let Some(records) = response_data.records {
            Ok(records.data)
        } else if let Some(result) = response_data.result {
            Ok(result.records.unwrap_or_default())
        } else {
            Err(CwaError::InvalidResponse)
        }
    }

    /// 取得 36 小時預報
    pub async fn fetch_36hr_forecast(&self, county: &str) -> Result<Forecast36hrRecords, CwaError> {
        self.request(DATASET_36HR, &[("locationName", county)]).await
    }

    /// 取得現在天氣觀測
    pub async fn fetch_observation(&self) -> Result<ObservationRecords, CwaError> {
        self.request(DATASET_OBS, &[]).await
    }

    /// 取得 1 週預報
    pub async fn fetch_week_forecast(&self, county: &str) -> Result<WeekForecastRecords, CwaError> {
        self.request(DATASET_WEEK, &[("locationName", county)]).await
    }
    
    /// 同步取得所有天氣資料並建構 Snapshot
    pub fn fetch_all(&self, county: &str) -> Option<Snapshot> {
        use crate::weather::Position;
        
        // 建立阻塞式 agent
        let agent = AgentBuilder::new()
            .timeout_read(Duration::from_secs(15))
            .timeout_write(Duration::from_secs(15))
            .build();
        
        // 1. 取得 36 小時預報
        let url_36hr = {
            let mut url = url::Url::parse(&format!("{}/{}", CWA_BASE_URL, DATASET_36HR)).unwrap();
            url.query_pairs_mut()
                .append_pair("Authorization", &self.api_key)
                .append_pair("format", "JSON")
                .append_pair("locationName", county);
            url
        };
        
        let forecast_36hr = agent.get(url_36hr.as_str()).call().ok()?.into_string().ok()?;
        let forecast_36hr: Forecast36hrRecords = serde_json::from_str(&forecast_36hr).ok()?;
        let parsed_36hr = parse_36hr_forecast(&forecast_36hr);
        if parsed_36hr.is_empty() { return None; }
        let forecast = &parsed_36hr[0];
        
        // 2. 取得觀測資料
        let url_obs = {
            let mut url = url::Url::parse(&format!("{}/{}", CWA_BASE_URL, DATASET_OBS)).unwrap();
            url.query_pairs_mut()
                .append_pair("Authorization", &self.api_key)
                .append_pair("format", "JSON");
            url
        };
        
        let observation_data = agent.get(url_obs.as_str()).call().ok()?.into_string().ok()?;
        let observation_records: ObservationRecords = serde_json::from_str(&observation_data).ok()?;
        let position = get_county_position(county).unwrap_or(Position{
            lat: 23.97, lon: 120.98, label: county.into(), region: "台灣".into()
        });
        let parsed_obs = parse_observation(&observation_records, position.lat, position.lon);
        // 3. 取得 1 週預報
        let url_week = {
            let mut url = url::Url::parse(&format!("{}/{}", CWA_BASE_URL, DATASET_WEEK)).unwrap();
            url.query_pairs_mut()
                .append_pair("Authorization", &self.api_key)
                .append_pair("format", "JSON")
                .append_pair("locationName", county);
            url
        };
        
        let week_data = agent.get(url_week.as_str()).call().ok()?.into_string().ok()?;
        let week_records: WeekForecastRecords = serde_json::from_str(&week_data).ok()?;
        let parsed_week = parse_week_forecast(&week_records);
        let week_forecast = parsed_week.first();
        
        Some(build_snapshot(forecast, parsed_obs.as_ref(), week_forecast, position))
    }
}

/// 解析後的觀測資料
#[derive(Debug, Clone)]
pub struct ParsedObservation {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub wind_speed: Option<f32>,
    pub weather_desc: Option<String>,
    pub station_name: String,
}

/// 解析後的 36 小時預報
#[derive(Debug, Clone)]
pub struct Parsed36hrForecast {
    pub location_name: String,
    pub periods: Vec<ForecastPeriod>,
}

#[derive(Debug, Clone)]
pub struct ForecastPeriod {
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub weather: Option<String>,      // Wx
    pub pop: Option<u32>,             // PoP
    pub min_temp: Option<f32>,        // MinT
    pub max_temp: Option<f32>,        // MaxT
    pub comfort: Option<String>,      // CI
}

/// 解析後的 1 週預報
#[derive(Debug, Clone)]
pub struct ParsedWeekForecast {
    pub location_name: String,
    pub daily: Vec<DailyForecast>,
}

#[derive(Debug, Clone, Default)]
pub struct DailyForecast {
    pub date: NaiveDate,
    pub weather: Option<String>,
    pub pop: Option<u32>,
    pub min_temp: Option<f32>,
    pub max_temp: Option<f32>,
}

/// 正規化台灣縣市名稱（台 -> 臺）
pub fn normalize_tw_county_name(input: &str) -> String {
    let trimmed = input.trim().replace('台', "臺");
    match trimmed.as_str() {
        "高雄" => "高雄市".to_string(),
        "臺南" => "臺南市".to_string(),
        "臺北" => "臺北市".to_string(),
        "新北" => "新北市".to_string(),
        "桃園" => "桃園市".to_string(),
        "臺中" => "臺中市".to_string(),
        "新竹" => "新竹市".to_string(),
        "嘉義" => "嘉義市".to_string(),
        "基隆" => "基隆市".to_string(),
        "苗栗" => "苗栗縣".to_string(),
        "彰化" => "彰化縣".to_string(),
        "南投" => "南投縣".to_string(),
        "雲林" => "雲林縣".to_string(),
        "屏東" => "屏東縣".to_string(),
        "宜蘭" => "宜蘭縣".to_string(),
        "花蓮" => "花蓮縣".to_string(),
        "臺東" => "臺東縣".to_string(),
        "澎湖" => "澎湖縣".to_string(),
        "金門" => "金門縣".to_string(),
        "連江" => "連江縣".to_string(),
        other => other.to_string(),
    }
}

/// 台灣 22 縣市列表（標準 CWA 名稱）
pub const TW_COUNTIES: [&str; 22] = [
    "高雄市", "臺南市", "臺北市", "新北市", "桃園市", "臺中市",
    "新竹市", "嘉義市", "基隆市", "新竹縣", "苗栗縣", "彰化縣",
    "南投縣", "雲林縣", "嘉義縣", "屏東縣", "宜蘭縣", "花蓮縣",
    "臺東縣", "澎湖縣", "金門縣", "連江縣",
];
/// 取得縣市的經緯度位置
pub fn get_county_position(county: &str) -> Option<Position> {
    let normalized = normalize_tw_county_name(county);
    let (lat, lon) = match normalized.as_str() {
        "高雄市" => (22.6273, 120.3014),
        "臺南市" => (22.9908, 120.2133),
        "臺北市" => (25.0330, 121.5654),
        "新北市" => (25.0170, 121.4628),
        "桃園市" => (24.9937, 121.3009),
        "臺中市" => (24.1477, 120.6736),
        "新竹市" => (24.8038, 120.9685),
        "嘉義市" => (23.4801, 120.4491),
        "基隆市" => (25.1283, 121.7419),
        "新竹縣" => (24.8387, 121.0177),
        "苗栗縣" => (24.5602, 120.8214),
        "彰化縣" => (24.0518, 120.5161),
        "南投縣" => (23.9095, 120.6860),
        "雲林縣" => (23.7092, 120.4313),
        "嘉義縣" => (23.4518, 120.2554),
        "屏東縣" => (22.5519, 120.5487),
        "宜蘭縣" => (24.7021, 121.7378),
        "花蓮縣" => (23.9872, 121.6016),
        "臺東縣" => (22.7583, 121.1444),
        "澎湖縣" => (23.5689, 119.5773),
        "金門縣" => (24.4452, 118.3613),
        "連江縣" => (26.1582, 119.9397),
        _ => return None,
    };
    Some(Position{lat, lon, label: normalized.clone(), region: "台灣".into()})
}
/// 解析 36 小時預報資料
pub fn parse_36hr_forecast(records: &Forecast36hrRecords) -> Vec<Parsed36hrForecast> {
    records.locations.iter().map(|loc| {
        let mut wx_map: HashMap<String, String> = HashMap::new();
        let mut pop_map: HashMap<String, u32> = HashMap::new();
        let mut min_t_map: HashMap<String, f32> = HashMap::new();
        let mut max_t_map: HashMap<String, f32> = HashMap::new();
        let mut ci_map: HashMap<String, String> = HashMap::new();

        for elem in &loc.weather_element {
            for slot in &elem.time {
                let key = format!("{}~{}", slot.start_time, slot.end_time);
                match elem.element_name.as_str() {
                    "Wx" => { wx_map.insert(key.clone(), slot.parameter.parameter_name.clone()); }
                    "PoP" => { if let Some(v) = slot.parameter.parameter_value.as_ref().and_then(|s| s.parse().ok()) { pop_map.insert(key.clone(), v); } }
                    "MinT" => { if let Some(v) = slot.parameter.parameter_value.as_ref().and_then(|s| s.parse().ok()) { min_t_map.insert(key.clone(), v); } }
                    "MaxT" => { if let Some(v) = slot.parameter.parameter_value.as_ref().and_then(|s| s.parse().ok()) { max_t_map.insert(key.clone(), v); } }
                    "CI" => { ci_map.insert(key.clone(), slot.parameter.parameter_name.clone()); }
                    _ => {}
                }
            }
        }

        let mut periods = Vec::new();
        for (key, weather) in wx_map {
            let (start_str, end_str) = key.split_once('~').unwrap_or(("", ""));
            let start_time = parse_cwa_time(start_str).unwrap_or_else(|| Local::now());
            let end_time = parse_cwa_time(end_str).unwrap_or_else(|| Local::now());
            
            periods.push(ForecastPeriod {
                start_time,
                end_time,
                weather: Some(weather),
                pop: pop_map.get(&key).copied(),
                min_temp: min_t_map.get(&key).copied(),
                max_temp: max_t_map.get(&key).copied(),
                comfort: ci_map.get(&key).cloned(),
            });
        }
        periods.sort_by_key(|p| p.start_time);

        Parsed36hrForecast {
            location_name: loc.location_name.clone(),
            periods,
        }
    }).collect()
}

/// 解析觀測資料（找最近測站）
pub fn parse_observation(records: &ObservationRecords, target_lat: f64, target_lon: f64) -> Option<ParsedObservation> {
    let mut best: Option<(f64, ParsedObservation)> = None;

    for loc in &records.location {
        let lat: f64 = loc.lat.parse().ok()?;
        let lon: f64 = loc.lon.parse().ok()?;
        
        let dist_sq = (lat - target_lat).powi(2) + (lon - target_lon).powi(2);
        
        let mut temp = None;
        let mut humidity = None;
        let mut wind_speed = None;
        let mut weather_desc = None;

        for elem in &loc.weather_element {
            match elem.element_name.as_str() {
                "TEMP" => temp = elem.element_value.parse().ok(),
                "HUMD" => humidity = elem.element_value.parse().ok(),
                "WDSD" => wind_speed = elem.element_value.parse().ok(),
                "WEATHER" => weather_desc = Some(elem.element_value.clone()),
                _ => {}
            }
        }

        if temp.is_some() {
            let obs = ParsedObservation {
                temperature: temp,
                humidity,
                wind_speed,
                weather_desc,
                station_name: loc.location_name.clone(),
            };
            best = Some(match best {
                Some((d, _)) if d < dist_sq => (d, obs),
                None => (dist_sq, obs),
                Some(other) => other,
            });
        }
    }

    best.map(|(_, o)| o)
}

/// 解析 1 週預報
pub fn parse_week_forecast(records: &WeekForecastRecords) -> Vec<ParsedWeekForecast> {
    records.locations.iter().map(|loc| {
        let mut daily_map: HashMap<NaiveDate, DailyForecast> = HashMap::new();

        for elem in &loc.weather_element {
            for slot in &elem.time {
                if let Some(date) = parse_cwa_date(&slot.start_time) {
                    let daily = daily_map.entry(date).or_insert(DailyForecast {
                        date,
                        weather: None,
                        pop: None,
                        min_temp: None,
                        max_temp: None,
                    });

                    for val in &slot.element_value {
                        let v: f32 = val.value.parse().unwrap_or_default();
                        match elem.element_name.as_str() {
                            "Wx" => daily.weather = Some(val.value.clone()),
                            "PoP" => daily.pop = Some(v as u32),
                            "MinT" => daily.min_temp = Some(v),
                            "MaxT" => daily.max_temp = Some(v),
                            _ => {}
                        }
                    }
                }
            }
        }

        let mut daily: Vec<DailyForecast> = daily_map.into_values().collect();
        daily.sort_by_key(|d| d.date);

        ParsedWeekForecast {
            location_name: loc.location_name.clone(),
            daily,
        }
    }).collect()
}

fn parse_cwa_time(s: &str) -> Option<DateTime<Local>> {
    // 格式：2024-01-15 06:00:00
    NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok().and_then(|d| {
        let time_part = &s[11..19];
        chrono::NaiveTime::parse_from_str(time_part, "%H:%M:%S").ok().map(|t| {
            Local.from_local_datetime(&d.and_time(t)).single().unwrap_or_else(|| Local::now())
        })
    })
}

fn parse_cwa_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(&s[..10], "%Y-%m-%d").ok()
}

/// 將 CWA 天氣現象文字對應到 WMO 代碼
pub fn cwa_weather_to_wmo_code(text: Option<&str>) -> u8 {
    let text = text.unwrap_or("");
    match text {
        t if t.contains("晴") && !t.contains("雲") => 0,      // 晴
        t if t.contains("多雲") || t.contains("晴時多雲") => 1, // 多雲
        t if t.contains("陰") => 3,                          // 陰
        t if t.contains("雨") && t.contains("雷") => 95,     // 雷雨
        t if t.contains("雨") => 61,                         // 雨
        t if t.contains("雪") => 71,                         // 雪
        t if t.contains("霧") => 45,                         // 霧
        _ => 2,                                              // 部分多雲
    }
}

/// 將 36 小時預報 + 觀測 + 1 週預報轉換為 Snapshot
pub fn build_snapshot(
    forecast_36hr: &Parsed36hrForecast,
    observation: Option<&ParsedObservation>,
    week_forecast: Option<&ParsedWeekForecast>,
    position: Position,
) -> Snapshot {
    let now = Local::now();
    
    // 目前天氣：優先用觀測資料，否則用 36hr 第一個時段
    let (current_temp, current_condition, current_code, humidity, wind_speed) = if let Some(obs) = observation {
        (
            obs.temperature.unwrap_or(25.0),
            obs.weather_desc.clone().unwrap_or_else(|| "不明".to_string()),
            cwa_weather_to_wmo_code(obs.weather_desc.as_deref()),
            obs.humidity.map(|h| h as u32),
            obs.wind_speed,
        )
    } else if let Some(first) = forecast_36hr.periods.first() {
        (
            first.max_temp.unwrap_or(25.0),
            first.weather.clone().unwrap_or_else(|| "不明".to_string()),
            cwa_weather_to_wmo_code(first.weather.as_deref()),
            None,
            None,
        )
    } else {
        (25.0, "不明".to_string(), 2, None, None)
    };

    // 七日預報
    let mut days = Vec::new();
    if let Some(week) = week_forecast {
        for daily in &week.daily {
            days.push(Day {
                label: if daily.date == NaiveDate::from(now.date_naive()) { "今天".to_string() }
                       else if daily.date == (NaiveDate::from(now.date_naive()) + chrono::Duration::days(1)) { "明天".to_string() }
                       else { format!("周{}", ["一", "二", "三", "四", "五", "六", "日"][daily.date.weekday().num_days_from_monday() as usize]) },
                date: daily.date.to_string(),
                high: daily.max_temp.unwrap_or(30.0),
                low: daily.min_temp.unwrap_or(20.0),
                code: cwa_weather_to_wmo_code(daily.weather.as_deref()),
                rain: daily.pop.unwrap_or(0),
            });
        }
    } else {
        // 回退到 36hr 預報產生前 3 天
        let mut date_map: HashMap<NaiveDate, Vec<&ForecastPeriod>> = HashMap::new();
        for p in &forecast_36hr.periods {
            date_map.entry(p.start_time.date_naive()).or_default().push(p);
        }
        for (i, (date, periods)) in date_map.into_iter().take(7).enumerate() {
            let max_t = periods.iter().filter_map(|p| p.max_temp).fold(f32::MIN, f32::max);
            let min_t = periods.iter().filter_map(|p| p.min_temp).fold(f32::MAX, f32::min);
            let pop = periods.iter().filter_map(|p| p.pop).max().unwrap_or(0);
            let weather = periods.first().and_then(|p| p.weather.as_deref());
            
            days.push(Day {
                label: if i == 0 { "今天".to_string() }
                       else if i == 1 { "明天".to_string() }
                       else { format!("周{}", ["一", "二", "三", "四", "五", "六", "日"][date.weekday().num_days_from_monday() as usize]) },
                date: date.to_string(),
                high: if max_t == f32::MIN { 30.0 } else { max_t },
                low: if min_t == f32::MAX { 20.0 } else { min_t },
                code: cwa_weather_to_wmo_code(weather),
                rain: pop,
            });
        }
    }

    // 日出日落（從 36hr 取第一天）
    let sun_times = forecast_36hr.periods.first().and_then(|p| {
        let sunrise = p.start_time.timestamp();
        let sunset = p.end_time.timestamp();
        Some(SunTimes {
            day_start: sunrise,
            day_end: sunset,
            sunrise,
            sunset,
            timezone: "Asia/Taipei".to_string(),
        })
    });

    Snapshot {
        condition: Some(current_condition),
        attributions: Vec::new(),
        place: position.label.clone(),
        position: Some(position),
        auto: false,
        temp: current_temp,
        feels: None,
        humidity,
        wind: wind_speed,
        code: current_code,
        day: now.time().hour() >= 6 && now.time().hour() < 18,
        high: days.first().map(|d| d.high),
        low: days.first().map(|d| d.low),
        sunrise: None,
        sunset: None,
        sun_times,
        days,
        at: now.timestamp(),
    }
}
/// 背景執行緒執行工作並回傳結果（與 qweather::spawn 相容）
pub(crate) fn spawn<T: Send + 'static>(work: impl FnOnce() -> Result<T, CwaError> + Send + 'static) -> Receiver<Result<T, CwaError>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx
}