use super::*;

/// 竖版 405×720 在 100% DPI、无边框、工作区够大时的约束。
fn vertical() -> AspectLimits {
    AspectLimits {
        ratio: 405.0 / 720.0,
        frame: (0, 0),
        min_width: 405,
        max_width: 1080,
    }
}

const WORK: Rect = Rect {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 2000,
};

/// 中心在 (960, 1000) 的设计尺寸窗口，离工作区四边都远。
const BASE: Rect = Rect {
    left: 758,
    top: 640,
    right: 1163,
    bottom: 1360,
};

fn anchor(window: Rect) -> DragAnchor {
    DragAnchor::new(window, WORK)
}

fn size(r: Rect) -> (i32, i32) {
    (r.right - r.left, r.bottom - r.top)
}

/// 中心不动：左右边之和、上下边之和与锚点窗口相差不超过 1（奇数宽高的中心落在半像素上）。
fn assert_centered(r: Rect, around: Rect) {
    let dx = (r.left + r.right) - (around.left + around.right);
    let dy = (r.top + r.bottom) - (around.top + around.bottom);
    assert!(
        dx.abs() <= 1 && dy.abs() <= 1,
        "{r:?} 偏离 {around:?} 的中心"
    );
}

#[test]
fn dragging_right_edge_grows_both_sides_around_center() {
    // BASE 中心横坐标 960.5，右边拉到 1264：半宽 303.5
    let proposed = Rect {
        right: 1264,
        ..BASE
    };
    let r = resize_centered(proposed, Edge::Right, anchor(BASE), vertical());
    assert_eq!(size(r), (607, 1079));
    assert!(r.left < BASE.left);
    assert_centered(r, BASE);
}

#[test]
fn dragging_top_edge_follows_height_around_center() {
    // 上边拉到中心上方 540：高 1080，按比例宽 607.5 取整 608
    let proposed = Rect { top: 460, ..BASE };
    let r = resize_centered(proposed, Edge::Top, anchor(BASE), vertical());
    assert_eq!(size(r), (608, 1081));
    assert!(r.bottom > BASE.bottom);
    assert_centered(r, BASE);
}

#[test]
fn left_and_right_are_symmetric() {
    let a = anchor(BASE);
    let right = resize_centered(
        Rect {
            right: 1300,
            ..BASE
        },
        Edge::Right,
        a,
        vertical(),
    );
    let left = resize_centered(Rect { left: 621, ..BASE }, Edge::Left, a, vertical());
    assert_eq!(right, left);
}

#[test]
fn corner_takes_the_larger_of_both() {
    // 横向只拉了 50（宽 505），纵向拉了 360（高 1440 → 宽 810）：以高为准
    let proposed = Rect {
        right: BASE.right + 50,
        bottom: BASE.bottom + 360,
        ..BASE
    };
    let r = resize_centered(proposed, Edge::BottomRight, anchor(BASE), vertical());
    assert_eq!(size(r), (810, 1440));
    assert_centered(r, BASE);
}

#[test]
fn clamps_to_design_size_and_max_size() {
    let a = anchor(BASE);
    let shrink = Rect {
        right: 1000,
        ..BASE
    };
    assert_eq!(
        size(resize_centered(shrink, Edge::Right, a, vertical())),
        (405, 720)
    );

    let huge = Rect {
        right: 5000,
        ..BASE
    };
    assert_eq!(
        size(resize_centered(huge, Edge::Right, a, vertical())),
        (1080, 1920)
    );
}

