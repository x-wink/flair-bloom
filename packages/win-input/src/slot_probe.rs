//! 逐槽验证写入的 F24 是否通过应用的键盘输入入口送达。

use crate::{revive, KeyboardInputSource};
use interception_sys::*;
use serde::Serialize;
use std::{
    sync::{
        atomic::{AtomicU32, Ordering},
        mpsc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

const WAIT: Duration = Duration::from_millis(400);
const MARKER_PREFIX: usize = 0x534C_0000;
static SEQUENCE: AtomicU32 = AtomicU32::new(0);
static ACTIVE: Mutex<Option<Sample>> = Mutex::new(None);

struct Sample {
    marker: usize,
    started_ms: u64,
    events: mpsc::Sender<bool>,
    unmarked: u32,
}

impl Sample {
    fn observe(&mut self, source: KeyboardInputSource, is_up: bool) -> bool {
        match source {
            KeyboardInputSource::Native(marker) if marker != self.marker => {
                self.unmarked += 1;
                return marker & !0xffff == MARKER_PREFIX;
            }
            // WebView 不携带驱动标记。用按键发生时间排除上一个槽迟到的 IPC 和失焦补发的抬起。
            KeyboardInputSource::WebView(time) if time.is_none_or(|ms| ms < self.started_ms) => {
                return false;
            }
            _ => {}
        }
        let _ = self.events.send(is_up);
        true
    }
}

pub fn observe_slot_key(key: qzh_profile::KeyId, source: KeyboardInputSource, is_up: bool) -> bool {
    if key != qzh_profile::KeyId::Keyboard(u32::from(VK_F24)) {
        return false;
    }
    if let Some(active) = revive(ACTIVE.lock()).as_mut() {
        return active.observe(source, is_up);
    }
    matches!(source, KeyboardInputSource::Native(marker) if marker & !0xffff == MARKER_PREFIX)
}

#[derive(Debug, Serialize)]
pub struct SlotResult {
    slot: i32,
    hardware_id: String,
    submitted: [i32; 2],
    received: [bool; 2],
}
impl SlotResult {
    fn delivered(&self) -> bool {
        self.submitted == [1, 1] && self.received == [true, true]
    }
}
#[derive(Debug, Default, Serialize)]
pub struct SlotProbe {
    control_submitted: u32,
    unmarked_f24: u32,
    control_received: [bool; 2],
    pub error: Option<String>,
    slots: Vec<SlotResult>,
}
impl SlotProbe {
    pub fn selected(&self) -> Option<i32> {
        self.slots
            .iter()
            .find(|row| row.delivered())
            .map(|row| row.slot)
    }

    fn scan_slots(
        &mut self,
        ids: &[(i32, String)],
        all: bool,
        mut probe: impl FnMut(i32) -> ([i32; 2], [bool; 2], u32),
    ) {
        for (slot, hardware_id) in ids.iter().filter(|(slot, _)| (1..=10).contains(slot)) {
            let (submitted, received, unmarked) = probe(*slot);
            let row = SlotResult {
                slot: *slot,
                hardware_id: hardware_id.clone(),
                submitted,
                received,
            };
            let delivered = row.delivered();
            tracing::info!(slot, ?submitted, ?received, "游戏模式键盘槽送达");
            self.slots.push(row);
            self.unmarked_f24 += unmarked;
            if delivered && !all {
                break;
            }
        }
    }
}

fn observe_write<T>(
    marker: usize,
    timeout: Duration,
    write: impl FnOnce() -> T,
) -> (T, [bool; 2], u32) {
    let (tx, events) = mpsc::channel();
    let started = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    *revive(ACTIVE.lock()) = Some(Sample {
        marker,
        started_ms: started.as_millis() as u64,
        events: tx,
        unmarked: 0,
    });
    let submitted = write();
    let deadline = Instant::now() + timeout;
    let mut seen = [false; 2];
    while let Ok(up) = events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        // 单独的抬起不能与随后另一轮的按下拼成一次送达。
        if !up || seen[0] {
            seen[usize::from(up)] = true;
        }
        if seen == [true; 2] || Instant::now() >= deadline {
            break;
        }
    }
    let unmarked = revive(ACTIVE.lock())
        .take()
        .map_or(0, |sample| sample.unmarked);
    (submitted, seen, unmarked)
}

pub(super) fn check(ctx: InterceptionContext, ids: &[(i32, String)], all: bool) -> SlotProbe {
    let mut result = SlotProbe::default();
    let vk = u32::from(VK_F24);
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
    if scan == 0 || scan > 255 || unsafe { MapVirtualKeyW(scan, MAPVK_VSC_TO_VK_EX) } != vk {
        result.error = Some("当前键盘布局无法安全映射 F24".into());
        return result;
    }
    // 调用方持有驱动 context 锁，启用与导出不会交错发样本。输入回调只访问 ACTIVE。
    let base = MARKER_PREFIX | ((SEQUENCE.fetch_add(1, Ordering::Relaxed) as usize & 0xfff) << 4);
    if all {
        let input = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wScan: scan as u16,
                    dwFlags: KEYEVENTF_SCANCODE | flags,
                    dwExtraInfo: base,
                    ..unsafe { std::mem::zeroed() }
                },
            },
        };
        let pair = [input(0), input(KEYEVENTF_KEYUP)];
        let (submitted, seen, unmarked) = observe_write(base, Duration::from_secs(1), || unsafe {
            SendInput(2, pair.as_ptr(), std::mem::size_of::<INPUT>() as i32)
        });
        result.control_submitted = submitted;
        result.control_received = seen;
        result.unmarked_f24 += unmarked;
        if submitted != 2 || seen != [true; 2] {
            result.error = Some("轻量对照未通过，仅能确认实际收到回送的槽。".into());
        }
    }
    result.scan_slots(ids, all, |slot| {
        let marker = base + slot as usize;
        let send = |state| {
            let stroke = InterceptionKeyStroke {
                code: scan as u16,
                state,
                information: marker as u32,
            };
            unsafe { interception_send(ctx, slot, (&raw const stroke).cast(), 1) }
        };
        observe_write(marker, WAIT, || [send(0), send(1)])
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_ids_require_both_observations_and_successful_writes() {
        let row = |slot, submitted, received| SlotResult {
            slot,
            hardware_id: "相同硬件".into(),
            submitted,
            received,
        };
        let mut probe = SlotProbe {
            slots: vec![
                row(1, [1, 1], [false, false]),
                row(4, [1, 1], [true, false]),
                row(5, [7, 7], [true, true]),
            ],
            ..Default::default()
        };
        assert_eq!(probe.selected(), None);
        probe.slots.push(row(6, [1, 1], [true, true]));
        assert_eq!(probe.selected(), Some(6));
    }

    #[test]
    fn relay_accepts_current_edges_and_excludes_old_or_cleanup_events() {
        let (events, rx) = mpsc::channel();
        let mut sample = Sample {
            marker: MARKER_PREFIX + 3,
            started_ms: 1200,
            events,
            unmarked: 0,
        };
        assert!(!sample.observe(KeyboardInputSource::WebView(Some(1199)), false));
        assert!(!sample.observe(KeyboardInputSource::WebView(None), true));
        assert!(sample.observe(KeyboardInputSource::Native(MARKER_PREFIX + 2), true));
        assert!(rx.try_recv().is_err());
        assert!(sample.observe(KeyboardInputSource::WebView(Some(1200)), false));
        assert!(sample.observe(KeyboardInputSource::WebView(Some(1201)), true));
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), [false, true]);
    }

    #[test]
    fn activation_stops_at_first_delivered_slot_and_diagnosis_checks_the_rest() {
        let ids = [1, 2, 4, 6, 11].map(|slot| (slot, String::from("同一设备")));
        for (all, expected) in [(false, vec![1, 2, 4]), (true, vec![1, 2, 4, 6])] {
            let mut visited = Vec::new();
            let mut result = SlotProbe::default();
            result.scan_slots(&ids, all, |slot| {
                visited.push(slot);
                match slot {
                    1 => ([1, 1], [false, false], 0),
                    2 => ([1, 1], [true, false], 0),
                    _ => ([1, 1], [true, true], 0),
                }
            });
            assert_eq!(visited, expected);
            assert_eq!(result.selected(), Some(4));
        }
    }
}
