//! 臺灣中央氣象署 (CWA) 開放資料平臺 API 客戶端
//! 支援：一般天氣預報-今明 36 小時 (F-C0032-001)、現在天氣觀測 (O-A0003-001)、鄉鎮未來 1 週 (F-D0047-091)
//! 授權碼只進 Windows 憑證管理器；網路呼叫與 JSON 解析分開，解析可用固定樣本單元測試。

use crate::weather::{Day, Position, Snapshot, SunTimes};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc};
use std::time::Duration;
use ureq::AgentBuilder;

const CWA_BASE_URL: &str = "https://opendata.cwa.gov.tw/api/v1/rest/datastore";
/// 授權碼申請頁（設定頁「申請授權碼」連結）。
pub const AUTHKEY_URL: &str = "https://opendata.cwa.gov.tw/user/authkey";
const DATASET_36HR: &str = "F-C0032-001"; // 一般天氣預報-今明 36 小時
const DATASET_OBS: &str = "O-A0003-001"; // 現在天氣觀測報告
const DATASET_WEEK: &str = "F-D0047-091"; // 鄉鎮未來 1 週天氣預報
/// 觀測資料缺值標記（CWA 以 -99、-998 表示無資料）。
const MISSING_VALUES: [f32; 2] = [-99.0, -998.0];

/// CWA 錯誤型別
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CwaError {
    /// 未設定授權碼
    NoKey,
    /// 授權碼無效
    InvalidKey,
    /// 請求引數無效
    InvalidParam,
    /// 請求頻率超限
    RateLimited,
    /// 月度額度用盡
    QuotaExceeded,
    /// 網路錯誤
    Network,
    /// 回應格式錯誤
    InvalidResponse,
    /// 非臺灣地區
    OutOfRegion,
    /// 已停用
    Disabled,
}

impl CwaError {
    /// 面向使用者的說明；設定頁與錯誤訊息共用同一份文字。
    pub fn text(self) -> &'static str {
        match self {
            CwaError::NoKey => "尚未設定 CWA 授權碼",
            CwaError::InvalidKey => "CWA 授權碼無效",
            CwaError::InvalidParam => "請求引數無效",
            CwaError::RateLimited => "請求過於頻繁，請稍後再試",
            CwaError::QuotaExceeded => "月度額度已用盡",
            CwaError::Network => "網路連線失敗",
            CwaError::InvalidResponse => "伺服器回應格式異常",
            CwaError::OutOfRegion => "CWA 僅提供臺灣地區天氣資料",
            CwaError::Disabled => "CWA 天氣來源已停用",
        }
    }
}
impl std::fmt::Display for CwaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.text())
    }
}
impl std::error::Error for CwaError {}

// ---- 原始 JSON 結構（欄位名稱依 CWA 官方回應：records 內為 camelCase） ----

