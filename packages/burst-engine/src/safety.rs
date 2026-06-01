use qzh_profile::key_id::KeyId;
use win_input::key_events;

use crate::{revive, KeyEvent, PhysicalKeys, SimulatedKeys};

fn physical_key_down(physical_keys: &PhysicalKeys, key: KeyId) -> bool {
    revive(physical_keys.lock()).contains(&key)
}

fn record_simulated_down(simulated_keys: &SimulatedKeys, key: KeyId) {
    let mut keys = revive(simulated_keys.lock());
    *keys.entry(key).or_default() += 1;
}

fn record_simulated_up(simulated_keys: &SimulatedKeys, key: KeyId) -> bool {
    let mut keys = revive(simulated_keys.lock());
    let Some(count) = keys.get_mut(&key) else {
        return false;
    };
    if *count <= 1 {
        keys.remove(&key);
    } else {
        *count -= 1;
    }
    true
}

pub(crate) fn plan_key_down(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
    events: &mut Vec<KeyEvent>,
) -> bool {
    if !allow_while_physical_down && physical_key_down(physical_keys, key) {
        return false;
    }
    record_simulated_down(simulated_keys, key);
    events.push((key, false));
    true
}

pub(crate) fn plan_key_up(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) -> Option<KeyEvent> {
    // ⚠️ 账本（simulated_keys）只做尽力更新，不得作为是否发 key_up 的前置条件。
    //
    // 陷阱：cancel_all_loops 的 timeout fallback 会 drain 账本后再发 key_up。
    // 若此处仍以"账本有记录"为前提，scheduler 之后处理 StopAll cleanup 时账本已空，
    // 会静默跳过 key_up，驱动侧按键永久卡住，重启应用甚至重启电脑都无法解除
    // （Windows Fast Startup 下驱动状态随休眠文件恢复，仅「重启」而非「关机」才清）。
    //
    // 不变式：本函数仅通过 release_target_owner / release_all_target_holds 调用，
    // 两者都只在 target_holds 有 owner 时调用，此时键一定处于 simulated-down 状态，
    // 无需账本确认即可安全发 key_up。
    record_simulated_up(simulated_keys, key);
    if allow_while_physical_down || !physical_key_down(physical_keys, key) {
        Some((key, true))
    } else {
        None
    }
}

pub(crate) fn emit_key_events(events: &[KeyEvent]) {
    if !events.is_empty() {
        key_events(events);
    }
}

#[cfg(test)]
pub(crate) fn safe_key_down(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) -> bool {
    let mut events = Vec::new();
    let started = plan_key_down(
        key,
        physical_keys,
        simulated_keys,
        allow_while_physical_down,
        &mut events,
    );
    emit_key_events(&events);
    started
}

#[cfg(test)]
pub(crate) fn safe_key_up(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
    allow_while_physical_down: bool,
) {
    if let Some(event) = plan_key_up(
        key,
        physical_keys,
        simulated_keys,
        allow_while_physical_down,
    ) {
        emit_key_events(&[event]);
    }
}

pub(crate) fn release_simulated_key(
    key: KeyId,
    physical_keys: &PhysicalKeys,
    simulated_keys: &SimulatedKeys,
) {
    let was_down = revive(simulated_keys.lock()).remove(&key).is_some();
    if was_down && !physical_key_down(physical_keys, key) {
        emit_key_events(&[(key, true)]);
    }
}

pub(crate) fn release_simulated_keys(physical_keys: &PhysicalKeys, simulated_keys: &SimulatedKeys) {
    let keys: Vec<_> = revive(simulated_keys.lock())
        .drain()
        .map(|(key, _)| key)
        .collect();
    let mut events = Vec::new();
    for key in keys {
        if !physical_key_down(physical_keys, key) {
            events.push((key, true));
        }
    }
    emit_key_events(&events);
}
