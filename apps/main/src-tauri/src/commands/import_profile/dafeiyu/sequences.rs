use super::*;

pub(super) fn targets(raw: Option<&Value>, period: u32, result: &mut Converted) -> Vec<OutputStep> {
    let Some(raw) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    if raw.len() > MAX_OUTPUT_STEPS {
        result.skipped += raw.len() - MAX_OUTPUT_STEPS;
        result.warn("目标序列超过 256 步，已保留前 256 步。");
    }
    raw.iter()
        .take(MAX_OUTPUT_STEPS)
        .filter_map(|item| {
            let (key, modifiers) = if item.is_array() {
                (item.get(0), item.get(1))
            } else {
                (Some(item), None)
            };
            if let Some(key) = output_key(key, modifiers, result) {
                Some(OutputStep {
                    key,
                    interval_ms: period,
                })
            } else {
                result.skipped += 1;
                result.warn("目标序列中的无效按键已跳过，其余顺序保留。");
                None
            }
        })
        .collect()
}

pub(super) struct Phase {
    pub(super) slot: usize,
    pub(super) duration_ms: u32,
}

fn duration(value: &Value, index: usize) -> u32 {
    let seconds = number(value.get("cycle_secs").and_then(|s| s.get(index)))
        .or_else(|| number(value.get("auto_cycle_sec")))
        .unwrap_or(0.5);
    (seconds.clamp(0.1, 60.0) * 1000.0).round() as u32
}

pub(super) fn cycle(value: &Value, slots: &[Slot], result: &mut Converted) -> Vec<Phase> {
    let raw = value.get("cycle_steps").and_then(Value::as_array);
    let indices: Vec<Option<usize>> = if let Some(raw) = raw.filter(|v| !v.is_empty()) {
        if raw.len() > MAX_OUTPUT_STEPS {
            result.warn("多段宏超过 256 段，仅保留前 256 段引用。");
            result.skipped += raw.len() - MAX_OUTPUT_STEPS;
        }
        raw.iter()
            .take(MAX_OUTPUT_STEPS)
            .map(|row| {
                let set = row.get(0)?.as_u64()?;
                let slot = row.get(1)?.as_u64()?;
                if set >= 2 || slot >= 4 {
                    return None;
                }
                usize::try_from(set * 4 + slot).ok()
            })
            .collect()
    } else {
        let order = value
            .get("cycle_order")
            .and_then(Value::as_str)
            .unwrap_or("");
        let digits = order
            .chars()
            .filter_map(|ch| ch.to_digit(10).filter(|n| (1..=6).contains(n)));
        let count = digits.clone().count();
        if count > MAX_OUTPUT_STEPS {
            result.skipped += count - MAX_OUTPUT_STEPS;
            result.warn("多段宏超过 256 段，仅保留前 256 段引用。");
        }
        digits
            .take(MAX_OUTPUT_STEPS)
            .map(|n| Some(n as usize - 1))
            .collect()
    };
    indices
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| {
            let slot = source.and_then(|source| {
                slots
                    .iter()
                    .position(|s| s.source_index == source && s.trigger.is_some())
            });
            if let Some(slot) = slot {
                Some(Phase {
                    slot,
                    duration_ms: duration(value, index),
                })
            } else {
                result.skipped += 1;
                result.warn("多段宏中不存在的槽位引用已跳过，其余顺序保留。");
                None
            }
        })
        .collect()
}

pub(super) fn default_cycle(value: &Value, slots: &[Slot]) -> Vec<Phase> {
    slots
        .iter()
        .enumerate()
        .filter(|(_, s)| s.trigger.is_some())
        .map(|(index, _)| Phase {
            slot: index,
            duration_ms: duration(value, index),
        })
        .collect()
}

