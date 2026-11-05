use super::*;
use crate::Theme;
use lopdf::{Document, Object, dictionary};

fn pdf(name: &str) -> PdfDocument {
    let mut doc = Document::with_version("1.7");
    let pages = doc.new_object_id();
    let field = doc.add_object(dictionary! {
        "Type"=>"Annot", "Subtype"=>"Widget", "FT"=>"Tx",
        "T"=>Object::string_literal(name), "V"=>Object::string_literal("Initial"),
        "Rect"=>vec![30.into(),30.into(),250.into(),60.into()], "Ff"=>0
    });
    let page = doc.add_object(dictionary! {
        "Type"=>"Page", "Parent"=>pages,
        "MediaBox"=>vec![0.into(),0.into(),1200.into(),1600.into()],
        "Resources"=>dictionary!{}, "Annots"=>vec![Object::Reference(field)]
    });
    doc.objects.insert(
        pages,
        dictionary! {"Type"=>"Pages", "Kids"=>vec![Object::Reference(page)], "Count"=>1}.into(),
    );
    let catalog = doc.add_object(dictionary!{"Type"=>"Catalog", "Pages"=>pages, "AcroForm"=>dictionary!{"Fields"=>vec![Object::Reference(field)]}});
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    PdfDocument::open(&bytes).unwrap()
}

fn fixture() -> (Controls, PageViewer) {
    let mut ui = Controls::new(Theme::FluentLight).unwrap();
    let root = ui.root();
    let mut viewer = PageViewer::pdf(
        &mut ui,
        root,
        pdf("Customer"),
        LayoutStyle {
            width: Length::Percent(1.0),
            height: Length::Percent(1.0),
            ..Default::default()
        },
    )
    .unwrap();
    ui.update(Size::new(1000.0, 650.0)).unwrap();
    viewer.refresh(&mut ui).unwrap();
    (ui, viewer)
}

fn wheel(ui: &Controls, viewer: &PageViewer, delta: Point, modifiers: Modifiers) -> InputEvent {
    let b = ui.runtime.bounds(viewer.parts.canvas).unwrap();
    InputEvent::Wheel {
        position: Point::new(b.x + 60.0, b.y + 60.0),
        delta,
        modifiers,
    }
}

#[test]
fn wheel_offsets_follow_the_shared_viewport_direction() {
    let (mut ui, mut viewer) = fixture();
    viewer.zoom = Some(1.0);
    for (delta, shift, expected) in [
        (Point::new(40.0, 84.0), false, Point::new(40.0, 84.0)),
        (Point::new(0.0, 84.0), true, Point::new(124.0, 84.0)),
        (Point::new(-24.0, -28.0), false, Point::new(100.0, 56.0)),
    ] {
        let event = wheel(
            &ui,
            &viewer,
            delta,
            Modifiers {
                shift,
                ..Default::default()
            },
        );
        assert!(viewer.input(&mut ui, &event).unwrap());
        viewer.refresh(&mut ui).unwrap();
        assert_eq!(viewer.offset, expected);
    }
    ui.set_enabled(viewer.parts.canvas, false).unwrap();
    let event = wheel(&ui, &viewer, Point::new(0.0, 84.0), Modifiers::default());
    assert!(!viewer.input(&mut ui, &event).unwrap());
    assert_eq!(viewer.offset, Point::new(100.0, 56.0));
}

#[test]
fn zoom_starts_from_fit_and_zero_or_horizontal_ctrl_wheel_is_inert() {
    let (mut ui, mut viewer) = fixture();
    let b = ui.runtime.bounds(viewer.parts.canvas).unwrap();
    let fit = viewer.effective_zoom(b);
    assert!(fit < 0.5);
    let modifiers = Modifiers {
        control: true,
        ..Default::default()
    };
    for delta in [
        Point::ZERO,
        Point::new(84.0, 0.0),
        Point::new(0.0, f32::NAN),
    ] {
        let event = wheel(&ui, &viewer, delta, modifiers);
        assert!(!viewer.input(&mut ui, &event).unwrap());
        assert_eq!(viewer.zoom, None);
    }
    let event = wheel(&ui, &viewer, Point::new(0.0, -84.0), modifiers);
    viewer.input(&mut ui, &event).unwrap();
    assert!((viewer.effective_zoom(b) - fit * 1.1).abs() < 1e-6);
    let event = wheel(&ui, &viewer, Point::new(0.0, 84.0), modifiers);
    viewer.input(&mut ui, &event).unwrap();
    assert!((viewer.effective_zoom(b) - fit).abs() < 1e-6);
    viewer.zoom = None;
    let button = viewer
        .actions
        .iter()
        .find(|(_, a)| *a == "zoom-in")
        .unwrap()
        .0;
    viewer.activate(&mut ui, button).unwrap();
    assert!((viewer.effective_zoom(b) - fit * 1.25).abs() < 1e-6);
}

#[test]
fn field_escape_restores_document_value_after_editor_acceptance_and_blur() {
    let (mut ui, mut viewer) = fixture();
    let field = viewer.parts.field_value;
    assert_eq!(
        ui.semantics()
            .into_iter()
            .find(|n| n.id == field)
            .unwrap()
            .label,
        "PDF field Customer"
    );
    // Generic editor acceptance is independent of applying a PDF field.
    ui.set_editor_value(field, "Unapplied after blur").unwrap();
    ui.runtime
        .apply(Mutation::Focus(Some(viewer.parts.canvas)))
        .unwrap();
    assert!(viewer.guard_field(&ui).is_err());
    ui.runtime.apply(Mutation::Focus(Some(field))).unwrap();
    assert!(
        viewer
            .input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Modifiers::default(),
                    repeat: false
                }
            )
            .unwrap()
    );
    assert_eq!(ui.document(field).unwrap().text(), "Initial");
    assert_eq!(viewer.pdf_document().unwrap().fields()[0].value, "Initial");
    assert!(viewer.guard_field(&ui).is_ok());
    let token = viewer.begin_load().unwrap();
    assert!(
        viewer
            .complete_load(&mut ui, token, pdf("Replacement"))
            .unwrap()
    );
    assert_eq!(viewer.parts.field_value, field);
    assert_eq!(
        ui.semantics()
            .into_iter()
            .find(|n| n.id == field)
            .unwrap()
            .label,
        "PDF field Replacement"
    );
}

#[test]
fn field_composition_is_not_replaced_by_document_cancellation() {
    let (mut ui, mut viewer) = fixture();
    let field = viewer.parts.field_value;
    ui.runtime.apply(Mutation::Focus(Some(field))).unwrap();
    ui.ime_preedit("に", Some((0, 0))).unwrap();
    assert!(viewer.cancel_field_draft(&mut ui).is_err());
    assert!(
        !viewer
            .input(
                &mut ui,
                &InputEvent::KeyDown {
                    key: Key::Escape,
                    modifiers: Modifiers::default(),
                    repeat: false
                }
            )
            .unwrap()
    );
    assert!(ui.document(field).unwrap().composition().is_some());
    assert_eq!(viewer.pdf_document().unwrap().fields()[0].value, "Initial");
}
