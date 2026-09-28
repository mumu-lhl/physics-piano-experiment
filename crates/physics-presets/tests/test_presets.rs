use physics_presets::{
    guitar_factory_presets, piano_factory_presets, ParamTransition, Preset, PresetManager,
    UndoManager,
};
use std::collections::HashMap;

#[test]
fn test_preset_json_roundtrip() {
    let mut params = HashMap::new();
    params.insert("inharm".to_string(), 1.05);
    params.insert("hardness".to_string(), 0.95);

    let preset = Preset::new("test_preset", "Test Preset", "piano", params);
    let json = preset.to_json().expect("Serialization failed");
    let deserialized = Preset::from_json(&json).expect("Deserialization failed");

    assert_eq!(preset.id, deserialized.id);
    assert_eq!(preset.name, deserialized.name);
    assert_eq!(preset.params, deserialized.params);
}

#[test]
fn test_factory_presets_loading() {
    let piano_presets = piano_factory_presets();
    assert_eq!(piano_presets.len(), 6);
    for p in &piano_presets {
        assert_eq!(p.instrument, "piano");
        assert!(p.is_factory);
        assert!(p.params.contains_key("inharm"));
        assert!(p.params.contains_key("hardness"));
        assert!(p.params.contains_key("lid_angle"));
    }

    let guitar_presets = guitar_factory_presets();
    assert_eq!(guitar_presets.len(), 6);
    for p in &guitar_presets {
        assert_eq!(p.instrument, "guitar");
        assert!(p.is_factory);
        assert!(p.params.contains_key("mode"));
        assert!(p.params.contains_key("tone"));
        assert!(p.params.contains_key("amp_drive"));
    }
}

#[test]
fn test_preset_manager_save_and_delete() {
    let temp_dir =
        std::env::temp_dir().join(format!("physics_presets_test_{}", std::process::id()));
    let factory = piano_factory_presets();
    let mut mgr = PresetManager::with_custom_dir("piano", factory, temp_dir.clone());

    assert_eq!(mgr.presets().len(), 6);

    // Save a custom user preset
    let mut custom_params = HashMap::new();
    custom_params.insert("inharm".to_string(), 1.8);
    let custom = Preset::new("my_custom_piano", "My Custom Piano", "piano", custom_params);
    mgr.save_user_preset(custom.clone())
        .expect("Failed to save");

    assert_eq!(mgr.presets().len(), 7);
    let loaded = mgr.get_preset("my_custom_piano").expect("Preset not found");
    assert_eq!(loaded.name, "My Custom Piano");
    assert!(!loaded.is_factory);

    // Test reload from disk
    let count = mgr.load_user_presets().expect("Failed to reload");
    assert_eq!(count, 1);
    assert_eq!(mgr.presets().len(), 7);

    // Delete user preset
    let deleted = mgr
        .delete_user_preset("my_custom_piano")
        .expect("Failed to delete");
    assert!(deleted);
    assert_eq!(mgr.presets().len(), 6);
    assert!(mgr.get_preset("my_custom_piano").is_none());

    // Cleanup
    let _ = std::fs::remove_dir_all(temp_dir);
}

#[test]
fn test_undo_manager_single_param_gesture() {
    let mut undo_mgr = UndoManager::new(20);

    // 1. Gesture starts: user clicks slider at 0.5
    undo_mgr.begin_gesture("hardness", 0.5);

    // 2. Dragging occurs (continuous changes not recorded yet)
    // ...

    // 3. User releases mouse at 1.25 -> commits single action
    let committed = undo_mgr.end_gesture("hardness", 1.25);
    assert!(committed);
    assert!(undo_mgr.can_undo());
    assert!(!undo_mgr.can_redo());

    // 4. Test Undo
    let restorations = undo_mgr.undo().expect("Undo failed");
    assert_eq!(restorations.len(), 1);
    assert_eq!(restorations[0], ("hardness".to_string(), 0.5));
    assert!(!undo_mgr.can_undo());
    assert!(undo_mgr.can_redo());

    // 5. Test Redo
    let redo_restorations = undo_mgr.redo().expect("Redo failed");
    assert_eq!(redo_restorations.len(), 1);
    assert_eq!(redo_restorations[0], ("hardness".to_string(), 1.25));
    assert!(undo_mgr.can_undo());
    assert!(!undo_mgr.can_redo());
}

#[test]
fn test_undo_manager_batch_param_preset() {
    let mut undo_mgr = UndoManager::new(20);

    let batch_changes = vec![
        ParamTransition {
            param_id: "hardness".to_string(),
            old_value: 1.0,
            new_value: 1.55,
        },
        ParamTransition {
            param_id: "lid_angle".to_string(),
            old_value: 45.0,
            new_value: 60.0,
        },
    ];

    undo_mgr.record_batch("Apply Bright Pop Grand", batch_changes);

    assert!(undo_mgr.can_undo());

    // Undo should restore previous values
    let restorations = undo_mgr.undo().expect("Batch undo failed");
    assert_eq!(restorations.len(), 2);
    // In reverse order of application
    assert_eq!(restorations[0], ("lid_angle".to_string(), 45.0));
    assert_eq!(restorations[1], ("hardness".to_string(), 1.0));

    // Redo should restore preset values
    let redo_restorations = undo_mgr.redo().expect("Batch redo failed");
    assert_eq!(redo_restorations.len(), 2);
    assert_eq!(redo_restorations[0], ("hardness".to_string(), 1.55));
    assert_eq!(redo_restorations[1], ("lid_angle".to_string(), 60.0));
}

#[test]
fn test_preset_manager_rename_and_overwrite() {
    let temp_dir =
        std::env::temp_dir().join(format!("physics_presets_test_edit_{}", std::process::id()));
    let factory = piano_factory_presets();
    let mut mgr = PresetManager::with_custom_dir("piano", factory, temp_dir);

    // Save a custom preset
    let mut custom_params = HashMap::new();
    custom_params.insert("inharm".to_string(), 1.0);
    let custom = Preset::new("user_1", "Original Name", "piano", custom_params);
    mgr.save_user_preset(custom).expect("Failed to save");

    assert!(mgr.is_user_preset("user_1"));
    assert!(!mgr.is_user_preset("steinway_concert_d"));

    // Overwrite parameters
    let mut new_params = HashMap::new();
    new_params.insert("inharm".to_string(), 2.5);
    assert!(mgr
        .overwrite_user_preset("user_1", new_params)
        .expect("Failed to overwrite"));
    assert_eq!(
        mgr.get_preset("user_1").unwrap().params.get("inharm"),
        Some(&2.5)
    );

    // Rename
    assert!(mgr
        .rename_user_preset("user_1", "Renamed Piano")
        .expect("Failed to rename"));
    assert_eq!(mgr.get_preset("user_1").unwrap().name, "Renamed Piano");

    // Clean up
    let _ = mgr.delete_user_preset("user_1");
}
