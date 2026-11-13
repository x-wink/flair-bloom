//! 按基础连发、多段宏和输出序列迁移大肥鱼配置，转换差异统一进入预览。

use super::*;
use qzh_profile::profile::{OutputStep, MAX_OUTPUT_STEPS};
use serde_json::Value;

mod sequences;
#[cfg(test)]
mod tests;

pub(super) const SOURCE: &str = "大肥鱼按键";

fn is_config(value: &Value) -> bool {
    (value.get("keys").is_some_and(Value::is_array)
        || value.get("sets").is_some_and(Value::is_array))
        && value.get("interval_ms").is_some_and(Value::is_number)
        && value.get("hold_ms").is_some_and(Value::is_number)
        && value.get("toggle_vk").is_some()
        && value.get("cycle_vk").is_some()
        && value.get("toggle_mods").is_some()
}

fn is_macro(value: &Value) -> bool {
    value.get("name").is_some_and(Value::is_string)
        && value.get("created").is_some_and(Value::is_string)
        && value.get("events").is_some_and(Value::is_array)
}

pub(super) fn detect(data: &str) -> bool {
    serde_json::from_str::<Value>(data).is_ok_and(|v| is_config(&v) || is_macro(&v))
}

pub(super) fn prepare(path: &Path, data: &str) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(data).map_err(|e| e.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.remove("_import_macro");
        object.remove("_import_macro_error");
    }
    if is_config(&value) {
        if let Some(name) = value
            .get("macro_name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
        {
            let loaded = (|| {
                if name.contains(['/', '\\', ':']) || matches!(name, "." | "..") {
                    return Err("录制宏名称不是有效文件名".to_string());
                }
                let folder = path.parent().ok_or("配置目录无效")?.join("macros");
                let macro_path = folder.join(format!("{name}.json"));
                let canonical = std::fs::canonicalize(&macro_path).map_err(|e| e.to_string())?;
                let base = std::fs::canonicalize(&folder).map_err(|e| e.to_string())?;
                if !canonical.starts_with(base) {
                    return Err("关联宏不在 macros 目录内".into());
                }
                let text = read_import_text(&canonical)?;
                let recorded: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                if !is_macro(&recorded) {
                    return Err("关联文件不是大肥鱼录制宏".into());
                }
                Ok(recorded)
            })();
            match loaded {
                Ok(recorded) => value["_import_macro"] = recorded,
                Err(error) => value["_import_macro_error"] = error.into(),
            }
        }
    }
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(|v| v.as_f64().or_else(|| v.as_str()?.parse().ok()))
        .filter(|n| n.is_finite())
}

fn key(value: &Value) -> Option<KeyId> {
    let vk = value.as_u64().or_else(|| value.as_str()?.parse().ok())?;
    let mapped = match vk {
        16..=18 => return None,
        232 => KeyId::Mouse(MouseButton::WheelUp),
        233 => KeyId::Mouse(MouseButton::WheelDown),
        1..=255 => vk_to_key_id(vk as u32),
        _ => return None,
    };
    Some(mapped)
}

fn output_key(
    raw: Option<&Value>,
    modifiers: Option<&Value>,
    result: &mut Converted,
) -> Option<KeyId> {
    if modifiers.is_some_and(|v| number(Some(v)) != Some(0.0)) {
        result.warn("组合键已忽略 Ctrl／Shift／Alt／Win 修饰，只保留主键；可能与其他键位重合。");
    }
    raw.and_then(key).filter(|k| {
        qzh_profile::accepts(
            qzh_profile::KeySlot::Target,
            *k,
            qzh_profile::InjectCaps::default(),
        )
        .is_ok()
    })
}

#[derive(Default)]
struct Converted {
    rules: Vec<BurstRule>,
    skipped: usize,
    warnings: Vec<String>,
    hotkeys: Hotkeys,
}

impl Converted {
    fn warn(&mut self, text: impl Into<String>) {
        self.warnings.push(text.into());
    }

    fn add_rule(&mut self, trigger: KeyId, mut steps: Vec<OutputStep>, group: Option<&str>) {
        if steps.is_empty() {
            self.skipped += 1;
            self.warn("没有有效目标的规则已跳过。");
            return;
        }
        if let Some(index) = self.rules.iter().position(|r| r.trigger_key == trigger) {
            if self.rules[index].steps == steps {
                self.warn("同一启动键且目标相同的槽位已合并。");
            } else {
                self.rules[index].steps.append(&mut steps);
                self.warn("忽略修饰键后出现相同启动键，目标已按顺序合并为一条规则。");
            }
            if self.rules[index].steps.len() > MAX_OUTPUT_STEPS {
                self.skipped += self.rules[index].steps.len() - MAX_OUTPUT_STEPS;
                self.rules[index].steps.truncate(MAX_OUTPUT_STEPS);
                self.warn("合并后的目标序列超过 256 步，已保留前 256 步。");
            }
            return;
        }
        if self.rules.len() == MAX_RULES {
            self.skipped += 1;
            self.warn("规则超过数量上限，超出部分已跳过。");
            return;
        }
        self.rules.push(BurstRule {
            id: make_id(),
            enabled: false,
            trigger_key: trigger,
            steps,
            mode: BurstMode::Toggle,
            stop_key: None,
            group: group.map(str::to_string),
        });
    }
}

