//! Window geometry in the units the macOS display APIs use: global points with the origin at the
//! bottom-left of the main display. A screenshot of one display has its origin at the top-left of
//! that display and is measured in pixels (points times the backing scale).

/// A rectangle in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One display: its frame in global points and its backing scale factor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub frame: Rect,
    pub scale: f64,
}

/// A rectangle in the pixels of one display's screenshot (origin top-left).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PxRect {
    pub left: i64,
    pub top: i64,
    pub width: i64,
    pub height: i64,
}

/// The panel: `w` by `h` points, centred on the display, `bottom_margin` points above its bottom edge.
pub fn panel_rect(frame: Rect, w: f64, h: f64, bottom_margin: f64) -> Rect {
    Rect {
        x: frame.x + (frame.w - w) / 2.0,
        y: frame.y + bottom_margin,
        w,
        h,
    }
}

/// The pixel rectangle of the panel inside a screenshot of the display (SPEC AC-27):
/// left=(x-fx)*scale, top=(fy+fh-(y+h))*scale, width=w*scale, height=h*scale, each rounded to the
/// nearest pixel.
pub fn px_rect(panel: Rect, frame: Rect, scale: f64) -> PxRect {
    PxRect {
        left: ((panel.x - frame.x) * scale).round() as i64,
        top: ((frame.y + frame.h - (panel.y + panel.h)) * scale).round() as i64,
        width: (panel.w * scale).round() as i64,
        height: (panel.h * scale).round() as i64,
    }
}

/// The centre of the panel in page points of a full-screen page on the same display (origin
/// top-left of the display).
pub fn page_point(panel: Rect, frame: Rect) -> (f64, f64) {
    (
        panel.x + panel.w / 2.0 - frame.x,
        frame.y + frame.h - (panel.y + panel.h / 2.0),
    )
}

/// The display with the 1-based number `n` (1 is the main display), if there is one.
pub fn pick_display(displays: &[Display], n: usize) -> Option<Display> {
    n.checked_sub(1).and_then(|i| displays.get(i)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1920.0,
        h: 1080.0,
    };

    #[test]
    fn panel_rect_is_centred_and_above_the_bottom_edge() {
        let p = panel_rect(MAIN, 1200.0, 120.0, 120.0);
        assert_eq!(
            p,
            Rect {
                x: 360.0,
                y: 120.0,
                w: 1200.0,
                h: 120.0
            }
        );
    }

    #[test]
    fn panel_rect_follows_a_frame_with_an_offset() {
        let frame = Rect {
            x: 1920.0,
            y: -200.0,
            w: 2560.0,
            h: 1440.0,
        };
        let p = panel_rect(frame, 1200.0, 120.0, 120.0);
        assert_eq!(
            p,
            Rect {
                x: 1920.0 + 680.0,
                y: -80.0,
                w: 1200.0,
                h: 120.0
            }
        );
    }

    #[test]
    fn px_rect_on_the_main_display_at_scale_two() {
        let p = Rect {
            x: 360.0,
            y: 120.0,
            w: 1200.0,
            h: 120.0,
        };
        let r = px_rect(p, MAIN, 2.0);
        assert_eq!(
            r,
            PxRect {
                left: 720,
                top: 1680,
                width: 2400,
                height: 240
            }
        );
    }

    #[test]
    fn px_rect_on_a_second_display_to_the_right_at_scale_one() {
        let frame = Rect {
            x: 1920.0,
            y: 0.0,
            w: 2560.0,
            h: 1440.0,
        };
        let p = panel_rect(frame, 1200.0, 120.0, 120.0);
        let r = px_rect(p, frame, 1.0);
        assert_eq!(
            r,
            PxRect {
                left: 680,
                top: 1200,
                width: 1200,
                height: 120
            }
        );
    }

    #[test]
    fn px_rect_on_a_display_with_a_negative_origin() {
        let frame = Rect {
            x: -1920.0,
            y: 200.0,
            w: 1920.0,
            h: 1080.0,
        };
        let p = Rect {
            x: -1500.0,
            y: 300.0,
            w: 400.0,
            h: 100.0,
        };
        let r = px_rect(p, frame, 2.0);
        assert_eq!(
            r,
            PxRect {
                left: 840,
                top: 1760,
                width: 800,
                height: 200
            }
        );
    }

    #[test]
    fn px_rect_rounds_to_the_nearest_pixel() {
        let p = Rect {
            x: 0.25,
            y: 0.25,
            w: 10.25,
            h: 10.25,
        };
        let frame = Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 100.0,
        };
        let r = px_rect(p, frame, 1.0);
        assert_eq!(
            r,
            PxRect {
                left: 0,
                top: 90,
                width: 10,
                height: 10
            }
        );
    }

    #[test]
    fn page_point_is_the_panel_centre_with_a_top_left_origin() {
        let p = Rect {
            x: 360.0,
            y: 120.0,
            w: 1200.0,
            h: 120.0,
        };
        assert_eq!(page_point(p, MAIN), (960.0, 900.0));
    }

    #[test]
    fn pick_display_numbers_from_one_and_refuses_zero_and_too_large() {
        let a = Display {
            frame: MAIN,
            scale: 2.0,
        };
        let b = Display {
            frame: Rect {
                x: 1920.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            scale: 1.0,
        };
        let all = [a, b];
        assert_eq!(pick_display(&all, 1), Some(a));
        assert_eq!(pick_display(&all, 2), Some(b));
        assert_eq!(pick_display(&all, 0), None);
        assert_eq!(pick_display(&all, 3), None);
    }
}
