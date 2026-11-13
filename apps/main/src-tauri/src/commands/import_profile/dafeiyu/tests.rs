use super::*;
use serde_json::json;

fn config() -> Value {
    json!({"keys":[49,50,51,52],"enabled":[true,true,true,true],"interval_ms":100,"hold_ms":10,"toggle_mods":0,"toggle_vk":119,"cycle_vk":120})
}

fn recorded() -> Value {
    json!({"name":"合成录制","created":"2026-01-01","events":[[0,65,2,20],[50,66,0,30],[130,65,0,40]]})
}

fn targets(rule: &BurstRule) -> Vec<KeyId> {
    rule.steps.iter().map(|s| s.key).collect()
}

#[test]
fn recognizes_config_and_named_recording_but_not_other_apps() {
    assert!(detect(&config().to_string()));
    assert!(detect(&recorded().to_string()));
    for value in [
        json!({"delay_us":100000}),
        json!({"keys":[49],"interval_ms":100}),
        json!({"events":[[0,65,0]],"name":"no-created"}),
    ] {
        assert!(!detect(&value.to_string()));
    }
    assert!(!detect("invalid"));
}

#[test]
fn imports_six_slots_from_sets_without_duplicating_top_level_mirror() {
    let mut value = config();
    value["sets"] = json!([{"keys":[65,66,67,68],"mods":[0,0,0,0],"enabled":[true,false,true,true]},{"keys":[69,70]}]);
    let data = value.to_string();
    let rules = parse_rules(&data);
    assert_eq!(
        rules.iter().map(|r| r.trigger_key).collect::<Vec<_>>(),
        (65..=70).map(KeyId::Keyboard).collect::<Vec<_>>()
    );
    assert!(rules.iter().all(|r| !r.enabled
        && r.mode == BurstMode::Toggle
        && r.group.is_some()
        && r.steps[0].interval_ms == 100));
    assert_eq!(parse_preview(&data, "test").rule_count, 6);
    assert_eq!(
        parse_hotkeys(&data).global_toggle,
        Some(KeyId::Keyboard(119))
    );
}

#[test]
fn strips_combo_modifiers_and_keeps_remaining_main_keys() {
    let mut value = config();
    value["keys"] = json!([65, 65, 16, 256, 66, -1]);
    value["key_mods"] = json!([0, 1, 0, 0, 2, 0]);
    let preview = parse_preview(&value.to_string(), "test");
    assert_eq!((preview.rule_count, preview.skipped_count), (2, 3));
    assert!(preview
        .warnings
        .iter()
        .any(|w| w.contains("忽略") && w.contains("修饰")));
}

