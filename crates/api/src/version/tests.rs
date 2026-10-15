use super::*;

#[test]
fn a_version_is_written_major_minor_patch() {
    assert_eq!(Version::new(1, 20, 3).to_string(), "1.20.3");
}

#[test]
fn versions_order_by_major_then_minor_then_patch() {
    assert!(Version::new(1, 9, 9) < Version::new(2, 0, 0));
    assert!(Version::new(1, 2, 9) < Version::new(1, 10, 0));
    assert!(Version::new(1, 2, 3) < Version::new(1, 2, 4));
}
