use super::*;

#[test]
fn full_paths_and_small_budgets_keep_the_correct_ends() {
    assert_eq!(elide("/short/path", 32), "/short/path");
    assert_eq!(elide("/a/long/path", 0), "");
    assert_eq!(elide("/a/long/path", -1), "");
    assert_eq!(elide("/a/long/path", 1), "…");
    assert_eq!(elide("/a/long/path", 2), "…h");
    assert_eq!(elide("/a/long/path", 3), "/…h");
    assert_eq!(elide("/a/long/path", 9), "/a…g/path");
}

#[test]
fn unicode_paths_preserve_characters_and_give_the_tail_more_room() {
    let path = "C:\\研究\\共享数据\\最新结果\\📁目录";
    for budget in 3..path.chars().count() as i32 {
        let displayed = elide(path, budget);
        let (head, tail) = displayed.split_once('…').expect("middle ellipsis");
        assert!(path.starts_with(head));
        assert!(path.ends_with(tail));
        assert!(!head.is_empty());
        assert!(!tail.is_empty());
        assert!(tail.chars().count() >= head.chars().count());
        assert_eq!(displayed.chars().count(), budget as usize);
    }
}

#[test]
fn oversized_paths_have_a_fixed_display_budget_without_losing_the_tail() {
    let path = format!("/root/{}final-directory", "研究/".repeat(5000));
    let displayed = elide(&path, i32::MAX);
    assert_eq!(displayed.chars().count(), MAX_DISPLAY_CHARS);
    assert!(displayed.starts_with("/root/"));
    assert!(displayed.ends_with("final-directory"));
}
