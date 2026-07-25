use meshy::ui::insert_device_display_name;

#[test]
fn insert_new_device_returns_true() {
    let mut devices = Vec::new();
    let added = insert_device_display_name(
        &mut devices,
        "AA:BB:CC:DD:EE:FF".into(),
        "MeshCore-1".into(),
    );

    assert!(added);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].2, "MeshCore-1 (AA:BB:CC:DD:EE:FF)");
}

#[test]
fn insert_duplicate_name_always_includes_mac() {
    let mut devices = Vec::new();
    let first_added = insert_device_display_name(
        &mut devices,
        "AA:BB:CC:DD:EE:FF".into(),
        "MeshCore-1".into(),
    );
    let second_added = insert_device_display_name(
        &mut devices,
        "11:22:33:44:55:66".into(),
        "MeshCore-1".into(),
    );

    assert!(first_added);
    assert!(second_added);
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].2, "MeshCore-1 (AA:BB:CC:DD:EE:FF)");
    assert_eq!(devices[1].2, "MeshCore-1 (11:22:33:44:55:66)");
}

#[test]
fn insert_same_address_returns_false() {
    let mut devices = Vec::new();
    let first_added = insert_device_display_name(
        &mut devices,
        "AA:BB:CC:DD:EE:FF".into(),
        "MeshCore-1".into(),
    );
    let second_added = insert_device_display_name(
        &mut devices,
        "AA:BB:CC:DD:EE:FF".into(),
        "MeshCore-1".into(),
    );

    assert!(first_added);
    assert!(!second_added);
    assert_eq!(devices.len(), 1);
}