/// 最外層回應：`{"success":"true","records":{...}}`
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    success: Option<String>,
    msg: Option<String>,
    records: Option<T>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Forecast36hrRecords {
    #[serde(default)]
    location: Vec<ForecastLocation>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ForecastLocation {
    location_name: String,
    #[serde(default)]
    weather_element: Vec<WeatherElement>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeatherElement {
    element_name: String,
    #[serde(default)]
    time: Vec<TimeSlot>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TimeSlot {
    start_time: String,
    end_time: String,
    parameter: Parameter,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Parameter {
    parameter_name: String,
    parameter_value: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservationRecords {
    #[serde(default)]
    location: Vec<ObservationLocation>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservationLocation {
    location_name: String,
    lat: String,
    lon: String,
    #[serde(default)]
    weather_element: Vec<ObservationElement>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservationElement {
    element_name: String,
    element_value: String,
}

/// F-D0047-091：`records.locations[]` 依縣市分組，每個縣市內含 `location[]`（鄉鎮）。
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeekForecastRecords {
    #[serde(default)]
    locations: Vec<WeekCounty>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeekCounty {
    #[serde(default)]
    location: Vec<WeekForecastLocation>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeekForecastLocation {
    location_name: String,
    #[serde(default)]
    weather_element: Vec<WeekWeatherElement>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeekWeatherElement {
    element_name: String,
    #[serde(default)]
    time: Vec<WeekTimeSlot>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeekTimeSlot {
    start_time: String,
    #[allow(dead_code)]
    end_time: String,
    #[serde(default)]
    element_value: Vec<WeekElementValue>,
}
#[derive(Debug, Deserialize)]
pub(crate) struct WeekElementValue {
    value: String,
}

// ---- 解析後的資料 ----

#[derive(Debug, Clone)]
pub struct ParsedObservation {
    pub temperature: Option<f32>,
    pub humidity: Option<f32>,
    pub wind_speed: Option<f32>,
    pub weather_desc: Option<String>,
    pub station_name: String,
}

#[derive(Debug, Clone)]
pub struct Parsed36hrForecast {
    pub location_name: String,
    pub periods: Vec<ForecastPeriod>,
}

#[derive(Debug, Clone)]
pub struct ForecastPeriod {
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub weather: Option<String>, // Wx
    pub pop: Option<u32>,        // PoP
    pub min_temp: Option<f32>,   // MinT
    pub max_temp: Option<f32>,   // MaxT
    pub comfort: Option<String>, // CI
}

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

// ---- 臺灣縣市與鄉鎮 ----

/// 臺灣 22 縣市（CWA 標準名稱）
pub const TW_COUNTIES: [&str; 22] = [
    "高雄市", "臺南市", "臺北市", "新北市", "桃園市", "臺中市",
    "基隆市", "新竹市", "嘉義市", "新竹縣", "苗栗縣", "彰化縣",
    "南投縣", "雲林縣", "嘉義縣", "屏東縣", "宜蘭縣", "花蓮縣",
    "臺東縣", "澎湖縣", "金門縣", "連江縣",
];

/// 預設提供鄉鎮選單的縣市（其餘縣市目前只提供縣市級預報）。
const TW_DISTRICTS: [(&str, &[&str]); 3] = [
    ("臺北市", &["中正區", "大同區", "中山區", "松山區", "大安區", "萬華區", "信義區", "士林區", "北投區", "內湖區", "南港區", "文山區"]),
    ("臺南市", &["中西區", "東區", "南區", "北區", "安平區", "安南區", "永康區", "歸仁區", "新化區", "左鎮區", "玉井區", "楠西區", "南化區", "仁德區", "關廟區", "龍崎區", "官田區", "麻豆區", "佳里區", "西港區", "七股區", "將軍區", "學甲區", "北門區", "新營區", "後壁區", "白河區", "東山區", "六甲區", "下營區", "柳營區", "鹽水區", "善化區", "大內區", "山上區", "新市區", "安定區"]),
    ("高雄市", &["新興區", "前金區", "苓雅區", "鹽埕區", "鼓山區", "旗津區", "前鎮區", "三民區", "楠梓區", "小港區", "左營區", "仁武區", "大社群", "岡山區", "路竹區", "阿蓮區", "田寮區", "燕巢區", "橋頭區", "梓官區", "彌陀區", "永安區", "湖內區", "鳳山區", "大寮區", "林園區", "鳥松區", "大樹區", "旗山區", "美濃區", "六龜區", "內門區", "杉林區", "甲仙區", "桃源區", "那瑪夏區", "茂林區", "茄萣區"]),
];

/// 鄉鎮選單（沒有資料時為空，表示只使用縣市級預報）。
pub fn district_options(county: &str) -> &'static [&'static str] {
    let county = normalize_tw_city_name(county);
    TW_DISTRICTS.iter().find(|(name, _)| *name == county).map_or(&[], |(_, districts)| *districts)
}

/// 顯示與儲存用的地點名稱：「高雄市三民區」或「臺北市」。
pub fn area_label(county: &str, district: &str) -> String {
    let county = normalize_tw_city_name(county);
    if district.trim().is_empty() { county } else { format!("{county}{}", district.trim()) }
}

/// 正規化臺灣縣市名稱（臺 -> 臺，簡稱補上「市」或「縣」）。
pub fn normalize_tw_city_name(input: &str) -> String {
    let trimmed = input.trim().replace('臺', "臺");
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

/// 縣市的近似中心座標（用於觀測站就近選擇與地圖位置）。
pub fn get_county_position(county: &str) -> Option<Position> {
    let normalized = normalize_tw_city_name(county);
    let (lat, lon) = match normalized.as_str() {
        "臺北市" => (25.0330, 121.5654),
        "新北市" => (25.0170, 121.4628),
        "桃園市" => (24.9937, 121.3009),
        "臺中市" => (24.1477, 120.6736),
        "臺南市" => (22.9908, 120.2133),
        "高雄市" => (22.6273, 120.3014),
        "基隆市" => (25.1283, 121.7419),
        "新竹市" => (24.8038, 120.9685),
        "嘉義市" => (23.4801, 120.4491),
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
    Some(Position { lat, lon, label: normalized, region: "臺灣".into() })
}

// ---- 解析 ----

/// 解開 `{"success":..., "records":{...}}`；`success` 為 false 時依訊息區分授權碼與額度問題。
fn parse_envelope<T: for<'de> Deserialize<'de> + Default>(body: &str) -> Result<T, CwaError> {
    let envelope: Envelope<T> = serde_json::from_str(body).map_err(|_| CwaError::InvalidResponse)?;
    if envelope.success.as_deref() == Some("false") {
        let msg = envelope.msg.unwrap_or_default();
        return Err(if msg.contains("授權") || msg.to_ascii_lowercase().contains("authoriz") {
            CwaError::InvalidKey
        } else if msg.contains("額度") || msg.to_ascii_lowercase().contains("quota") {
            CwaError::QuotaExceeded
        } else {
            CwaError::InvalidResponse
        });
    }
    envelope.records.ok_or(CwaError::InvalidResponse)
}

/// 解析 36 小時預報，依時段聚合 Wx / PoP / MinT / MaxT / CI。
pub(crate) fn parse_36hr_forecast(records: &Forecast36hrRecords) -> Vec<Parsed36hrForecast> {
    records.location.iter().map(|loc| {
        let mut periods: HashMap<(String, String), ForecastPeriod> = HashMap::new();
        for elem in &loc.weather_element {
            for slot in &elem.time {
                let (Some(start), Some(end)) = (parse_cwa_time(&slot.start_time), parse_cwa_time(&slot.end_time)) else { continue };
                let entry = periods.entry((slot.start_time.clone(), slot.end_time.clone())).or_insert(ForecastPeriod {
                    start_time: start, end_time: end, weather: None, pop: None, min_temp: None, max_temp: None, comfort: None,
                });
                // 數值可能只出現在 parameterName 中，因此 parameterValue 缺失時退回名稱。
                let raw = slot.parameter.parameter_value.as_deref().unwrap_or(&slot.parameter.parameter_name);
                let value = raw.trim().parse::<f32>().ok();
                match elem.element_name.as_str() {
                    "Wx" => entry.weather = Some(slot.parameter.parameter_name.clone()),
                    "PoP" => entry.pop = value.map(|v| v.max(0.0) as u32),
                    "MinT" => entry.min_temp = value,
                    "MaxT" => entry.max_temp = value,
                    "CI" => entry.comfort = Some(slot.parameter.parameter_name.clone()),
                    _ => {}
                }
            }
        }
        let mut periods: Vec<ForecastPeriod> = periods.into_values().collect();
        periods.sort_by_key(|p| p.start_time);
        Parsed36hrForecast { location_name: loc.location_name.clone(), periods }
    }).collect()
}

/// 解析觀測資料：取離目標座標最近、且有氣溫的測站；缺值標記視為沒有資料。
pub(crate) fn parse_observation(records: &ObservationRecords, target_lat: f64, target_lon: f64) -> Option<ParsedObservation> {
    let mut best: Option<(f64, ParsedObservation)> = None;
    for loc in &records.location {
        let (Ok(lat), Ok(lon)) = (loc.lat.parse::<f64>(), loc.lon.parse::<f64>()) else { continue };
        let number = |name: &str| loc.weather_element.iter().find(|e| e.element_name == name)
            .and_then(|e| e.element_value.trim().parse::<f32>().ok()).filter(|v| !MISSING_VALUES.contains(v));
        let Some(temperature) = number("TEMP") else { continue };
        let weather_desc = loc.weather_element.iter().find(|e| e.element_name == "WEATHER")
            .map(|e| e.element_value.trim().to_string()).filter(|s| !s.is_empty() && s != "-");
        let distance = (lat - target_lat).powi(2) + (lon - target_lon).powi(2);
        let obs = ParsedObservation {
            temperature: Some(temperature),
            humidity: number("HUMD"),
            wind_speed: number("WDSD"),
            weather_desc,
            station_name: loc.location_name.clone(),
        };
        if best.as_ref().map_or(true, |(d, _)| distance < *d) {
            best = Some((distance, obs));
        }
    }
    best.map(|(_, obs)| obs)
}

/// 解析 1 週預報。`district` 指定鄉鎮名稱（如「三民區」）；找不到時取第一筆。
/// 降雨機率欄位同時接受 `PoP` 與 `PoP12h`，取每日較高值。
pub(crate) fn parse_week_forecast(records: &WeekForecastRecords, district: &str) -> Option<ParsedWeekForecast> {
    let locations: Vec<&WeekForecastLocation> = records.locations.iter().flat_map(|c| c.location.iter()).collect();
    let loc = locations.iter().find(|l| l.location_name == district).or(locations.first())?;
    let mut daily_map: HashMap<NaiveDate, DailyForecast> = HashMap::new();
    for elem in &loc.weather_element {
        for slot in &elem.time {
            let Some(date) = parse_cwa_date(&slot.start_time) else { continue };
            let daily = daily_map.entry(date).or_insert_with(|| DailyForecast { date, ..Default::default() });
            for val in &slot.element_value {
                let number = val.value.trim().parse::<f32>().ok().filter(|v| !MISSING_VALUES.contains(v));
                match elem.element_name.as_str() {
                    "Wx" => daily.weather = Some(val.value.clone()),
                    "PoP" | "PoP12h" => daily.pop = daily.pop.max(number.map(|v| v.max(0.0) as u32)),
                    "MinT" => daily.min_temp = number,
                    "MaxT" => daily.max_temp = number,
                    _ => {}
                }
            }
        }
    }
    let mut daily: Vec<DailyForecast> = daily_map.into_values().collect();
    daily.sort_by_key(|d| d.date);
    Some(ParsedWeekForecast { location_name: loc.location_name.clone(), daily })
}

fn parse_cwa_time(s: &str) -> Option<DateTime<Local>> {
    // 格式：2024-01-15 06:00:00
    let date = NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()?;
    let time = chrono::NaiveTime::parse_from_str(s.get(11..19)?, "%H:%M:%S").ok()?;
    Local.from_local_datetime(&date.and_time(time)).earliest()
}

fn parse_cwa_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

/// 將 CWA 天氣現象文字對應到 WMO 程式碼（與 Open-Meteo 的圖示共用）。
pub fn cwa_weather_to_wmo_code(text: Option<&str>) -> u8 {
    let text = text.unwrap_or("");
    match text {
        t if t.contains("雷") => 95,
        t if t.contains("雪") => 71,
        t if t.contains("雨") => 61,
        t if t.contains("霧") => 45,
        t if t.contains("陰") => 3,
        t if t.contains("多雲") => 2,
        t if t.contains("晴") => 0,
        _ => 2,
    }
}

/// 將 36 小時預報 + 觀測 + 1 週預報轉換為 Snapshot。
pub(crate) fn build_snapshot(
    forecast_36hr: &Parsed36hrForecast,
    observation: Option<&ParsedObservation>,
    week_forecast: Option<&ParsedWeekForecast>,
    position: Position,
) -> Snapshot {
    build_snapshot_at(forecast_36hr, observation, week_forecast, position, Local::now())
}

fn build_snapshot_at(
    forecast_36hr: &Parsed36hrForecast,
    observation: Option<&ParsedObservation>,
    week_forecast: Option<&ParsedWeekForecast>,
    position: Position,
    now: DateTime<Local>,
) -> Snapshot {
    let today = now.date_naive();

    // 目前天氣：優先觀測，否則用 36 小時第一個時段
    let (current_temp, current_condition, current_code, humidity, wind_speed) = if let Some(obs) = observation {
        (
            obs.temperature.unwrap_or(25.0),
            obs.weather_desc.clone().unwrap_or_else(|| "不明".to_string()),
            cwa_weather_to_wmo_code(obs.weather_desc.as_deref()),
            // O-A0003-001 的 HUMD 多為小數（0.71 = 71%），個別資料集以百分比表示，兩者都接受。
            obs.humidity.map(|h| if h <= 1.0 { (h * 100.0).round() as u32 } else { h.round() as u32 }),
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

    let day_label = |date: NaiveDate, index: usize| -> String {
        if date == today { "今天".into() }
        else if index == 1 && date == today + chrono::Duration::days(1) { "明天".into() }
        else { format!("周{}", ["一", "二", "三", "四", "五", "六", "日"][date.weekday().num_days_from_monday() as usize]) }
    };

    let mut days = Vec::new();
    if let Some(week) = week_forecast {
        for (index, daily) in week.daily.iter().filter(|d| d.date >= today).take(7).enumerate() {
            days.push(Day {
                label: day_label(daily.date, index),
                date: daily.date.to_string(),
                high: daily.max_temp.unwrap_or(30.0),
                low: daily.min_temp.unwrap_or(20.0),
                code: cwa_weather_to_wmo_code(daily.weather.as_deref()),
                rain: daily.pop.unwrap_or(0),
            });
        }
    } else {
        // 無一週預報時，以 36 小時預報彙整出前幾天
        let mut by_date: std::collections::BTreeMap<NaiveDate, Vec<&ForecastPeriod>> = std::collections::BTreeMap::new();
        for p in &forecast_36hr.periods {
            by_date.entry(p.start_time.date_naive()).or_default().push(p);
        }
        for (index, (date, periods)) in by_date.into_iter().take(3).enumerate() {
            let max_t = periods.iter().filter_map(|p| p.max_temp).fold(f32::MIN, f32::max);
            let min_t = periods.iter().filter_map(|p| p.min_temp).fold(f32::MAX, f32::min);
            days.push(Day {
                label: day_label(date, index),
                date: date.to_string(),
                high: if max_t == f32::MIN { 30.0 } else { max_t },
                low: if min_t == f32::MAX { 20.0 } else { min_t },
                code: cwa_weather_to_wmo_code(periods.first().and_then(|p| p.weather.as_deref())),
                rain: periods.iter().filter_map(|p| p.pop).max().unwrap_or(0),
            });
        }
    }

    Snapshot {
        condition: Some(current_condition),
        attributions: vec!["資料來源：中央氣象署開放資料平臺".into()],
        place: position.label.clone(),
        position: Some(position),
        auto: false,
        temp: current_temp,
        feels: None,
        humidity,
        wind: wind_speed,
        code: current_code,
        day: (6..18).contains(&now.time().hour()),
        high: days.first().map(|d| d.high),
        low: days.first().map(|d| d.low),
        sunrise: None,
        sunset: None,
        sun_times: forecast_36hr.periods.first().map(|p| SunTimes {
            day_start: p.start_time.timestamp(),
            day_end: p.end_time.timestamp(),
            sunrise: p.start_time.timestamp(),
            sunset: p.end_time.timestamp(),
            timezone: "Asia/Taipei".to_string(),
        }),
        days,
        at: now.timestamp(),
    }
}

/// 阻塞式 CWA 客戶端；只在背景執行緒或設定頁的測試中呼叫。
#[derive(Clone)]
pub struct CwaClient {
    api_key: String,
    pub gate: Arc<AtomicBool>,
}

impl CwaClient {
    pub fn new(api_key: String) -> Self {
        Self { api_key: api_key.trim().to_string(), gate: Arc::new(AtomicBool::new(true)) }
    }

    /// 不回傳授權碼，避免錯誤或除錯資訊外洩。
    pub fn has_key(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn get(&self, dataset: &str, params: &[(&str, &str)]) -> Result<String, CwaError> {
        if !self.gate.load(Ordering::SeqCst) {
            return Err(CwaError::Disabled);
        }
        if self.api_key.is_empty() {
            return Err(CwaError::NoKey);
        }
        let mut url = url::Url::parse(&format!("{CWA_BASE_URL}/{dataset}")).map_err(|_| CwaError::InvalidParam)?;
        url.query_pairs_mut()
            .append_pair("Authorization", &self.api_key)
            .append_pair("format", "JSON")
            .extend_pairs(params.iter().copied());
        let agent = AgentBuilder::new().redirects(0).timeout(Duration::from_secs(15)).build();
        let response = agent.get(url.as_str()).call().map_err(|e| match e {
            ureq::Error::Status(401 | 403, _) => CwaError::InvalidKey,
            ureq::Error::Status(429, _) => CwaError::RateLimited,
            _ => CwaError::Network,
        })?;
        response.into_string().map_err(|_| CwaError::InvalidResponse)
    }

    /// 測試授權碼：抓一次 36 小時預報即可確認金鑰與網路。
    pub fn test(&self, county: &str) -> Result<(), CwaError> {
        self.fetch_36hr(county).map(|_| ())
    }

    fn fetch_36hr(&self, county: &str) -> Result<Parsed36hrForecast, CwaError> {
        let body = self.get(DATASET_36HR, &[("locationName", county)])?;
        let records: Forecast36hrRecords = parse_envelope(&body)?;
        parse_36hr_forecast(&records).into_iter().next().filter(|f| !f.periods.is_empty()).ok_or(CwaError::InvalidResponse)
    }

    /// 同步抓取三個資料集並組成 Snapshot。`district` 為空時使用縣市級的一週預報。
    pub fn fetch_all(&self, county: &str, district: &str, position: Position) -> Result<Snapshot, CwaError> {
        let forecast = self.fetch_36hr(county)?;
        let observation = self.get(DATASET_OBS, &[])
            .and_then(|body| parse_envelope::<ObservationRecords>(&body))
            .ok()
            .and_then(|records| parse_observation(&records, position.lat, position.lon));
        let week_area = if district.is_empty() { county } else { district };
        let week = self.get(DATASET_WEEK, &[("locationName", week_area)])
            .and_then(|body| parse_envelope::<WeekForecastRecords>(&body))
            .ok()
            .and_then(|records| parse_week_forecast(&records, week_area));
        Ok(build_snapshot(&forecast, observation.as_ref(), week.as_ref(), position))
    }
}

/// 背景執行緒執行工作並回傳結果（與 Open-Meteo 相同的非阻塞接收方式）。
pub(crate) fn spawn<T: Send + 'static>(work: impl FnOnce() -> Result<T, CwaError> + Send + 'static) -> Receiver<Result<T, CwaError>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORECAST_36HR: &str = r#"{"success":"true","result":{"resource_id":"F-C0032-001"},"records":{"location":[{"locationName":"高雄市","weatherElement":[
        {"elementName":"Wx","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","parameter":{"parameterName":"多雲時晴","parameterValue":"2"}},{"startTime":"2026-11-16 18:00:00","endTime":"2026-11-17 06:00:00","parameter":{"parameterName":"陰短暫雨","parameterValue":"6"}}]},
        {"elementName":"PoP","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","parameter":{"parameterName":"20","parameterUnit":"百分比"}},{"startTime":"2026-11-16 18:00:00","endTime":"2026-11-17 06:00:00","parameter":{"parameterName":"60","parameterUnit":"百分比"}}]},
        {"elementName":"MinT","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","parameter":{"parameterName":"21","parameterUnit":"攝氏度"}},{"startTime":"2026-11-16 18:00:00","endTime":"2026-11-17 06:00:00","parameter":{"parameterName":"19","parameterUnit":"攝氏度"}}]},
        {"elementName":"MaxT","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","parameter":{"parameterName":"28","parameterUnit":"攝氏度"}},{"startTime":"2026-11-16 18:00:00","endTime":"2026-11-17 06:00:00","parameter":{"parameterName":"24","parameterUnit":"攝氏度"}}]},
        {"elementName":"CI","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","parameter":{"parameterName":"悶熱","parameterValue":"3"}}]}
    ]}]}}"#;

    const OBSERVATION: &str = r#"{"success":"true","records":{"location":[
        {"lat":"22.6","lon":"120.3","locationName":"高雄","stationId":"C0V880","time":{"obsTime":"2026-11-16 14:00:00"},"weatherElement":[{"elementName":"TEMP","elementValue":"27.4"},{"elementName":"HUMD","elementValue":"0.71"},{"elementName":"WDSD","elementValue":"3.2"},{"elementName":"WEATHER","elementValue":"多雲"}]},
        {"lat":"23.9","lon":"120.9","locationName":"南投","stationId":"C0U000","time":{"obsTime":"2026-11-16 14:00:00"},"weatherElement":[{"elementName":"TEMP","elementValue":"-99"},{"elementName":"HUMD","elementValue":"-99"},{"elementName":"WDSD","elementValue":"-99"},{"elementName":"WEATHER","elementValue":"-"}]},
        {"lat":"22.7","lon":"120.2","locationName":"旗山","stationId":"C0V123","time":{"obsTime":"2026-11-16 14:00:00"},"weatherElement":[{"elementName":"TEMP","elementValue":"26.9"},{"elementName":"HUMD","elementValue":"-998"},{"elementName":"WDSD","elementValue":"1.1"},{"elementName":"WEATHER","elementValue":"晴"}]}
    ]}}"#;

    const WEEK: &str = r#"{"success":"true","records":{"locations":[{"locationsName":"高雄市","location":[
        {"locationName":"三民區","weatherElement":[
            {"elementName":"Wx","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","elementValue":[{"value":"多雲","measures":""}]},{"startTime":"2026-11-17 06:00:00","endTime":"2026-11-17 18:00:00","elementValue":[{"value":"雷陣雨","measures":""}]}]},
            {"elementName":"PoP12h","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","elementValue":[{"value":"30","measures":"百分比"}]},{"startTime":"2026-11-17 06:00:00","endTime":"2026-11-17 18:00:00","elementValue":[{"value":"70","measures":"百分比"}]}]},
            {"elementName":"MinT","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-17 06:00:00","elementValue":[{"value":"20","measures":"攝氏度"}]},{"startTime":"2026-11-17 06:00:00","endTime":"2026-11-18 06:00:00","elementValue":[{"value":"21","measures":"攝氏度"}]}]},
            {"elementName":"MaxT","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-17 06:00:00","elementValue":[{"value":"29","measures":"攝氏度"}]},{"startTime":"2026-11-17 06:00:00","endTime":"2026-11-18 06:00:00","elementValue":[{"value":"30","measures":"攝氏度"}]}]}
        ]},
        {"locationName":"仁武區","weatherElement":[{"elementName":"Wx","time":[{"startTime":"2026-11-16 06:00:00","endTime":"2026-11-16 18:00:00","elementValue":[{"value":"晴","measures":""}]}]}]}
    ]}]}}"#;

    const REJECTED: &str = r#"{"success":"false","msg":"授權碼錯誤或已失效"}"#;

    fn position() -> Position {
        get_county_position("高雄市").unwrap()
    }

    #[test]
    fn parses_36hr_fixture_into_periods_with_wx_pop_temperatures_and_comfort() {
        let records: Forecast36hrRecords = parse_envelope(FORECAST_36HR).unwrap();
        let parsed = parse_36hr_forecast(&records);
        assert_eq!(parsed[0].location_name, "高雄市");
        let periods = &parsed[0].periods;
        assert_eq!(periods.len(), 2);
        assert_eq!(periods[0].weather.as_deref(), Some("多雲時晴"));
        assert_eq!(periods[0].pop, Some(20));
        assert_eq!(periods[0].min_temp, Some(21.0));
        assert_eq!(periods[0].max_temp, Some(28.0));
        assert_eq!(periods[0].comfort.as_deref(), Some("悶熱"));
        assert_eq!(periods[1].pop, Some(60));
        assert_eq!(periods[1].min_temp, Some(19.0));
    }

    #[test]
    fn observation_uses_nearest_station_with_valid_temperature_and_drops_missing_values() {
        let records: ObservationRecords = parse_envelope(OBSERVATION).unwrap();
        // 南投站離目標最近但氣溫缺值，應跳過；旗山站的濕度缺值應為 None。
        let obs = parse_observation(&records, 22.9, 120.2).unwrap();
        assert_eq!(obs.station_name, "旗山");
        assert_eq!(obs.temperature, Some(26.9));
        assert_eq!(obs.humidity, None);
        assert_eq!(obs.wind_speed, Some(1.1));
        let obs = parse_observation(&records, 22.6, 120.3).unwrap();
        assert_eq!(obs.station_name, "高雄");
        assert_eq!(obs.humidity, Some(0.71));
        assert_eq!(obs.weather_desc.as_deref(), Some("多雲"));
    }

    #[test]
    fn week_forecast_selects_the_requested_district_and_reads_pop12h() {
        let records: WeekForecastRecords = parse_envelope(WEEK).unwrap();
        let week = parse_week_forecast(&records, "三民區").unwrap();
        assert_eq!(week.location_name, "三民區");
        assert_eq!(week.daily.len(), 2);
        assert_eq!(week.daily[0].pop, Some(30));
        assert_eq!(week.daily[1].pop, Some(70));
        assert_eq!(week.daily[1].weather.as_deref(), Some("雷陣雨"));
        assert_eq!(week.daily[1].max_temp, Some(30.0));
        assert_eq!(week.daily[1].min_temp, Some(21.0));
        assert_eq!(parse_week_forecast(&records, "仁武區").unwrap().daily.len(), 1);
    }

    #[test]
    fn rejected_envelope_reports_invalid_key_and_bad_json_is_invalid_response() {
        assert_eq!(parse_envelope::<Forecast36hrRecords>(REJECTED).err(), Some(CwaError::InvalidKey));
        assert_eq!(parse_envelope::<Forecast36hrRecords>("not json").err(), Some(CwaError::InvalidResponse));
    }

    #[test]
    fn builds_snapshot_with_seven_day_panel_from_the_week_forecast() {
        let forecast = parse_36hr_forecast(&parse_envelope(FORECAST_36HR).unwrap()).remove(0);
        let observation = parse_observation(&parse_envelope(OBSERVATION).unwrap(), 22.6, 120.3);
        let week = parse_week_forecast(&parse_envelope(WEEK).unwrap(), "三民區");
        let now = Local.with_ymd_and_hms(2026, 11, 16, 14, 0, 0).unwrap();
        let snapshot = build_snapshot_at(&forecast, observation.as_ref(), week.as_ref(), position(), now);
        assert_eq!(snapshot.temp, 27.4);
        assert_eq!(snapshot.humidity, Some(71));
        assert_eq!(snapshot.place, "高雄市");
        assert_eq!(snapshot.days.len(), 2, "一週預報的今天之後資料都要進入面板");
        assert_eq!(snapshot.days[0].label, "今天");
        assert_eq!(snapshot.days[0].rain, 30);
    }

    #[test]
    fn falls_back_to_thirty_six_hour_days_when_week_forecast_is_missing() {
        let forecast = parse_36hr_forecast(&parse_envelope(FORECAST_36HR).unwrap()).remove(0);
        let now = Local.with_ymd_and_hms(2026, 11, 16, 14, 0, 0).unwrap();
        let snapshot = build_snapshot_at(&forecast, None, None, position(), now);
        assert!(!snapshot.days.is_empty());
        assert_eq!(snapshot.temp, 28.0, "無觀測時以 36 小時第一個時段代替");
    }

    #[test]
    fn city_names_normalize_to_cwa_standard_and_cover_all_twenty_two_counties() {
        assert_eq!(normalize_tw_city_name("臺南"), "臺南市");
        assert_eq!(normalize_tw_city_name(" 高雄 "), "高雄市");
        assert_eq!(normalize_tw_city_name("臺東"), "臺東縣");
        assert_eq!(TW_COUNTIES.len(), 22);
        for county in TW_COUNTIES {
            assert_eq!(normalize_tw_city_name(county), county);
            assert!(get_county_position(county).is_some(), "{county} 缺少座標");
        }
        assert_eq!(area_label("高雄", "三民區"), "高雄市三民區");
        assert_eq!(area_label("臺南市", ""), "臺南市");
        assert!(district_options("高雄市").contains(&"三民區"));
        assert!(district_options("臺南市").contains(&"仁德區"));
        assert!(district_options("臺北市").contains(&"信義區"));
        assert!(district_options("澎湖縣").is_empty());
    }

    #[test]
    fn weather_text_maps_to_icons_with_thunderstorm_before_rain() {
        assert_eq!(cwa_weather_to_wmo_code(Some("雷陣雨")), 95);
        assert_eq!(cwa_weather_to_wmo_code(Some("陰短暫雨")), 61);
        assert_eq!(cwa_weather_to_wmo_code(Some("多雲時晴")), 2);
        assert_eq!(cwa_weather_to_wmo_code(Some("晴")), 0);
    }

    #[test]
    fn client_without_key_never_sends_a_request() {
        let client = CwaClient::new("  ".into());
        assert!(!client.has_key());
        assert_eq!(client.test("臺北市"), Err(CwaError::NoKey));
    }
}
