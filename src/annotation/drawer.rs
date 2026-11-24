use egui::{Color32, CursorIcon, PointerButton, Pos2, Rect, Response, Stroke, StrokeKind, Ui};
use itertools::Itertools;

#[derive(Default)]
pub(super) struct State {
    draw_start: Pos2,
    boundings: Vec<Rect>,
    drag: Option<Drag>,
}

struct Drag {
    coords: [f32; 4],
    movables: [bool; 4],
    index: usize,
}

pub(super) fn update(drawer: &mut State, response: &Response, ui: &Ui) {
    draw(ui, response, drawer);
    drag(ui, response, drawer);
    display_boxes(ui, &drawer.boundings);
}

fn drag(ui: &Ui, response: &Response, drawer: &mut State) {
    start_drag(ui, response, drawer);
    update_drag(response, drawer);
    end_drag(response, &mut drawer.drag);
}

fn draw(ui: &Ui, response: &Response, drawer: &mut State) {
    start_draw(response, &mut drawer.draw_start);
    update_draw(ui, response, drawer.draw_start);
    end_draw(response, drawer);
}

fn end_drag(response: &Response, drag: &mut Option<Drag>) {
    if response.drag_stopped_by(PointerButton::Primary) {
        *drag = None;
    }
}

fn update_drag(response: &Response, drawer: &mut State) {
    if response.dragged_by(PointerButton::Primary)
        && let Some(drag) = &mut drawer.drag
    {
        let delta = response.drag_delta();
        let coords = drag
            .movables
            .iter()
            .copied()
            .zip(drag.coords)
            .zip([delta.x, delta.y, delta.x, delta.y])
            .map(|((movable, mut coord), delta)| {
                if movable {
                    coord += delta;
                }
                coord
            })
            .collect_array::<4>()
            .unwrap();
        drag.coords.clone_from(&coords);
        let [x1, y1, x2, y2] = coords;
        drawer.boundings[drag.index] = Rect::from_two_pos(Pos2::new(x1, y1), Pos2::new(x2, y2));
    }
}

fn start_drag(ui: &Ui, response: &Response, drawer: &mut State) {
    let Some(pos) = response.hover_pos() else {
        return;
    };
    if let Some((index, rect)) = drawer
        .boundings
        .iter()
        .enumerate()
        .filter(|(_, rect)| contains(**rect, pos))
        .min_by(|(_, a), (_, b)| a.area().partial_cmp(&b.area()).unwrap())
    {
        let horizontal = Strip::from_rect_pos(rect.left(), rect.right(), pos.x);
        let vertical = Strip::from_rect_pos(rect.top(), rect.bottom(), pos.y);
        ui.set_cursor_icon(Strip::cursor([horizontal, vertical]));
        if response.drag_started_by(PointerButton::Primary) {
            drawer.drag = Some(Drag {
                index,
                movables: Strip::movables(horizontal, vertical),
                coords: [rect.left(), rect.top(), rect.right(), rect.bottom()],
            });
        }
    }
}

fn end_draw(response: &Response, drawer: &mut State) {
    if response.drag_stopped_by(PointerButton::Secondary) {
        let rect = Rect::from_two_pos(drawer.draw_start, response.hover_pos().unwrap());
        if rect.area() > 50.0 {
            drawer.boundings.push(rect);
        }
    }
}

fn update_draw(ui: &Ui, response: &Response, draw_start: Pos2) {
    if !response.dragged_by(PointerButton::Secondary) {
        return;
    }
    let painter = ui.painter();
    let rect = Rect::from_two_pos(draw_start, response.hover_pos().unwrap());
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_rgb(255, 10, 10)),
        StrokeKind::Middle,
    );
}

fn start_draw(response: &Response, pos: &mut Pos2) {
    if response.drag_started_by(PointerButton::Secondary) {
        *pos = response.hover_pos().unwrap();
    }
}

fn display_boxes(ui: &Ui, boxes: &[Rect]) {
    let painter = ui.painter();
    for rect in boxes.iter().copied() {
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(255, 10, 10)),
            StrokeKind::Middle,
        );
    }
}

fn contains(rect: Rect, pos: Pos2) -> bool {
    const TOLERANCE: f32 = 4.0;
    pos.x >= rect.left() - TOLERANCE
        && pos.x <= rect.right() + TOLERANCE
        && pos.y >= rect.top() - TOLERANCE
        && pos.y <= rect.bottom() + TOLERANCE
}

#[derive(Clone, Copy)]
enum Strip {
    Low,
    Middle,
    High,
}

impl Strip {
    fn from_rect_pos(low: f32, high: f32, pos: f32) -> Self {
        const TOLERANCE: f32 = 4.0;
        if pos < low + TOLERANCE {
            Self::Low
        } else if pos > high - TOLERANCE {
            Self::High
        } else {
            Self::Middle
        }
    }

    fn cursor(strips: [Strip; 2]) -> CursorIcon {
        match strips {
            [Strip::Low, Strip::Low] => CursorIcon::ResizeNorthWest,
            [Strip::Low, Strip::Middle] => CursorIcon::ResizeWest,
            [Strip::Low, Strip::High] => CursorIcon::ResizeSouthWest,
            [Strip::Middle, Strip::Low] => CursorIcon::ResizeNorth,
            [Strip::Middle, Strip::Middle] => CursorIcon::Grab,
            [Strip::Middle, Strip::High] => CursorIcon::ResizeSouth,
            [Strip::High, Strip::Low] => CursorIcon::ResizeNorthEast,
            [Strip::High, Strip::Middle] => CursorIcon::ResizeEast,
            [Strip::High, Strip::High] => CursorIcon::ResizeSouthEast,
        }
    }

    fn movables(horizontal: Self, vertical: Self) -> [bool; 4] {
        let [left, right] = Self::axis_movables(horizontal, vertical);
        let [top, bottom] = Self::axis_movables(vertical, horizontal);
        [left, top, right, bottom]
    }

    fn axis_movables(self, other: Self) -> [bool; 2] {
        match self {
            Strip::Low => [true, false],
            Strip::Middle if matches!(other, Self::Middle) => [true, true],
            Strip::Middle => [false, false],
            Strip::High => [false, true],
        }
    }
}
