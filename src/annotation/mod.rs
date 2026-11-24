mod drawer;

use crate::annotation::drawer::State as Drawer;
use eframe::{Frame, NativeOptions};
use egui::{
    CentralPanel, Panel, PointerButton, Rect, Response, ScrollArea, Sense, Ui, Vec2,
    ViewportBuilder,
};
use std::{fs, ops::Range};

pub(super) fn run_gui() {
    let options = NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..NativeOptions::default()
    };
    eframe::run_native(
        "Gazer",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(State::new()))
        }),
    )
    .unwrap();
}

struct State {
    images: Images,
    scene: Rect,
    drawer: Drawer,
}

struct Images {
    uris: Vec<String>,
    resolved: Range<usize>,
    selected: Option<usize>,
}

impl State {
    fn new() -> Self {
        Self {
            images: Images::new(images_uris()),
            scene: Rect::ZERO,
            drawer: Drawer::default(),
        }
    }
}

impl Images {
    fn new(uris: Vec<String>) -> Self {
        Self {
            uris,
            resolved: Range::default(),
            selected: None,
        }
    }

    fn to_forget(&self, range: &Range<usize>) -> impl Iterator<Item = &String> {
        self.indexes_to_forget(range).map(|index| &self.uris[index])
    }

    fn indexes_to_forget(&self, range: &Range<usize>) -> impl Iterator<Item = usize> {
        let resolved = &self.resolved;
        let before = resolved.start.min(range.start)..range.start;
        let after = range.end.min(resolved.end)..resolved.end;
        before.chain(after).filter(|x| Some(*x) != self.selected)
    }

    fn len(&self) -> usize {
        self.uris.len()
    }

    fn selected_uri(&self) -> Option<&str> {
        self.selected.map(|index| self.uris[index].as_str())
    }
}

fn images_uris() -> Vec<String> {
    let directory = std::env::current_dir().unwrap();
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| format!("file://{}", path.display()))
        .collect()
}

impl eframe::App for State {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        images(ui, &mut self.images);
        center(self, ui);
    }
}

fn images(ui: &mut Ui, images: &mut Images) {
    Panel::left("Images panel").show(ui, |ui| {
        const ROW_HEIGHT: f32 = 200.0;
        ScrollArea::vertical().show_rows(ui, ROW_HEIGHT, images.len(), |ui, range| {
            forget(ui, images.to_forget(&range));
            images.resolved.clone_from(&range);
            for index in range.clone() {
                let uri = &images.uris[index];
                let image = egui::Image::new(uri);
                let button = egui::Button::new(image).selected(Some(index) == images.selected);
                if ui
                    .add_sized([ui.available_width(), ROW_HEIGHT], button)
                    .clicked()
                {
                    forget_selected(images, ui);
                    images.selected = Some(index);
                }
            }
        })
    });
}

fn forget_selected(images: &mut Images, ui: &mut Ui) {
    if let Some(selected_image) = images.selected
        && !images.resolved.contains(&selected_image)
    {
        let uri = &images.uris[selected_image];
        ui.forget_image(uri);
    }
}

fn forget<'a>(ui: &mut Ui, images: impl Iterator<Item = &'a String>) {
    for uri in images {
        ui.forget_image(uri);
    }
}

fn center(state: &mut State, ui: &mut Ui) {
    let Some(uri) = state.images.selected_uri() else {
        return;
    };
    CentralPanel::no_frame().show(ui, |ui| {
        let mut pan_delta = Vec2::ZERO;
        egui::Scene::new()
            .scroll_zooms(true)
            .zoom_range(1.0..=f32::INFINITY)
            .show(ui, &mut state.scene, |ui| {
                let response = ui.image(uri).interact(Sense::DRAG);
                drawer::update(&mut state.drawer, &response, ui);
                pan(&response, &mut pan_delta);
            });
        state.scene = state.scene.translate(pan_delta);
    });
}

fn pan(response: &Response, delta: &mut Vec2) {
    if response.dragged_by(PointerButton::Middle) {
        *delta = -response.drag_delta();
    }
}
