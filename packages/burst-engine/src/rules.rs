use std::collections::{HashMap, HashSet};

use qzh_profile::{
    key_id::KeyId,
    profile::{BurstMode, BurstRule},
    MAX_RULES,
};

use crate::{MAX_BURST_INTERVAL_MS, MIN_BURST_INTERVAL_MS};

#[derive(Default)]
pub(crate) struct RuleSnapshot {
    pub(crate) rules: Vec<BurstRule>,
    pub(crate) press_index: HashMap<KeyId, Vec<usize>>,
    pub(crate) hold_release_index: HashMap<KeyId, Vec<usize>>,
}

impl RuleSnapshot {
    pub(crate) fn new(rules: Vec<BurstRule>) -> Self {
        let mut snapshot = Self {
            rules,
            press_index: HashMap::new(),
            hold_release_index: HashMap::new(),
        };
        for (idx, rule) in snapshot.rules.iter().enumerate() {
            if !rule.enabled {
                continue;
            }
            match rule.mode {
                BurstMode::Hold => {
                    push_rule_index(&mut snapshot.press_index, rule.trigger_key, idx);
                    push_rule_index(&mut snapshot.hold_release_index, rule.trigger_key, idx);
                }
                BurstMode::Toggle => {
                    push_rule_index(&mut snapshot.press_index, rule.trigger_key, idx);
                    let stop = rule.stop_key.unwrap_or(rule.trigger_key);
                    if stop != rule.trigger_key {
                        push_rule_index(&mut snapshot.press_index, stop, idx);
                    }
                }
            }
        }
        snapshot
    }
}

fn push_rule_index(index: &mut HashMap<KeyId, Vec<usize>>, key: KeyId, rule_idx: usize) {
    index.entry(key).or_default().push(rule_idx);
}

pub(crate) fn normalize_rules_for_engine(rules: Vec<BurstRule>) -> Vec<BurstRule> {
    let mut normalized = Vec::with_capacity(rules.len().min(MAX_RULES));
    let mut seen_ids = HashSet::new();
    for mut rule in rules {
        if normalized.len() >= MAX_RULES {
            break;
        }
        if !seen_ids.insert(rule.id.clone()) {
            continue;
        }
        rule.interval_ms = rule
            .interval_ms
            .clamp(MIN_BURST_INTERVAL_MS, MAX_BURST_INTERVAL_MS);
        normalized.push(rule);
    }
    normalized
}