#[test]
fn imports_target_sequences_in_order_including_repeated_keys_and_mouse_wheel() {
    let mut value = config();
    value["keys"] = json!([65]);
    value["key_seqs"] = json!([[[65, 2], [65, 0], [5, 1], [232, 0], [233, 0]]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(
        targets(&rules[0]),
        vec![
            KeyId::Keyboard(65),
            KeyId::Keyboard(65),
            KeyId::Mouse(MouseButton::X1),
            KeyId::Mouse(MouseButton::WheelUp),
            KeyId::Mouse(MouseButton::WheelDown)
        ]
    );
    assert!(rules[0].steps.iter().all(|s| s.interval_ms == 100));
}

#[test]
fn keeps_valid_target_steps_and_falls_back_when_all_steps_are_invalid() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["key_seqs"] = json!([[[67, 0], [-1, 0], [68, 0]], [[16, 0]]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(
        targets(&rules[0]),
        vec![KeyId::Keyboard(67), KeyId::Keyboard(68)]
    );
    assert_eq!(targets(&rules[1]), vec![KeyId::Keyboard(66)]);
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 2);
}

#[test]
fn merges_distinct_sequences_after_modifier_degradation_creates_a_trigger_collision() {
    let mut value = config();
    value["keys"] = json!([65, 65]);
    value["key_mods"] = json!([1, 2]);
    value["key_seqs"] = json!([[[66, 0]], [[67, 0]]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 1);
    assert_eq!(
        targets(&rules[0]),
        vec![KeyId::Keyboard(66), KeyId::Keyboard(67)]
    );
}

#[test]
fn preserves_multi_part_macro_group_order_from_both_slot_sections() {
    let mut value = config();
    value["sets"] = json!([{"keys":[65,66,67,68]},{"keys":[69,70]}]);
    value["custom_cycle"] = json!(true);
    value["cycle_steps"] = json!([[1, 1], [0, 2], [1, 1], [9, 0]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(
        rules.iter().map(|r| r.trigger_key).collect::<Vec<_>>(),
        vec![70, 67, 65, 66, 68, 69]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
    assert!(rules
        .iter()
        .all(|r| r.group.as_deref() == Some("大肥鱼多段宏")));
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 1);
}

#[test]
fn supports_legacy_macro_order_without_dropping_unreferenced_slots() {
    let mut value = config();
    value["cycle_order"] = json!("3→1→3");
    let rules = parse_rules(&value.to_string());
    assert_eq!(
        rules.iter().map(|r| r.trigger_key).collect::<Vec<_>>(),
        vec![51, 49, 50, 52]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
}

#[test]
fn expands_automatic_macro_phases_and_keeps_their_dwell_durations() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["auto_cycle"] = json!(true);
    value["cycle_steps"] = json!([[0, 1], [0, 0], [0, 1]]);
    value["cycle_secs"] = json!([0.3, 0.2, 0.1]);
    let rules = parse_rules(&value.to_string());
    let automatic = rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(120))
        .unwrap();
    assert_eq!(
        targets(automatic),
        vec![66, 66, 66, 65, 65, 66]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        automatic.steps.iter().map(|s| s.interval_ms).sum::<u32>(),
        600
    );
}

#[test]
fn long_automatic_phases_keep_all_segments_within_step_and_interval_limits() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["interval_ms"] = json!(20);
    value["auto_cycle"] = json!(true);
    value["cycle_secs"] = json!([60, 60]);
    let rules = parse_rules(&value.to_string());
    let automatic = rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(120))
        .unwrap();
    assert_eq!(automatic.steps.len(), MAX_OUTPUT_STEPS);
    assert_eq!(
        automatic.steps.iter().map(|s| s.interval_ms).sum::<u32>(),
        120000
    );
    assert!(
        targets(automatic).contains(&KeyId::Keyboard(65))
            && targets(automatic).contains(&KeyId::Keyboard(66))
    );
    automatic.validate().unwrap();
}

#[test]
fn automatic_cycle_keeps_disabled_source_parts_out_of_its_output() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["enabled"] = json!([false, true]);
    value["auto_cycle"] = json!(true);
    let rules = parse_rules(&value.to_string());
    let automatic = rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(120))
        .unwrap();
    assert!(targets(automatic)
        .iter()
        .all(|key| *key == KeyId::Keyboard(66)));
}

#[test]
fn degrades_output_hold_and_simultaneous_keys_instead_of_discarding_rules() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["hold_mode"] = json!(true);
    value["sync_mode"] = json!(true);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 2);
    assert!(rules
        .iter()
        .all(|r| targets(r) == vec![KeyId::Keyboard(65), KeyId::Keyboard(66)]));
    assert!(rules[0].steps.iter().all(|s| s.interval_ms == 50));
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 0);
}

#[test]
fn imports_recorded_macro_timestamps_as_output_intervals() {
    let data = recorded().to_string();
    let rules = parse_rules(&data);
    assert_eq!(rules.len(), 1);
    assert_eq!(
        targets(&rules[0]),
        vec![65, 66, 65]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        rules[0]
            .steps
            .iter()
            .map(|s| s.interval_ms)
            .collect::<Vec<_>>(),
        vec![50, 80, 40]
    );
    assert_eq!(parse_preview(&data, "folder").suggested_name, "合成录制");
    assert!(!rules[0].enabled);
}

#[test]
fn recordings_with_overlaps_bad_events_and_long_gaps_still_keep_valid_targets() {
    let mut value = recorded();
    value["events"] = json!([
        [10, 65, 0, 5],
        [10, 66, 0],
        [5, 67, 1],
        [20000, 68, 0, 20000],
        [0, -1, 0]
    ]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(
        rules[0]
            .steps
            .iter()
            .map(|s| s.interval_ms)
            .collect::<Vec<_>>(),
        vec![10, 10, 10000, 10000]
    );
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 1);
}

