//! 天氣來源設定：Open-Meteo（免金鑰，預設）與台灣中央氣象署 (CWA)。
//! 授權碼只保存在 Windows 憑證管理器；設定檔只記錄來源與縣市、鄉鎮。
use crate::weather::Position;
use serde::{Deserialize, Serialize};

/// 舊版設定中的未知來源（例如已移除的和風天氣）一律回到 Open-Meteo。
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub(crate) enum Provider {
    Cwa,
    #[default]
    #[serde(other)]
    OpenMeteo,
}
impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenMeteo => "Open-Meteo",
            Self::Cwa => "中央氣象署 (CWA)",
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::OpenMeteo => "https://open-meteo.com/",
            Self::Cwa => "https://opendata.cwa.gov.tw/",
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Location {
    pub position: Position,
    pub timezone: String,
    /// CWA 查詢用的縣市與鄉鎮（鄉鎮為空時使用縣市級一週預報）。
    #[serde(default)]
    pub county: String,
    #[serde(default)]
    pub district: String,
}

/// 由縣市與鄉鎮（可為空）建立 CWA 地點；鄉鎮必須屬於該縣市的預設清單。
pub(crate) fn cwa_location(county: &str, district: &str) -> Option<Location> {
    use crate::cwa_weather::{area_label, district_options, get_county_position, normalize_tw_city_name};
    let county = normalize_tw_city_name(county);
    let district = district.trim();
    if !district.is_empty() && !district_options(&county).contains(&district) {
        return None;
    }
    let mut position = get_county_position(&county)?;
    position.label = area_label(&county, district);
    Some(Location { position, timezone: "Asia/Taipei".into(), county, district: district.into() })
}
impl Location {
    pub fn valid(&self) -> bool {
        self.position.valid() && self.timezone.parse::<chrono_tz::Tz>().is_ok()
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub(crate) struct Config {
    pub provider: Provider,
    pub location: Option<Location>,
    pub saved: Vec<Location>,
    /// 舊版明文授權碼，只用於一次性搬移到憑證管理器，永不寫回設定檔。
    #[serde(default, rename = "credential", skip_serializing)]
    pub legacy_key: String,
}
impl Config {
    pub fn load() -> Self {
        std::fs::read(crate::model::config_dir().join("weather-provider.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(|mut c| {
                // 旧版和风城市的时区与地区不属于台湾来源，直接丢弃。
                c.saved.retain(|l| l.valid() && l.position.region == "台灣");
                c.saved.truncate(crate::weather::MAX_CITIES);
                if !c.location.as_ref().is_some_and(|l| l.valid() && l.position.region == "台灣") {
                    c.location = None;
                }
                c
            })
            .unwrap_or_default()
    }
    pub fn save(&self) -> Result<(), Error> {
        let dir = crate::model::config_dir();
        std::fs::create_dir_all(&dir).map_err(|_| Error::Storage)?;
        crate::model::atomic_write(
            &dir.join("weather-provider.json"),
            &serde_json::to_vec(self).map_err(|_| Error::Storage)?,
        )
        .map_err(|_| Error::Storage)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Storage,
}
impl Error {
    pub fn text(self) -> &'static str {
        match self {
            Self::Storage => "无法安全保存天气设置，请检查 Windows 凭据管理器后重试。",
        }
    }
}

#[cfg(all(windows, not(test)))]
mod keystore {
    const SERVICE: &str = "Daozai.Weather.CWA";
    const USER: &str = "authkey";
    fn entry() -> Option<keyring::Entry> {
        keyring::Entry::new(SERVICE, USER).ok()
    }
    pub fn store(key: &str) -> bool {
        entry().is_some_and(|e| e.set_password(key).is_ok())
    }
    pub fn load() -> Option<String> {
        entry()?.get_password().ok().filter(|k| !k.trim().is_empty())
    }
    pub fn delete() -> bool {
        match entry().map(|e| e.delete_credential()) {
            Some(Ok(())) | Some(Err(keyring::Error::NoEntry)) => true,
            _ => false,
        }
    }
}
#[cfg(any(not(windows), test))]
mod keystore {
    // 非 Windows 与测试环境不接触真实凭据库；测试通过 CwaClient 直接注入授权碼。
    pub fn store(_key: &str) -> bool {
        cfg!(test)
    }
    pub fn load() -> Option<String> {
        None
    }
    pub fn delete() -> bool {
        true
    }
}

/// 保存 CWA 授權碼到 Windows 憑證管理器。
pub(crate) fn store_cwa_key(key: &str) -> Result<(), Error> {
    if keystore::store(key.trim()) { Ok(()) } else { Err(Error::Storage) }
}
/// 讀取已保存的 CWA 授權碼；沒有或無法讀取時為 None。
pub(crate) fn load_cwa_key() -> Option<String> {
    keystore::load()
}
/// 清除已保存的 CWA 授權碼。
pub(crate) fn delete_cwa_key() -> Result<(), Error> {
    if keystore::delete() { Ok(()) } else { Err(Error::Storage) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_provider_names_fall_back_to_open_meteo() {
        let config: Config = serde_json::from_str(r#"{"provider":"Qweather","host":"x.qweatherapi.com","saved":[]}"#).unwrap();
        assert_eq!(config.provider, Provider::OpenMeteo);
        let config: Config = serde_json::from_str(r#"{"provider":"Cwa","saved":[]}"#).unwrap();
        assert_eq!(config.provider, Provider::Cwa);
    }

    #[test]
    fn legacy_plaintext_key_is_read_but_never_written_back() {
        let config: Config = serde_json::from_str(r#"{"provider":"Cwa","credential":"legacy-key","saved":[]}"#).unwrap();
        assert_eq!(config.legacy_key, "legacy-key");
        let saved = serde_json::to_string(&config).unwrap();
        assert!(!saved.contains("legacy-key") && !saved.contains("credential"));
    }
}
