use super::next_menu_index;

#[test]
fn navigation_skips_disabled_items_and_wraps_in_both_directions() {
    let selectable = [false, true, false, true, false];
    let next = |current, direction| {
        next_menu_index(selectable.len(), current, direction, |i| selectable[i])
    };
    assert_eq!(next(-1, 1), 1);
    assert_eq!(next(1, 1), 3);
    assert_eq!(next(3, 1), 1);
    assert_eq!(next(1, -1), 3);
    assert_eq!(next(0, -1), 3);
}

#[test]
fn empty_and_disabled_menus_have_no_selection() {
    assert_eq!(next_menu_index(0, -1, 1, |_| unreachable!()), -1);
    assert_eq!(next_menu_index(5, 2, -1, |_| false), -1);
}

#[test]
fn navigation_bounds_model_access_and_normalizes_stale_indices() {
    assert_eq!(
        next_menu_index(usize::MAX, i32::MAX, 1, |i| {
            assert!(i < 128);
            i == 7
        }),
        7
    );
    assert_eq!(next_menu_index(2, i32::MIN, -1, |i| i == 1), 1);
}