#[test]
fn oversized_target_sequences_are_bounded_and_reported() {
    let mut value = config();
    value["keys"] = json!([65]);
    value["key_seqs"] = json!([vec![vec![66, 0]; MAX_OUTPUT_STEPS + 3]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules[0].steps.len(), MAX_OUTPUT_STEPS);
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 3);
}

#[test]
fn converts_timed_auxiliary_actions_to_manual_toggle_rules() {
    let mut value = config();
    value["fy_vk"] = json!(56);
    value["fy_toggle_vk"] = json!(118);
    value["fy_period_ms"] = json!(40);
    value["ny_vk"] = json!(57);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 6);
    let auxiliary = rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(118))
        .unwrap();
    assert_eq!(targets(auxiliary), vec![KeyId::Keyboard(56)]);
    assert_eq!(auxiliary.group, None);
}

#[test]
fn degrades_global_hotkey_modifiers_but_reports_unresolvable_conflicts() {
    let mut value = config();
    value["toggle_mods"] = json!(2);
    assert_eq!(
        parse_hotkeys(&value.to_string()).global_toggle,
        Some(KeyId::Keyboard(119))
    );
    for vk in [6, 232, 49, 999] {
        value["toggle_vk"] = json!(vk);
        assert_eq!(parse_hotkeys(&value.to_string()).global_toggle, None);
    }
}

#[test]
fn invalid_data_never_fabricates_output() {
    assert!(parse_rules("invalid").is_empty());
    let mut value = config();
    value["sets"] = json!([{"keys":null}]);
    assert!(parse_rules(&value.to_string()).is_empty());
    let mut value = config();
    value["interval_ms"] = json!(0);
    assert!(parse_rules(&value.to_string())
        .iter()
        .all(|r| r.steps[0].interval_ms == 20));
}

#[test]
fn loading_a_config_also_loads_its_named_macro_without_changing_source_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("macros")).unwrap();
    let path = directory.path().join("config.json");
    let macro_path = directory.path().join("macros/合成录制.json");
    let mut value = config();
    value["macro_name"] = json!("合成录制");
    value["macro_hot_vk"] = json!(121);
    value["macro_hot_mods"] = json!(2);
    let bytes = format!("\u{feff}{}", recorded());
    std::fs::write(&macro_path, &bytes).unwrap();
    std::fs::write(&path, value.to_string()).unwrap();
    let preview = preview_import(path.display().to_string(), None).unwrap();
    let profile = convert_profile(&path, None).unwrap();
    assert_eq!(preview.rule_count, 5);
    let rule = profile
        .rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(121))
        .unwrap();
    assert_eq!(
        targets(rule),
        vec![65, 66, 65]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
    assert_eq!(std::fs::read_to_string(&macro_path).unwrap(), bytes);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), value.to_string());
}

#[test]
fn standalone_recording_with_arbitrary_json_name_is_scanned_previewed_and_converted() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("组合录制.JSON");
    std::fs::write(&path, recorded().to_string()).unwrap();
    let found = scan_targets(vec![directory.path().to_path_buf(); 2], None);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].display_name, "合成录制");
    let preview = preview_import(path.display().to_string(), None).unwrap();
    let profile = convert_profile(&path, None).unwrap();
    assert_eq!(preview.rule_count, profile.rules.len());
    assert_eq!(profile.rules[0].steps.len(), 3);
    let unrelated = directory.path().join("unrelated.json");
    std::fs::write(&unrelated, "{\"events\":[]}").unwrap();
    assert!(read_import(&unrelated, None).is_err());
}

