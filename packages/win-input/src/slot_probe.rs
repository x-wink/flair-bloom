//! 用带标记的 F24 验证 Interception 键盘槽是否真正送达。

use interception_sys::*;
use serde::Serialize;
use std::{
    cell::RefCell,
    ptr::null_mut,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const WAIT: Duration = Duration::from_millis(400);
type Event = (i32, bool);
struct Observer {
    base: usize,
    scan: u32,
    events: mpsc::Sender<Event>,
    unmarked: std::sync::Arc<std::sync::atomic::AtomicU32>,
}
thread_local! {
    static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
}

unsafe extern "system" fn observe(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: WH_KEYBOARD_LL 的 HC_ACTION 附带有效的 KBDLLHOOKSTRUCT。
        let key = unsafe { &*(l as *const KBDLLHOOKSTRUCT) };
        if OBSERVER.with_borrow(|state| {
            state.as_ref().is_some_and(|observer| {
                let slot = key.dwExtraInfo.wrapping_sub(observer.base);
                if key.vkCode != u32::from(VK_F24) && key.scanCode != observer.scan {
                    return false;
                }
                if slot > 10 {
                    observer
                        .unmarked
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    return false;
                }
                let _ = observer
                    .events
                    .send((slot as i32, key.flags & LLKHF_UP != 0));
                true
            })
        }) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(null_mut(), code, w, l) }
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
}

fn receive(events: &mpsc::Receiver<Event>, slot: i32, timeout: Duration) -> [bool; 2] {
    let deadline = Instant::now() + timeout;
    let mut seen = [false; 2];
    while let Ok((source, released)) =
        events.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        if source == slot {
            seen[usize::from(released)] = true;
        }
        if seen == [true; 2] || Instant::now() >= deadline {
            break;
        }
    }
    seen
}

pub(super) fn check(ctx: InterceptionContext, ids: &[(i32, String)], all: bool) -> SlotProbe {
    let mut result = SlotProbe::default();
    let vk = u32::from(VK_F24);
    let scan = unsafe { MapVirtualKeyW(vk, MAPVK_VK_TO_VSC_EX) };
    if scan == 0 || scan > 255 || unsafe { MapVirtualKeyW(scan, MAPVK_VSC_TO_VK_EX) } != vk {
        result.error = Some("当前键盘布局无法安全映射 F24".into());
        return result;
    }
    // 区分连续两轮探测的延迟包。正常引擎只看自己的 SIM_MARKER。
    static SEQUENCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let base = (0x5140_0000
        | ((SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed) & 0xffff) << 4))
        as usize;
    let (tx, events) = mpsc::channel();
    let (ready, installed) = mpsc::channel();
    let unmarked = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    thread::scope(|scope| {
        let counter = unmarked.clone();
        scope.spawn(move || {
            OBSERVER.set(Some(Observer {
                base,
                scan,
                events: tx,
                unmarked: counter,
            }));
            let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(observe), null_mut(), 0) };
            if hook.is_null() {
                return;
            }
            unsafe {
                let mut msg: MSG = std::mem::zeroed();
                PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
                if ready.send(GetCurrentThreadId()).is_ok() {
                    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {}
                }
                UnhookWindowsHookEx(hook);
            }
        });
        let Ok(thread_id) = installed.recv() else {
            result.error = Some("无法安装键盘送达监听".into());
            return;
        };
        struct StopObserver(u32);
        impl Drop for StopObserver {
            fn drop(&mut self) {
                unsafe {
                    PostThreadMessageW(self.0, WM_QUIT, 0, 0);
                }
            }
        }
        let _stop = StopObserver(thread_id);
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
        let submitted = unsafe { SendInput(2, pair.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
        result.control_submitted = submitted;
        result.control_received = receive(&events, 0, Duration::from_secs(1));
        if submitted != 2 || result.control_received != [true; 2] {
            result.error = Some("轻量对照未通过，仅能确认有带标记回送的槽。".into());
        }
        for (slot, id) in ids.iter().filter(|(i, _)| (1..=10).contains(i)) {
            let send = |state| {
                let stroke = InterceptionKeyStroke {
                    code: scan as u16,
                    state,
                    information: (base + *slot as usize) as u32,
                };
                unsafe { interception_send(ctx, *slot, (&raw const stroke).cast(), 1) }
            };
            let row = SlotResult {
                slot: *slot,
                hardware_id: id.clone(),
                submitted: [send(0), send(1)],
                received: receive(&events, *slot, WAIT),
            };
            let ok = row.delivered();
            result.slots.push(row);
            if ok && !all {
                break;
            }
        }
    });
    result.unmarked_f24 = unmarked.load(std::sync::atomic::Ordering::Relaxed);
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
    #[ignore = "需要交互式 Windows 桌面和可用的 Interception 驱动，只发送并截获带标记的 F24"]
    fn verify_local_slot_delivery() {
        let result = crate::interception::probe_keyboard_slots().expect("无法打开 Interception");
        eprintln!("{result:?}");
        assert!(result.selected().is_some(), "没有送达的键盘槽");
    }
}
