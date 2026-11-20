# 參與開發

歡迎回報相容性問題、修復 bug 或完善使用說明。提交較大的功能前，先開 Issue 說明具體場景與預期行為。

## 本機開發

安裝 Rust stable、Visual Studio C++ 建置工具和 Windows SDK，複製儲存庫後執行：

```powershell
cargo build --locked --bin daozai
cargo test --locked --all-targets --features record -- --test-threads=1
```

為開發實例指定獨立目錄，避免覆蓋日常使用資料：

```powershell
$env:DAOZAI_DATA_DIR = Join-Path $env:TEMP 'daozai-development'
cargo run --locked --bin daozai
```

一般測試使用合成資料。帶 `#[ignore]` 的真實帳號測試或截圖匯出測試須按需要明確執行；不要在公共 CI 中連線私人帳號。

## 程式碼導覽

| 目錄／檔案 | 內容 |
| --- | --- |
| `src/main.rs` | Windows 外殼、系統狀態和背景服務銜接 |
| `src/app.rs`、`src/pages.rs` | 狀態與主介面 |
| `src/calendar.rs`、`src/calendar_ui.rs` | CalDAV、重複日程、月曆與列表 |
| `src/mail.rs`、`src/mail_reader.rs` | IMAP、MIME、驗證碼與郵件閱讀 |
| `src/quota*.rs` 及 provider 模組 | AI 額度與連線 |
| `src/native.rs`、`src/system.rs` | 原生視窗、輸入、拖放和系統事件 |
| `src/ui_tests.rs` | 無視窗互動與佈局迴歸測試 |
| `src/bin/daozai-record` | 合成資料示範與軟體渲染器 |
| `vendor/` | 少量必要的上游修補，見相依套件說明 |
| `installer/`、`scripts/` | 安裝、解除安裝與打包 |

## 提交要求

修 bug 時提供能重現的測試；介面變更附不含私人資訊的截圖。保持改動聚焦，不把個人設定、授權碼、Cookie、編譯產物或真實帳號資料提交到儲存庫。

原始碼貢獻按儲存庫 MIT 授權提交；第三方素材須同時提交準確的來源與授權聲明。不要修改或移除 vendored 依賴的原始版權聲明。
