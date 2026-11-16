//! OLED 畫素偏移：島體收起靜止時沿停靠邊緩慢移動幾個畫素，
//! 讓常駐的時間、電量等內容不在同一塊畫素上長期停留。
//!
//! 目的是**減輕** OLED 靜態顯示留下的痕跡，不承諾阻止燒屏；預設關閉，
//! 開啟後也不改動使用者儲存的位置（`island_offset` 不受影響）。
//!
//! 只在收起、可見且沒有互動時移動；展開、點選、拖拽時保持當前位置。
//! 移動只沿不貼邊的那一軸進行，貼邊的一側保持嚴絲合縫。
use std::time::Duration;

use crate::model::Dock;

/// 單方向最大位移，物理畫素。
pub const MAX_SHIFT: i32 = 3;
/// 每一步的間隔。
pub const STEP: Duration = Duration::from_secs(45);
/// 一個來回的步數：0 → MAX_SHIFT → -MAX_SHIFT → 0。
pub const CYCLE: u32 = MAX_SHIFT as u32 * 4;

/// 時間表上的位移：0 → 1 → … → MAX_SHIFT → … → -MAX_SHIFT → … → 0。
///
/// 每一步只差 1 個畫素，所以位置不會突然跳變；一個來回 `CYCLE` 步。
pub fn offset(elapsed: Duration) -> i32 {
    let max = MAX_SHIFT as i64;
    if max <= 0 { return 0; }
    let period = max * 4;
    let step = (elapsed.as_millis() / STEP.as_millis()) as i64;
    let phase = step.rem_euclid(period);
    if phase <= max {
        phase as i32
    } else if phase <= max * 3 {
        (max * 2 - phase) as i32
    } else {
        (phase - period) as i32
    }
}

/// 當前應該應用的位移（物理畫素）；`enabled` 為 false 時恆為 `(0, 0)`。
///
/// 頂部停靠只左右移動，左右停靠只上下移動：動了貼邊那一側，劉海和螢幕邊緣之間會裂開。
pub fn shift(enabled: bool, dock: Dock, elapsed: Duration) -> (i32, i32) {
    if !enabled { return (0, 0); }
    let px = offset(elapsed);
    if dock.is_side() { (0, px) } else { (px, 0) }
}

/// 向目標位移靠近，單軸每次最多 1 個畫素。
///
/// 展開、拖拽期間位置保持不變，長時間互動後目標可能已經走到別處；
/// 逐畫素收斂可以避免整塊面板（連同展開動畫）一次跳好幾個畫素。
pub fn toward(current: (i32, i32), target: (i32, i32)) -> (i32, i32) {
    fn step(from: i32, to: i32) -> i32 { from + (to - from).clamp(-1, 1) }
    (step(current.0, target.0), step(current.1, target.1))
}

/// 是否該移動：收起、沒有拖拽、不在編輯。
///
/// 展開、點選、拖拽期間保持當前位置；回到收起靜止後再繼續。
pub fn should_move(expanded: bool, dragging: bool, editing: bool) -> bool {
    !expanded && !dragging && !editing
}

/// 距離下一次位移還有多久。
///
/// 主視窗把它交給現有的重繪排程（`request_repaint_after`），到點醒一次即可，
/// 不額外起執行緒、不做高頻輪詢。剛跨過一步時返回整整一個間隔，不會原地空轉。
pub fn until_next_step(elapsed: Duration) -> Duration {
    let step = STEP.as_millis();
    let left = (step - elapsed.as_millis() % step) as u64;
    Duration::from_millis(left.clamp(150, step as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_moves_one_pixel_and_the_walk_covers_both_sides() {
        let period = CYCLE;
        let mut previous = offset(Duration::ZERO);
        let (mut lowest, mut highest) = (previous, previous);
        assert_eq!(previous, 0, "啟動時先貼邊，不改變預設位置");
        for step in 1..=period * 2 {
            let current = offset(STEP * step);
            assert!((current - previous).abs() == 1, "第 {step} 步只應移動 1 個畫素：{previous} → {current}");
            (lowest, highest) = (lowest.min(current), highest.max(current));
            previous = current;
        }
        assert_eq!((lowest, highest), (-MAX_SHIFT, MAX_SHIFT), "一個來回應兩側都用滿，再回到貼邊位置");
        assert_eq!(offset(STEP * period), 0);
    }

    #[test]
    fn shift_follows_the_free_axis_and_respects_the_switch() {
        let elapsed = STEP * MAX_SHIFT as u32; // 走到位移上限
        for dock in [Dock::Top, Dock::Left, Dock::Right] {
            let side = dock.is_side();
            let (x, y) = shift(true, dock, elapsed);
            assert_eq!((x, y), if side {(0, MAX_SHIFT)} else {(MAX_SHIFT, 0)}, "{dock:?}：只能沿不貼邊的一軸移動");
            assert_eq!(shift(false, dock, elapsed), (0, 0), "{dock:?}：關閉時必須回到貼邊位置");
        }
    }

    #[test]
    fn position_holds_within_a_step_and_starts_over_after_one_cycle() {
        let within = STEP * 2u32 + Duration::from_secs(44);
        assert_eq!(offset(within), offset(STEP * 2u32), "同一步內位置不變，避免視窗抖動");
        let period = STEP * CYCLE;
        for step in [0u32, 1, 5, 11] {
            assert_eq!(offset(STEP * step), offset(period + STEP * step), "一個來回後重復同樣的位移");
        }
        assert!(offset(Duration::MAX).abs() <= MAX_SHIFT, "超長執行時間不應溢位");
    }

    #[test]
    fn the_next_step_wakes_up_exactly_on_the_boundary() {
        let step = STEP.as_millis() as u64;
        assert_eq!(until_next_step(Duration::ZERO), STEP, "剛挪完就約下一個間隔，不空轉");
        assert_eq!(until_next_step(Duration::from_millis(1)), Duration::from_millis(step - 1));
        assert_eq!(until_next_step(STEP * 2u32 + Duration::from_secs(1)), Duration::from_secs(44));
        assert_eq!(until_next_step(STEP * 2u32), STEP);
        for left in [0u64, 1, 100, 149] {
            let wait = until_next_step(STEP * 3u32 + Duration::from_millis(step - left));
            assert!(wait >= Duration::from_millis(150) && wait <= STEP, "剛過邊界時也要留出最小間隔：{left} → {wait:?}");
        }
    }

    #[test]
    fn expanded_dragging_or_editing_holds_the_position() {
        assert!(should_move(false, false, false), "收起靜止時應該移動");
        assert!(!should_move(true, false, false), "展開時保持當前位置");
        assert!(!should_move(false, true, false), "拖拽時保持當前位置");
        assert!(!should_move(false, false, true), "編輯時保持當前位置");
        assert!(!should_move(true, true, true));
    }

    #[test]
    fn reopening_walks_back_one_pixel_at_a_time() {
        let target = shift(false, Dock::Top, Duration::ZERO);
        assert_eq!(target, (0, 0));
        // 展開前的位移是 3，直接恢復目標會讓整塊面板跳 3 畫素。
        let mut current = (3, 0);
        let mut frames = 0;
        while current != (0, 0) && frames < 10 {
            current = toward(current, (0, 0));
            frames += 1;
            assert!(frames <= 3, "最多 3 幀就應該收斂");
        }
        assert_eq!(current, (0, 0));
        assert_eq!(toward((0, 0), (0, 0)), (0, 0), "已經到位時不動");
        assert_eq!(toward((3, 0), (-2, 0)), (2, 0), "單軸每次只走 1 個畫素");
    }
}
