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

const BASE: Rect = Rect {
    left: 100,
    top: 50,
    right: 505,
    bottom: 770,
};

fn size(r: Rect) -> (i32, i32) {
    (r.right - r.left, r.bottom - r.top)
}

#[test]
fn dragging_right_edge_follows_width_and_grows_downward() {
    let proposed = Rect { right: 708, ..BASE };
    let r = lock_aspect(proposed, Edge::Right, vertical());
    assert_eq!(size(r), (608, 1081));
    assert_eq!((r.left, r.top), (BASE.left, BASE.top));
}

#[test]
fn dragging_bottom_edge_follows_height_and_grows_rightward() {
    let proposed = Rect {
        bottom: 50 + 1080,
        ..BASE
    };
    let r = lock_aspect(proposed, Edge::Bottom, vertical());
    assert_eq!(size(r), (608, 1081));
    assert_eq!((r.left, r.top), (BASE.left, BASE.top));
}

#[test]
fn dragging_left_or_top_keeps_opposite_edge_anchored() {
    let proposed = Rect { left: 0, ..BASE };
    let r = lock_aspect(proposed, Edge::Left, vertical());
    assert_eq!(r.right, BASE.right);
    assert_eq!(r.top, BASE.top);
    assert_eq!(size(r), (505, 898));

    let proposed = Rect { top: -310, ..BASE };
    let r = lock_aspect(proposed, Edge::Top, vertical());
    assert_eq!(r.bottom, BASE.bottom);
    assert_eq!(r.left, BASE.left);
}

#[test]
fn corner_takes_the_larger_of_both_and_anchors_opposite_corner() {
    // 宽只拉了 50，高拉了 360：以高为准
    let proposed = Rect {
        left: 50,
        top: -310,
        ..BASE
    };
    let r = lock_aspect(proposed, Edge::TopLeft, vertical());
    assert_eq!((r.right, r.bottom), (BASE.right, BASE.bottom));
    assert_eq!(size(r), (608, 1081));
}

#[test]
fn clamps_to_design_size_and_work_area() {
    let shrink = Rect { right: 300, ..BASE };
    assert_eq!(
        size(lock_aspect(shrink, Edge::Right, vertical())),
        (405, 720)
    );

    let huge = Rect {
        right: 5000,
        ..BASE
    };
    assert_eq!(
        size(lock_aspect(huge, Edge::Right, vertical())),
        (1080, 1920)
    );
}

#[test]
fn frame_is_added_outside_the_locked_client_area() {
    let limits = AspectLimits {
        frame: (16, 39),
        ..vertical()
    };
    let proposed = Rect {
        left: 0,
        top: 0,
        right: 810 + 16,
        bottom: 10,
    };
    let r = lock_aspect(proposed, Edge::Right, limits);
    assert_eq!(size(r), (810 + 16, 1440 + 39));
}

#[test]
fn wmsz_values_map_to_edges() {
    assert_eq!(Edge::from_wmsz(1), Some(Edge::Left));
    assert_eq!(Edge::from_wmsz(8), Some(Edge::BottomRight));
    assert_eq!(Edge::from_wmsz(0), None);
    assert_eq!(Edge::from_wmsz(9), None);
}