struct Slot {
    source_index: usize,
    trigger: Option<KeyId>,
    enabled: bool,
    steps: Vec<OutputStep>,
}

fn slots(value: &Value, period: u32, result: &mut Converted) -> Vec<Slot> {
    let mut slots = Vec::new();
    let mut read = |container: &Value, offset: usize, mods: &str, cap: usize| {
        if let Some(raw) = container
            .get("keys")
            .and_then(Value::as_array)
            .filter(|raw| raw.len() > cap)
        {
            result.skipped += raw.len() - cap;
            result.warn("按键槽位超过来源段或规则容量，超出部分已跳过。");
        }
        for (index, raw) in container
            .get("keys")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .take(cap)
            .enumerate()
        {
            let source_index = offset + index;
            let modifiers = container.get(mods).and_then(|v| v.get(index));
            if container.get(mods).is_some_and(|v| !v.is_array()) {
                result.warn("修饰键字段格式无效，已忽略修饰并保留主键。");
            }
            let trigger = output_key(Some(raw), modifiers, result);
            let sequence = container
                .get("key_seqs")
                .and_then(|s| s.get(index))
                .or_else(|| value.get("key_seqs").and_then(|s| s.get(source_index)));
            let mut steps = sequences::targets(sequence, period, result);
            if steps.is_empty() {
                if sequence
                    .is_some_and(|s| !s.is_null() && s.as_array().is_none_or(|s| !s.is_empty()))
                {
                    result.warn("目标序列没有有效按键，已退化为槽位主键连发。");
                }
                if let Some(key) = trigger {
                    steps.push(OutputStep {
                        key,
                        interval_ms: period,
                    });
                }
            }
            let trigger = trigger.or_else(|| steps.first().map(|s| s.key));
            if trigger.is_none() {
                result.skipped += 1;
                result.warn(format!("第 {} 个槽位没有可映射的按键。", source_index + 1));
            }
            slots.push(Slot {
                source_index,
                trigger,
                enabled: container
                    .get("enabled")
                    .and_then(|s| s.get(index))
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                steps,
            });
        }
    };
    // sets 是持久镜像，不能与顶层兼容字段重复合并。
    if let Some(sets) = value
        .get("sets")
        .and_then(Value::as_array)
        .filter(|s| !s.is_empty())
    {
        for (index, set) in sets.iter().take(2).enumerate() {
            read(set, index * 4, "mods", 4);
        }
    } else {
        read(value, 0, "key_mods", MAX_RULES);
    }
    slots
}

