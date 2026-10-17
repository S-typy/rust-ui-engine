use super::*;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
fn image() -> ClipboardImage {
    ClipboardImage {
        width: 2,
        height: 2,
        rgba: vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 12, 34, 56, 78],
    }
}
fn message(n: u8) -> PlatformServiceRequest {
    PlatformServiceRequest::Dispatch {
        channel: 7,
        payload: vec![n],
    }
}

#[test]
fn html_offsets_are_utf8_bytes_and_body_cannot_shadow_headers() {
    for text in [
        "",
        "Привет 🦀 é",
        "<b>Data</b>\nStartHTML:999\nEndFragment:0",
        "<!--EndFragment-->text",
    ] {
        let encoded = codec::html_encode(text);
        assert_eq!(codec::html_decode(&encoded).unwrap(), text);
    }
    for ending in ["\r", "\n"] {
        let text = String::from_utf8(codec::html_encode("fragment")).unwrap();
        let start = text.find("<html>").unwrap();
        let lines = text[..start].matches("\r\n").count();
        let mut header = text[..start].replace("\r\n", ending);
        for key in ["StartHTML:", "EndHTML:", "StartFragment:", "EndFragment:"] {
            let at = header.find(key).unwrap() + key.len();
            let value: usize = header[at..at + 10].parse().unwrap();
            header.replace_range(at..at + 10, &format!("{:010}", value - lines));
        }
        assert_eq!(
            codec::html_decode(format!("{header}{}", &text[start..]).as_bytes()).unwrap(),
            "fragment"
        );
    }
    let encoded = codec::html_encode(&"é".repeat(2048));
    assert_eq!(codec::html_decode(&encoded).unwrap(), "é".repeat(2048));
}
#[test]
fn html_rejects_truncation_duplicates_overflow_and_nonboundary_offsets() {
    let good = codec::html_encode("🦀");
    for length in 0..good.len() - 1 {
        assert!(
            codec::html_decode(&good[..length]).is_err(),
            "length {length}"
        );
    }
    let corrupt = |key: &str, value: &str| {
        let mut text = String::from_utf8(good.clone()).unwrap();
        let at = text.find(key).unwrap() + key.len();
        text.replace_range(at..at + 10, value);
        text.into_bytes()
    };
    assert!(codec::html_decode(&corrupt("EndFragment:", "9999999999")).is_err());
    let mut offset = String::from_utf8(good.clone()).unwrap();
    let at = offset.find("StartFragment:").unwrap() + 14;
    let n: usize = offset[at..at + 10].parse().unwrap();
    offset.replace_range(at..at + 10, &format!("{:010}", n + 1));
    assert!(codec::html_decode(offset.as_bytes()).is_err());
    assert!(codec::html_decode(b"StartHTML:0000000050\nStartHTML:0000000050\n").is_err());
}
#[test]
fn dib_roundtrip_preserves_rgba_orientation_and_rejects_other_recipes() {
    let input = image();
    let good = codec::dib_encode(&input);
    assert_eq!(codec::dib_decode(&good).unwrap(), input);
    let mut bottom = good.clone();
    bottom[8..12].copy_from_slice(&2u32.to_le_bytes());
    bottom[124..132].copy_from_slice(&good[132..140]);
    bottom[132..140].copy_from_slice(&good[124..132]);
    assert_eq!(codec::dib_decode(&bottom).unwrap(), input);
    for (at, value) in [
        (0, 40u32),
        (4, u32::MAX),
        (8, i32::MIN as u32),
        (16, 0),
        (20, 99),
        (32, 1),
        (40, 0),
        (56, 0),
        (112, 124),
        (116, 10),
    ] {
        let mut bad = good.clone();
        bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert!(codec::dib_decode(&bad).is_err(), "offset {at}");
    }
    assert!(codec::dib_decode(&good[..good.len() - 1]).is_err());
}
#[test]
fn requests_validate_before_queue_mutation_and_redact_payloads() {
    let q = queue::ServiceQueue::new(AppWaker::testing(|| true));
    let h = q.handle();
    let private = "private clipboard fixture";
    let request = PlatformServiceRequest::WriteClipboard(ClipboardContent {
        text: Some(private.into()),
        html: None,
        image: None,
    });
    assert!(!format!("{request:?}").contains(private));
    let id = h.submit(request).unwrap();
    for request in [
        PlatformServiceRequest::WriteClipboard(ClipboardContent::default()),
        PlatformServiceRequest::Dispatch {
            channel: 0,
            payload: vec![0; 65_537],
        },
        PlatformServiceRequest::SetTaskbarProgress(TaskbarProgress::Normal {
            completed: 2,
            total: 1,
        }),
        PlatformServiceRequest::PublishJumpList(JumpList {
            app_id: "bad id".into(),
            tasks: vec![],
        }),
    ] {
        assert_eq!(
            h.submit(request).unwrap_err().kind,
            ServiceErrorKind::Invalid
        );
    }
    assert_eq!(q.pop().unwrap().0, id);
    assert!(q.pop().is_none());
}
#[test]
fn queued_cancel_is_owner_scoped_and_drop_revokes_handles() {
    let q = queue::ServiceQueue::new(AppWaker::testing(|| true));
    let h = q.handle();
    let other = queue::ServiceQueue::new(AppWaker::testing(|| true));
    let a = h.submit(message(1)).unwrap();
    let foreign = other.handle().submit(message(2)).unwrap();
    assert!(!h.cancel(&foreign));
    assert!(h.cancel(&a));
    assert!(!h.cancel(&a));
    assert!(q.pop().is_none());
    let b = h.submit(message(3)).unwrap();
    assert_eq!(q.pop().unwrap().0, b);
    assert!(!h.cancel(&b));
    h.submit(message(4)).unwrap();
    drop(q);
    assert_eq!(
        h.submit(message(5)).unwrap_err().kind,
        ServiceErrorKind::OwnerClosed
    );
    let new = queue::ServiceQueue::new(AppWaker::testing(|| true));
    assert!(!new.handle().cancel(&a));
    assert!(other.pop().is_some());
}
#[test]
fn queue_capacity_and_failed_wake_do_not_publish_phantom_requests() {
    let q = queue::ServiceQueue::new(AppWaker::testing(|| true));
    let h = q.handle();
    let ids: Vec<_> = (0..64).map(|n| h.submit(message(n)).unwrap()).collect();
    assert_eq!(
        h.submit(message(99)).unwrap_err().kind,
        ServiceErrorKind::Capacity
    );
    assert!(h.cancel(&ids[7]));
    assert!(h.submit(message(99)).is_ok());
    assert_eq!(std::iter::from_fn(|| q.pop()).count(), 64);
    let failed = queue::ServiceQueue::new(AppWaker::testing(|| false));
    assert_eq!(
        failed.handle().submit(message(1)).unwrap_err().kind,
        ServiceErrorKind::OwnerClosed
    );
    assert!(failed.pop().is_none());
}
#[test]
fn dispatcher_waker_can_reenter_queue_without_holding_its_lock() {
    let slot = Arc::new(Mutex::new(None::<PlatformServiceHandle>));
    let current = slot.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let q = queue::ServiceQueue::new(AppWaker::testing(move || {
        count.fetch_add(1, Ordering::Relaxed);
        let h = current.lock().unwrap().clone().unwrap();
        h.submit(message(2)).unwrap();
        true
    }));
    let h = q.handle();
    *slot.lock().unwrap() = Some(h.clone());
    h.submit(message(1)).unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(std::iter::from_fn(|| q.pop()).count(), 2);
}
#[test]
fn concurrent_post_cancel_keeps_bounded_exact_delivery() {
    let q = queue::ServiceQueue::new(AppWaker::testing(|| true));
    let h = q.handle();
    std::thread::scope(|scope| {
        for n in 0..8 {
            let h = h.clone();
            scope.spawn(move || {
                for _ in 0..4 {
                    let id = h.submit(message(n)).unwrap();
                    assert!(h.cancel(&id));
                }
                h.submit(message(n)).unwrap();
            });
        }
    });
    assert_eq!(std::iter::from_fn(|| q.pop()).count(), 8);
}
#[test]
fn clipboard_bounds_and_shell_metadata_are_explicit() {
    assert!(
        ClipboardImage {
            width: 2049,
            height: 1,
            rgba: vec![]
        }
        .validate()
        .is_err()
    );
    assert!(
        ClipboardContent {
            text: Some("x\0y".into()),
            ..Default::default()
        }
        .validate()
        .is_err()
    );
    let icon = image();
    let action = TrayAction {
        id: 1,
        label: "Open".into(),
        enabled: true,
    };
    assert!(
        PlatformServiceRequest::SetTray(Some(TrayIcon {
            image: icon,
            tooltip: "Tray".into(),
            actions: vec![action.clone(), action]
        }))
        .validate()
        .is_err()
    );
    assert!(
        PlatformServiceRequest::PublishJumpList(JumpList {
            app_id: "org.example.App".into(),
            tasks: vec![JumpListTask {
                title: "Open".into(),
                arguments: "--new-document".into()
            }]
        })
        .validate()
        .is_ok()
    );
    assert!(
        PlatformServiceRequest::PublishJumpList(JumpList {
            app_id: "org.example.App".into(),
            tasks: vec![JumpListTask {
                title: "Open".into(),
                arguments: "--x\nmalformed".into()
            }]
        })
        .validate()
        .is_err()
    );
}