#[test]
fn pushed_back_inside_work_area_near_edges() {
    // 贴左上角的窗口向右下放大：中心让位，左上贴住工作区
    let corner = Rect {
        left: 0,
        top: 0,
        right: 405,
        bottom: 720,
    };
    let proposed = Rect {
        right: 810,
        bottom: 1440,
        ..corner
    };
    let r = resize_centered(proposed, Edge::BottomRight, anchor(corner), vertical());
    assert_eq!((r.left, r.top), (0, 0));
    assert!(r.right - r.left > 405);

    // 贴右下角的窗口放到最大：仍完整留在工作区里
    let corner = Rect {
        left: 1515,
        top: 1280,
        right: 1920,
        bottom: 2000,
    };
    let proposed = Rect {
        left: -5000,
        ..corner
    };
    let r = resize_centered(proposed, Edge::Left, anchor(corner), vertical());
    assert_eq!(size(r), (1080, 1920));
    assert!(r.left >= WORK.left && r.top >= WORK.top);
    assert!(r.right <= WORK.right && r.bottom <= WORK.bottom);
}

#[test]
fn oversized_window_keeps_top_left_visible() {
    // 最小尺寸都放不进的小工作区：露出左上角的标题栏
    let small = Rect {
        left: 100,
        top: 100,
        right: 400,
        bottom: 500,
    };
    let a = DragAnchor::new(BASE, small);
    let r = resize_centered(BASE, Edge::Right, a, vertical());
    assert_eq!((r.left, r.top), (small.left, small.top));
}

#[test]
fn frame_is_added_outside_the_locked_client_area() {
    let limits = AspectLimits {
        frame: (16, 39),
        ..vertical()
    };
    // 中心横坐标 750；右边拉到中心右侧 413，外框宽 826，减去边框客户区宽 810
    let window = Rect {
        left: 500,
        top: 200,
        right: 1000,
        bottom: 1000,
    };
    let proposed = Rect {
        right: 750 + 413,
        ..window
    };
    let r = resize_centered(proposed, Edge::Right, anchor(window), limits);
    assert_eq!(size(r), (810 + 16, 1440 + 39));
}

#[test]
fn wmsz_values_map_to_edges() {
    assert_eq!(Edge::from_wmsz(1), Some(Edge::Left));
    assert_eq!(Edge::from_wmsz(8), Some(Edge::BottomRight));
    assert_eq!(Edge::from_wmsz(0), None);
    assert_eq!(Edge::from_wmsz(9), None);
}

#[test]
fn snapped_half_screen_fits_largest_aspect_rect_centered() {
    // 竖版贴到 1920×1040 屏幕的左半边：高度顶满，宽按比例收窄并在左半边居中
    let snapped = Rect {
        left: 0,
        top: 0,
        right: 960,
        bottom: 1040,
    };
    let fitted = fit_within(snapped, vertical()).expect("比例不对，应当改写");
    let (w, h) = size(fitted);
    assert_eq!(h, 1040);
    assert!((f64::from(w) / f64::from(h) - 405.0 / 720.0).abs() < 0.002);
    assert_centered(fitted, snapped);
}

#[test]
fn aspect_correct_sizes_and_minimized_pass_through() {
    // 面板自己 setSize 的等比尺寸差一两像素舍入，不改
    assert_eq!(fit_within(BASE, vertical()), None);
    let rounded = Rect {
        bottom: BASE.bottom + 1,
        ..BASE
    };
    assert_eq!(fit_within(rounded, vertical()), None);
    // 最小化时系统给的矩形比设计尺寸小，不碰
    let minimized = Rect {
        left: -32000,
        top: -32000,
        right: -31840,
        bottom: -31972,
    };
    assert_eq!(fit_within(minimized, vertical()), None);
}

#[test]
fn snapped_rect_respects_max_width_and_frame() {
    let limits = AspectLimits {
        frame: (16, 39),
        max_width: 500,
        ..vertical()
    };
    let snapped = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 2000,
    };
    let fitted = fit_within(snapped, limits).expect("比例不对，应当改写");
    let (w, h) = size(fitted);
    assert_eq!(w, 500 + 16);
    assert_eq!(h, (500.0_f64 / (405.0 / 720.0)).round() as i32 + 39);
    assert_centered(fitted, snapped);
}
