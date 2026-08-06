use MeshcoreRat::bluetooth::scanner::{is_meshcore_name, normalize_uuids};

#[test]
fn meshcore_name_prefix_matches() {
    assert!(is_meshcore_name("MeshCore-🐲Froogal"));
    assert!(is_meshcore_name("MeshCoreTest"));
}

#[test]
fn non_meshcore_name_rejects() {
    assert!(!is_meshcore_name("NotMeshCore"));
    assert!(!is_meshcore_name("meshcore"));
    assert!(!is_meshcore_name(""));
}

#[test]
fn normalize_uuids_sorts_and_dedups() {
    let uuids = Some(vec!["b".into(), "a".into(), "b".into()]);
    let normalized = normalize_uuids(uuids);
    assert_eq!(normalized, Some(vec!["a".into(), "b".into()]));
}

#[test]
fn normalize_uuids_none_returns_none() {
    assert_eq!(normalize_uuids(None), None);
}