fn config_rules(value: &Value, result: &mut Converted) {
    let raw_period = number(value.get("interval_ms")).unwrap_or(100.0);
    let period = raw_period.clamp(20.0, 2000.0) as u32;
    if raw_period != f64::from(period) {
        result.warn("基础连发周期已调整到源主循环的 20～2000 ms 范围。");
    }
    let mut slots = slots(value, period, result);
    if value.get("hold_mode").and_then(Value::as_bool) == Some(true) {
        result.warn("持续按住输出已退化为普通连发，保留按键与目标序列。");
    }
    if value.get("sync_mode").and_then(Value::as_bool) == Some(true) {
        let active: Vec<_> = slots
            .iter()
            .filter(|s| s.enabled)
            .filter_map(|s| s.trigger)
            .collect();
        let gap = (period / u32::try_from(active.len()).unwrap_or(1).max(1))
            .max(MIN_EFFECTIVE_INTERVAL_MS);
        if !active.is_empty() {
            let targets: Vec<_> = active
                .into_iter()
                .map(|key| OutputStep {
                    key,
                    interval_ms: gap,
                })
                .collect();
            for slot in &mut slots {
                slot.steps.clone_from(&targets);
            }
        }
        result.warn("同步发送已退化为按顺序点按的目标序列，不再保证同时按下。");
    }
    let phases = sequences::cycle(value, &slots, result);
    let group = if phases.is_empty() {
        "大肥鱼按键组"
    } else {
        "大肥鱼多段宏"
    };
    let mut order = Vec::new();
    for index in phases.iter().map(|p| p.slot).chain(0..slots.len()) {
        if !order.contains(&index) {
            order.push(index);
        }
    }
    for index in order {
        if let Some(trigger) = slots[index].trigger {
            result.add_rule(trigger, slots[index].steps.clone(), Some(group));
        }
    }
    if !phases.is_empty() {
        result.warn(
            "多段宏按来源顺序导入同一规则组，可直接按各段启动键切换；原切下一段快捷键不直接复现。",
        );
    }
    if value.get("auto_cycle").and_then(Value::as_bool) == Some(true) {
        let phases = if phases.is_empty() {
            sequences::default_cycle(value, &slots)
        } else {
            phases
        };
        if let Some(steps) = sequences::automatic(&phases, &slots, result) {
            let trigger = output_key(value.get("cycle_vk"), value.get("cycle_mods"), result)
                .or_else(|| steps.first().map(|s| s.key));
            if let Some(trigger) = trigger {
                result.add_rule(trigger, steps, Some(group));
            }
        }
    }
    for (prefix, label) in [("fy", "扶摇"), ("ny", "聂云")] {
        if let Some(target) = output_key(value.get(format!("{prefix}_vk")), None, result) {
            let trigger = output_key(
                value.get(format!("{prefix}_toggle_vk")),
                value.get(format!("{prefix}_toggle_mods")),
                result,
            )
            .unwrap_or(target);
            let interval_ms = number(value.get(format!("{prefix}_period_ms")))
                .unwrap_or(40.0)
                .clamp(
                    f64::from(MIN_EFFECTIVE_INTERVAL_MS),
                    f64::from(MAX_INTERVAL_MS),
                ) as u32;
            result.add_rule(
                trigger,
                vec![OutputStep {
                    key: target,
                    interval_ms,
                }],
                None,
            );
            result.warn(format!(
                "{label}辅助动作已转为独立开关规则，定时结束和优先级退化为手动停止。"
            ));
        }
    }
    if let Some(recorded) = value.get("_import_macro") {
        let steps = sequences::recording(recorded, result);
        let trigger = output_key(
            value.get("macro_hot_vk"),
            value.get("macro_hot_mods"),
            result,
        )
        .or_else(|| steps.first().map(|s| s.key));
        if let Some(trigger) = trigger {
            result.add_rule(trigger, steps, None);
        }
    } else if value
        .get("macro_name")
        .and_then(Value::as_str)
        .is_some_and(|n| !n.is_empty())
    {
        result.skipped += 1;
        result.warn(format!(
            "关联录制宏未载入：{}；可在 macros 目录中单独选择录制 JSON 导入。",
            value
                .get("_import_macro_error")
                .and_then(Value::as_str)
                .unwrap_or("没有关联文件")
        ));
    }
    let raw_toggle = value.get("toggle_vk");
    if raw_toggle.is_some_and(|v| number(Some(v)) != Some(0.0)) {
        let toggle = output_key(raw_toggle, value.get("toggle_mods"), result).filter(|key| {
            qzh_profile::accepts(
                qzh_profile::KeySlot::Hotkey,
                *key,
                qzh_profile::InjectCaps::default(),
            )
            .is_ok()
                && !result.rules.iter().any(|r| r.trigger_key == *key)
        });
        result.hotkeys.global_toggle = toggle;
        if toggle.is_none() {
            result.warn("全局开关键不是可用键盘键或与退化后的启动键冲突，未迁移，请重新绑定。");
        }
    }
}

fn converted(data: &str) -> Converted {
    let Ok(value) = serde_json::from_str::<Value>(data) else {
        return Converted::default();
    };
    let mut result = Converted::default();
    result.warn("导入规则全部默认停用，请确认目标和启动键后启用。按启动键开始，再按停止；全局开关不会自动连发当前槽位。");
    result.warn(
        "独立按压时长、窗口限制、声音、驱动和外观不迁移；周期与录制时间间隔会适配气质花范围。",
    );
    if is_config(&value) {
        config_rules(&value, &mut result);
    } else if is_macro(&value) {
        let steps = sequences::recording(&value, &mut result);
        if let Some(first) = steps.first() {
            let trigger = first.key;
            result.add_rule(trigger, steps, None);
            result.warn("独立录制宏没有启动键，暂用首个目标键启动，可在高级设置中修改。");
        }
    }
    if !result.rules.is_empty() {
        let single = result.rules.iter().filter(|r| r.steps.len() == 1).count();
        let groups = result
            .rules
            .iter()
            .filter_map(|r| r.group.as_deref())
            .collect::<HashSet<_>>()
            .len();
        result.warn(format!(
            "识别结果：单键连发 {single} 条、规则组 {groups} 组、多步目标序列 {} 条。",
            result.rules.len() - single
        ));
    }
    result.warnings.sort();
    result.warnings.dedup();
    result
}

pub(super) fn parse_rules(data: &str) -> Vec<BurstRule> {
    converted(data).rules
}
pub(super) fn parse_hotkeys(data: &str) -> Hotkeys {
    converted(data).hotkeys
}
pub(super) fn parse_preview(data: &str, name: &str) -> ImportPreview {
    let result = converted(data);
    let macro_name = serde_json::from_str::<Value>(data)
        .ok()
        .filter(is_macro)
        .and_then(|v| v.get("name").and_then(Value::as_str).map(str::to_string));
    ImportPreview {
        source_app: SOURCE.into(),
        suggested_name: macro_name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| name.into()),
        rule_count: result.rules.len(),
        skipped_count: result.skipped,
        interval_ms: result
            .rules
            .iter()
            .flat_map(|r| &r.steps)
            .map(|s| s.interval_ms)
            .min()
            .unwrap_or(0),
        interval_max_ms: result
            .rules
            .iter()
            .flat_map(|r| &r.steps)
            .map(|s| s.interval_ms)
            .max()
            .unwrap_or(0),
        warnings: result.warnings,
        global_toggle: result.hotkeys.global_toggle,
        global_stop: None,
        panel_toggle: None,
    }
}