pub(super) fn automatic(
    phases: &[Phase],
    slots: &[Slot],
    result: &mut Converted,
) -> Option<Vec<OutputStep>> {
    let phases: Vec<_> = phases
        .iter()
        .filter(|phase| slots[phase.slot].enabled && !slots[phase.slot].steps.is_empty())
        .collect();
    if phases.is_empty() {
        result.warn("自动轮换没有已启用的有效来源段，仅保留手动规则组。");
        return None;
    }
    // 每段至少保留完整目标序列，并让长驻留能拆进单步 10 秒上限。
    let minimum: Vec<_> = phases
        .iter()
        .map(|p| {
            slots[p.slot]
                .steps
                .len()
                .max(p.duration_ms.div_ceil(MAX_INTERVAL_MS) as usize)
        })
        .collect();
    let min_total: usize = minimum.iter().sum();
    if min_total > MAX_OUTPUT_STEPS {
        result.warn(
            "自动轮换的完整目标超过 256 步，已保留各段目标与手动规则组，未生成自动轮换规则。",
        );
        return None;
    }
    let desired: Vec<_> = phases
        .iter()
        .zip(&minimum)
        .map(|(p, min)| {
            (p.duration_ms.div_ceil(slots[p.slot].steps[0].interval_ms) as usize).max(*min)
        })
        .collect();
    let total: usize = desired.iter().sum();
    let counts = if total <= MAX_OUTPUT_STEPS {
        desired
    } else {
        result.warn("自动轮换已适配 256 步上限：保留各段目标和驻留时长，降低展开后的点按频率。");
        let budget = MAX_OUTPUT_STEPS - min_total;
        let extra = total - min_total;
        let mut counts: Vec<_> = minimum
            .iter()
            .zip(&desired)
            .map(|(min, want)| min + budget * (want - min) / extra)
            .collect();
        let mut remaining = MAX_OUTPUT_STEPS - counts.iter().sum::<usize>();
        for (count, want) in counts.iter_mut().zip(&desired) {
            let add = remaining.min(*want - *count);
            *count += add;
            remaining -= add;
        }
        counts
    };
    let mut steps = Vec::new();
    for (phase, count) in phases.into_iter().zip(counts) {
        let targets = &slots[phase.slot].steps;
        let count_u32 = count as u32;
        for index in 0..count {
            let raw = phase.duration_ms / count_u32
                + u32::from((index as u32) < phase.duration_ms % count_u32);
            let interval_ms = raw.clamp(MIN_EFFECTIVE_INTERVAL_MS, MAX_INTERVAL_MS);
            if raw != interval_ms {
                result.warn("过短驻留已延长，以容纳完整目标序列和 10 ms 最小间隔。");
            }
            steps.push(OutputStep {
                key: targets[index % targets.len()].key,
                interval_ms,
            });
        }
    }
    result.warn("自动轮换已展开为循环输出序列，由原切键热键启动／停止；各段切换可改用手动规则组。");
    Some(steps)
}

pub(super) fn recording(value: &Value, result: &mut Converted) -> Vec<OutputStep> {
    let Some(events) = value.get("events").and_then(Value::as_array) else {
        return Vec::new();
    };
    if events.len() > MAX_OUTPUT_STEPS {
        result.skipped += events.len() - MAX_OUTPUT_STEPS;
        result.warn("录制宏超过 256 个事件，仅保留前 256 个事件。");
    }
    let mut taps = Vec::new();
    for event in events.iter().take(MAX_OUTPUT_STEPS) {
        let time = number(event.get(0)).filter(|v| *v >= 0.0);
        let key = output_key(event.get(1), event.get(2), result);
        if let Some((time, key)) = time.zip(key) {
            let hold = number(event.get(3)).unwrap_or(40.0).max(0.0);
            taps.push((time, key, hold));
        } else {
            result.skipped += 1;
            result.warn("录制宏中的无效事件已跳过，其余目标保留。");
        }
    }
    let mut steps = Vec::new();
    for (index, (time, key, hold)) in taps.iter().enumerate() {
        let raw = taps.get(index + 1).map_or(*hold, |next| next.0 - time);
        let interval_ms = raw
            .clamp(
                f64::from(MIN_EFFECTIVE_INTERVAL_MS),
                f64::from(MAX_INTERVAL_MS),
            )
            .round() as u32;
        if raw < f64::from(MIN_EFFECTIVE_INTERVAL_MS) || raw > f64::from(MAX_INTERVAL_MS) {
            result.warn(
                "录制事件的重叠、倒序或过长／过短间隔已适配到 10～10000 ms，按文件顺序点按。",
            );
        }
        steps.push(OutputStep {
            key: *key,
            interval_ms,
        });
    }
    result.warn("录制宏保留目标顺序和相邻事件间隔；忽略开头等待与独立按住时长，单次回放退化为切换式循环，需手动停止。");
    steps
}