#[test]
fn missing_unsafe_oversized_or_invalid_related_macros_do_not_block_basic_rules() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    for name in ["不存在", "../其他", "C:\\private"] {
        let mut value = config();
        value["macro_name"] = json!(name);
        value["_import_macro"] = recorded();
        let prepared = prepare(&path, &value.to_string()).unwrap();
        assert_eq!(parse_rules(&prepared).len(), 4);
        assert_eq!(parse_preview(&prepared, "test").skipped_count, 1);
    }
    std::fs::create_dir(directory.path().join("macros")).unwrap();
    let macro_path = directory.path().join("macros/过大.json");
    std::fs::File::create(&macro_path)
        .unwrap()
        .set_len(IMPORT_MAX_BYTES + 1)
        .unwrap();
    let mut value = config();
    value["macro_name"] = json!("过大");
    let prepared = prepare(&path, &value.to_string()).unwrap();
    assert_eq!(parse_rules(&prepared).len(), 4);
    assert!(parse_preview(&prepared, "test")
        .warnings
        .iter()
        .any(|w| w.contains("4 MB")));
    std::fs::write(&macro_path, "{}").unwrap();
    assert_eq!(
        parse_rules(&prepare(&path, &value.to_string()).unwrap()).len(),
        4
    );
}

#[test]
fn automatic_cycle_preserves_manual_sequences_when_full_expansion_cannot_fit() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["auto_cycle"] = json!(true);
    value["key_seqs"] = json!([
        vec![vec![67, 0]; MAX_OUTPUT_STEPS],
        vec![vec![68, 0]; MAX_OUTPUT_STEPS]
    ]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|r| r.steps.len() == MAX_OUTPUT_STEPS));
    assert!(parse_preview(&value.to_string(), "test")
        .warnings
        .iter()
        .any(|w| w.contains("未生成自动轮换")));
}

#[test]
fn automatic_macro_repeats_each_parts_target_sequence_without_losing_its_order() {
    let mut value = config();
    value["keys"] = json!([65, 66]);
    value["key_seqs"] = json!([[[70, 0], [71, 2]], [[72, 0]]]);
    value["auto_cycle"] = json!(true);
    value["cycle_secs"] = json!([0.4, 0.2]);
    let rules = parse_rules(&value.to_string());
    let automatic = rules
        .iter()
        .find(|r| r.trigger_key == KeyId::Keyboard(120))
        .unwrap();
    assert_eq!(
        targets(automatic),
        vec![70, 71, 70, 71, 72, 72]
            .into_iter()
            .map(KeyId::Keyboard)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        automatic.steps.iter().map(|s| s.interval_ms).sum::<u32>(),
        600
    );
}

#[test]
fn automatic_macro_with_all_parts_disabled_keeps_its_manual_group() {
    let mut value = config();
    value["enabled"] = json!([false, false, false, false]);
    value["auto_cycle"] = json!(true);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 4);
    assert!(parse_preview(&value.to_string(), "test")
        .warnings
        .iter()
        .any(|w| w.contains("没有已启用")));
}

#[test]
fn preserves_historical_two_full_sections_instead_of_discarding_saved_parts() {
    let mut value = config();
    value["sets"] = json!([{"keys":[65,66,67,68]},{"keys":[69,70,71,72]}]);
    value["cycle_steps"] = json!([[1, 3], [1, 2]]);
    let rules = parse_rules(&value.to_string());
    assert_eq!(rules.len(), 8);
    assert_eq!(rules[0].trigger_key, KeyId::Keyboard(72));
    assert_eq!(rules[1].trigger_key, KeyId::Keyboard(71));
}

#[test]
fn long_legacy_cycle_order_reports_its_truncated_references() {
    let mut value = config();
    value["cycle_order"] = json!("1".repeat(MAX_OUTPUT_STEPS + 3));
    assert_eq!(parse_preview(&value.to_string(), "test").skipped_count, 3);
}

#[cfg(unix)]
#[test]
fn associated_macro_cannot_follow_a_symlink_outside_its_directory() {
    let directory = tempfile::tempdir().unwrap();
    let macros = directory.path().join("macros");
    std::fs::create_dir(&macros).unwrap();
    let outside = directory.path().join("outside.json");
    std::fs::write(&outside, recorded().to_string()).unwrap();
    std::os::unix::fs::symlink(&outside, macros.join("linked.json")).unwrap();
    let mut value = config();
    value["macro_name"] = json!("linked");
    let prepared = prepare(&directory.path().join("config.json"), &value.to_string()).unwrap();
    assert_eq!(parse_rules(&prepared).len(), 4);
    assert!(parse_preview(&prepared, "test")
        .warnings
        .iter()
        .any(|w| w.contains("不在 macros")));
}
