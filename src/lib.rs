//! 島仔：Windows 頂部「靈動島」。
//!
//! - `app` / `pages`：與平臺無關的介面與動效（外殼與錄製器共用）
//! - `anim` / `paint` / `shape` / `theme`：彈簧、向量繪製、面板輪廓、配色字型
//! - `model` / `lyrics` / `smtc` / `icons`：資料、歌詞、系統媒體、檔案圖示
//! - `native` / `tray`：僅 Windows 的視窗區域、命中穿透與系統匣

pub mod display_layout;
pub mod diagnostics_tool;
mod mail_privacy;
pub mod poll_clock;
pub mod resources;
pub mod anim;
pub mod oled_shift;
pub mod app;
pub mod messenger;
pub mod child_process;
mod settings_window;
pub mod bt;
pub mod calendar;
mod calendar_ui;
mod local_calendar;
mod calendar_editor;
pub mod glass;
pub mod skin_paint;
pub mod icons;
pub mod input;
pub mod lyrics;
mod s2t;
pub mod mail;
pub mod gws;
pub mod gws_install;
pub mod mail_format;
mod mail_reader;
pub mod mail_manage;
mod mail_manage_ui;
pub mod clipboard;
pub mod side_dock;
mod task_picker;
pub mod model;
pub mod renderer;
pub mod otp;
pub mod quota;
mod quota_ui;
pub mod weather;
mod weather_ui;
mod weather_source_ui;
mod weather_provider;
pub mod cwa_weather;
mod pages;
mod extras;
mod pet;
mod pet_expressions;
mod bar_layout;
mod resident_settings;
mod avoidance;
mod focus_bar;
#[cfg(windows)]
pub mod backdrop;
#[cfg(windows)]
pub mod composition;
#[cfg(windows)]
pub mod layered;
#[cfg(windows)]
pub mod system;
pub mod paint;
pub mod shape;
mod fluid;
pub mod smtc;
pub mod music_volume;
pub mod theme;
pub mod settings_theme;
mod updates;
#[cfg(windows)]
pub use updates::record_install_size;

#[cfg(windows)]
pub mod native;
#[cfg(windows)]
pub mod wechat;
#[cfg(windows)]
pub mod tray;

#[cfg(test)]
mod ui_tests;
#[cfg(test)]
#[path = "bin/daozai-record/raster.rs"]
mod preview_raster;
