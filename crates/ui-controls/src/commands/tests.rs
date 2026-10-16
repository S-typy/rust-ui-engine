use super::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn settle(commands: &mut CommandRegistry, id: &CommandId) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while matches!(
        commands.async_state(id),
        Some(CommandAsyncState::Running { .. })
    ) {
        commands.poll_async();
        assert!(Instant::now() < deadline, "worker did not complete");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn typed_parameters_are_checked_before_toggle_predicate_or_callback() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("add");
    commands.register_toggle(id.clone(), "Add", false);
    let calls = Rc::new(Cell::new(0));
    let out = calls.clone();
    commands.on_execute_with(&id, move |n: &u32| out.set(out.get() + *n));
    assert!(!commands.enabled(&id));
    assert!(!commands.activate(&id));
    assert!(!commands.activate_with(&id, CommandParameter::new("wrong")));
    commands.set_parameter(&id, CommandParameter::new(3_u32));
    commands.set_can_execute_with(&id, |n: &u32| *n < 5);
    assert!(commands.enabled(&id));
    assert!(!commands.activate_with(&id, CommandParameter::new(9_u32)));
    assert!(commands.activate(&id));
    assert_eq!(calls.get(), 3);
    assert_eq!(commands.checked(&id), Some(true));
    assert!(commands.execute_with(&id, CommandParameter::new(2_u32)));
    assert_eq!(calls.get(), 5);
    assert_eq!(commands.checked(&id), Some(true));
    assert_eq!(commands.drain_invocations().len(), 2);
    assert_eq!(
        commands.parameter(&id).unwrap().downcast_ref::<u32>(),
        Some(&3)
    );
    commands.set_can_execute_with(&id, |_: &String| true);
    assert!(!commands.enabled(&id));
    assert!(commands.clear_parameter_gate(&id));
    assert!(commands.enabled(&id));
}

#[test]
fn disabled_and_busy_commands_do_not_reenter_predicates() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    let gates = Rc::new(Cell::new(0));
    let out = gates.clone();
    commands.set_can_execute(&id, move || {
        out.set(out.get() + 1);
        true
    });
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    commands.on_async_execute(
        &id,
        move |_: &(), _| {
            wait.lock().unwrap().recv().unwrap();
            Ok(7)
        },
        |_| {},
    );
    commands.set_enabled(&id, false);
    assert!(!commands.enabled(&id));
    assert!(!commands.activate(&id));
    assert_eq!(gates.get(), 0);
    commands.set_enabled(&id, true);
    assert!(commands.activate(&id));
    assert!(!commands.enabled(&id));
    assert!(!commands.execute(&id));
    assert_eq!(gates.get(), 1);
    release.send(()).unwrap();
    settle(&mut commands, &id);
}

#[test]
fn async_result_runs_once_on_ui_thread_and_captures_activation_parameter() {
    let ui_thread = std::thread::current().id();
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register_toggle(id.clone(), "Task", false);
    let wakes = Arc::new(AtomicUsize::new(0));
    let out = wakes.clone();
    commands.set_waker(Some(Arc::new(move || {
        out.fetch_add(1, Ordering::SeqCst);
    })));
    let result = Rc::new(RefCell::new(Vec::new()));
    let out = result.clone();
    commands.on_async_execute(
        &id,
        move |value: &u32, token| {
            assert_ne!(std::thread::current().id(), ui_thread);
            token.check()?;
            Ok(value * 2)
        },
        move |value| {
            assert_eq!(std::thread::current().id(), ui_thread);
            out.borrow_mut().push(value);
        },
    );
    commands.set_parameter(&id, CommandParameter::new(21_u32));
    assert!(commands.activate(&id));
    commands.set_parameter(&id, CommandParameter::new(100_u32));
    settle(&mut commands, &id);
    assert_eq!(&*result.borrow(), &[Ok(42)]);
    assert_eq!(commands.checked(&id), Some(true));
    assert_eq!(commands.poll_async(), 0);
    assert!(commands.enabled(&id));
    assert!(wakes.load(Ordering::SeqCst) >= 2);
    assert_eq!(commands.drain_invocations().len(), 1);
    assert_eq!(
        commands.drain_async_events()[0].outcome,
        CommandAsyncOutcome::Succeeded
    );
}

#[test]
fn cancel_wins_over_unpolled_success_without_toggle_rollback() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register_toggle(id.clone(), "Task", false);
    let (sent, received) = mpsc::channel();
    let outcomes = Rc::new(RefCell::new(Vec::new()));
    let out = outcomes.clone();
    commands.on_async_execute(
        &id,
        move |_: &(), _| {
            sent.send(()).unwrap();
            Ok(42)
        },
        move |r| out.borrow_mut().push(r),
    );
    assert!(commands.activate(&id));
    received.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(commands.cancel(&id));
    assert!(!commands.enabled(&id));
    settle(&mut commands, &id);
    assert_eq!(&*outcomes.borrow(), &[Err(CommandTaskError::Cancelled)]);
    assert_eq!(commands.checked(&id), Some(true));
    assert!(!commands.cancel(&id));
    assert_eq!(
        commands.drain_async_events()[0].outcome,
        CommandAsyncOutcome::Cancelled
    );
}

#[test]
fn replace_and_unregister_cancel_without_stale_completion() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Old");
    let (entered, seen) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    let (cancelled, observed) = mpsc::channel();
    commands.on_async_execute(
        &id,
        move |_: &(), token| {
            entered.send(()).unwrap();
            wait.lock().unwrap().recv().unwrap();
            cancelled.send(token.is_cancelled()).unwrap();
            Ok(())
        },
        |_| panic!("stale completion"),
    );
    assert!(commands.activate(&id));
    seen.recv_timeout(Duration::from_secs(10)).unwrap();
    commands.register(id.clone(), "New");
    release.send(()).unwrap();
    assert!(observed.recv_timeout(Duration::from_secs(10)).unwrap());
    assert_eq!(commands.poll_async(), 0);
    assert_eq!(commands.label(&id), Some("New"));
    assert!(commands.unregister(&id));
    assert!(!commands.enabled(&id));
    assert!(!commands.unregister(&id));
}

#[test]
fn registry_drop_requests_cancellation_and_stops_future_wake_delivery() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    let wakes = Arc::new(AtomicUsize::new(0));
    let out = wakes.clone();
    commands.set_waker(Some(Arc::new(move || {
        out.fetch_add(1, Ordering::SeqCst);
    })));
    let (entered, seen) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    let (cancelled, observed) = mpsc::channel();
    commands.on_async_execute(
        &id,
        move |_: &(), token| {
            entered.send(()).unwrap();
            wait.lock().unwrap().recv().unwrap();
            cancelled.send(token.is_cancelled()).unwrap();
            Ok(())
        },
        |_| panic!("dropped callback"),
    );
    commands.activate(&id);
    seen.recv_timeout(Duration::from_secs(10)).unwrap();
    drop(commands);
    release.send(()).unwrap();
    assert!(observed.recv_timeout(Duration::from_secs(10)).unwrap());
    assert_eq!(wakes.load(Ordering::SeqCst), 1);
}

#[test]
fn worker_failure_and_panic_are_terminal_and_error_messages_are_bounded() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    commands.on_async_execute(
        &id,
        |_: &(), _| Err::<(), _>(CommandTaskError::Failed("я".repeat(4000))),
        |r| {
            assert!(matches!(r, Err(CommandTaskError::Failed(s)) if s.len() <= 4096 && s.capacity() <= 4096));
        },
    );
    commands.activate(&id);
    settle(&mut commands, &id);
    assert!(
        matches!(&commands.drain_async_events()[0].outcome, CommandAsyncOutcome::Failed(s) if s.len() == 4096)
    );
    commands.on_async_execute(
        &id,
        |_: &(), _| -> Result<(), CommandTaskError> { panic!("worker failure") },
        |r| assert_eq!(r, Err(CommandTaskError::Panicked)),
    );
    commands.activate(&id);
    settle(&mut commands, &id);
    assert_eq!(
        commands.drain_async_events()[0].outcome,
        CommandAsyncOutcome::Panicked
    );
}

#[test]
fn completion_panic_is_not_replayed_and_host_wake_panic_does_not_lose_result() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    commands.set_waker(Some(Arc::new(|| panic!("faulty host"))));
    let calls = Rc::new(Cell::new(0));
    let out = calls.clone();
    commands.on_async_execute(
        &id,
        |_: &(), _| Ok(()),
        move |_| {
            out.set(out.get() + 1);
            panic!("faulty completion");
        },
    );
    assert!(commands.activate(&id));
    let deadline = Instant::now() + Duration::from_secs(10);
    while commands.active_workers() > 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| commands.poll_async())).is_err()
    );
    assert_eq!(calls.get(), 1);
    assert!(commands.enabled(&id));
    assert_eq!(commands.poll_async(), 0);
    assert_eq!(commands.drain_async_events().len(), 1);
}

#[test]
fn short_worker_diagnostic_does_not_retain_an_oversized_backing_allocation() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    commands.on_async_execute(
        &id,
        |_: &(), _| {
            let mut message = String::with_capacity(1024 * 1024);
            message.push_str("denied");
            Err::<(), _>(CommandTaskError::Failed(message))
        },
        |result| {
            let Err(CommandTaskError::Failed(message)) = result else {
                panic!("expected failure");
            };
            assert_eq!(message, "denied");
            assert!(message.capacity() <= 4096);
        },
    );
    assert!(commands.activate(&id));
    settle(&mut commands, &id);
}

#[test]
fn cancelled_replaced_workers_still_count_toward_global_limit() {
    let mut commands = CommandRegistry::default();
    let (entered, seen) = mpsc::channel();
    let mut releases = Vec::new();
    for i in 0..CommandRegistry::MAX_ASYNC_WORKERS {
        let id = CommandId::from(format!("task{i}"));
        commands.register(id.clone(), "Task");
        let (release, wait) = mpsc::channel();
        releases.push(release);
        let wait = Mutex::new(wait);
        let entered = entered.clone();
        commands.on_async_execute(
            &id,
            move |_: &(), _| {
                entered.send(()).unwrap();
                wait.lock().unwrap().recv().unwrap();
                Ok(())
            },
            |_| {},
        );
        assert!(commands.activate(&id));
        seen.recv_timeout(Duration::from_secs(10)).unwrap();
        commands.unregister(&id);
    }
    let id = CommandId::from("overflow");
    commands.register_toggle(id.clone(), "Task", false);
    commands.on_async_execute(&id, |_: &(), _| Ok(()), |_| {});
    assert!(!commands.enabled(&id));
    assert!(!commands.activate(&id));
    assert_eq!(commands.checked(&id), Some(false));
    assert_eq!(
        commands.active_workers(),
        CommandRegistry::MAX_ASYNC_WORKERS
    );
    for release in releases {
        release.send(()).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    while commands.active_workers() > 0 {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(commands.activate(&id));
    settle(&mut commands, &id);
}

#[test]
fn bounded_notifications_report_loss_without_reexecuting_actions() {
    let mut commands = CommandRegistry::default();
    let id = CommandId::from("task");
    commands.register(id.clone(), "Task");
    for _ in 0..CommandRegistry::MAX_PENDING_INVOCATIONS + 3 {
        assert!(commands.activate(&id));
    }
    assert_eq!(
        commands.drain_invocations().len(),
        CommandRegistry::MAX_PENDING_INVOCATIONS
    );
    assert_eq!(commands.take_dropped_invocations(), 3);
    assert_eq!(commands.take_dropped_invocations(), 0);
    for sequence in 0..CommandRegistry::MAX_PENDING_ASYNC_EVENTS {
        commands.async_events.push_back(CommandAsyncEvent {
            id: id.clone(),
            sequence: sequence as u64,
            outcome: CommandAsyncOutcome::Succeeded,
        });
    }
    commands.on_async_execute(&id, |_: &(), _| Ok(()), |_| {});
    commands.activate(&id);
    settle(&mut commands, &id);
    assert_eq!(
        commands.drain_async_events().len(),
        CommandRegistry::MAX_PENDING_ASYNC_EVENTS
    );
    assert_eq!(commands.take_dropped_async_events(), 1);
    assert_eq!(commands.take_dropped_async_events(), 0);
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Document {
    first: i32,
    second: String,
}
#[test]
fn grouped_participants_commit_undo_and_redo_atomically() {
    let mut document = Document {
        first: 1,
        second: "one".into(),
    };
    let original = document.clone();
    let mut history = CommandHistory::default();
    let mut group = history.begin(&mut document, "Two participants").unwrap();
    group
        .apply(|draft| {
            draft.first = 2;
            Ok(())
        })
        .unwrap();
    group
        .apply(|draft| {
            draft.second = "two".into();
            Ok(())
        })
        .unwrap();
    assert_eq!(group.draft().first, 2);
    assert!(group.commit().unwrap());
    let changed = document.clone();
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo_label(), Some("Two participants"));
    history.undo(&mut document).unwrap();
    assert_eq!(document, original);
    assert_eq!(history.redo_label(), Some("Two participants"));
    history.redo(&mut document).unwrap();
    assert_eq!(document, changed);
}
#[test]
fn failed_dropped_and_panicking_groups_never_publish_partial_state() {
    let mut state = 1;
    let mut history = CommandHistory::default();
    let mut group = history.begin(&mut state, "Fail").unwrap();
    group
        .apply(|draft| {
            *draft = 2;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        group.apply(|draft| {
            *draft = 3;
            Err("rejected".into())
        }),
        Err(CommandHistoryError::Rejected(_))
    ));
    assert_eq!(group.apply(|_| Ok(())), Err(CommandHistoryError::Aborted));
    assert_eq!(group.commit(), Err(CommandHistoryError::Aborted));
    assert_eq!(state, 1);
    assert_eq!(history.undo_len(), 0);
    {
        let mut group = history.begin(&mut state, "Drop").unwrap();
        group
            .apply(|draft| {
                *draft = 2;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(state, 1);
    let mut group = history.begin(&mut state, "Panic").unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| group.apply(|draft| {
            *draft = 4;
            panic!("participant panic")
        })))
        .is_err()
    );
    assert_eq!(group.commit(), Err(CommandHistoryError::Aborted));
    assert_eq!(state, 1);
}
#[test]
fn history_conflict_and_validation_preserve_state_and_cursor() {
    let mut state = 1;
    let mut history = CommandHistory::default();
    history
        .execute(&mut state, "Edit", |s| {
            *s = 2;
            Ok(())
        })
        .unwrap();
    assert!(
        history
            .undo_checked(&mut state, |_| Err("no permission".into()))
            .is_err()
    );
    assert_eq!(state, 2);
    assert_eq!(history.undo_len(), 1);
    state = 99;
    assert!(!history.can_undo(&state));
    assert_eq!(history.undo(&mut state), Err(CommandHistoryError::Conflict));
    assert_eq!(state, 99);
    state = 2;
    history.undo(&mut state).unwrap();
    assert!(
        history
            .redo_checked(&mut state, |_| Err("no permission".into()))
            .is_err()
    );
    assert_eq!(state, 1);
    assert_eq!(history.redo_len(), 1);
    state = 99;
    assert_eq!(history.redo(&mut state), Err(CommandHistoryError::Conflict));
    assert_eq!(history.redo_len(), 1);
}
#[test]
fn no_op_preserves_redo_and_capacity_evicts_oldest_complete_group() {
    let mut state = 0;
    let mut history = CommandHistory::with_capacity(2).unwrap();
    for value in 1..=3 {
        history
            .execute(&mut state, format!("Set {value}"), |s| {
                *s = value;
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(history.undo_len(), 2);
    history.undo(&mut state).unwrap();
    assert!(!history.execute(&mut state, "No-op", |_| Ok(())).unwrap());
    assert_eq!(history.redo_len(), 1);
    history
        .execute(&mut state, "Branch", |s| {
            *s = 9;
            Ok(())
        })
        .unwrap();
    assert_eq!(history.redo_len(), 0);
    history.undo(&mut state).unwrap();
    history.undo(&mut state).unwrap();
    assert_eq!(state, 1);
    assert_eq!(history.undo(&mut state), Err(CommandHistoryError::Empty));
    assert!(CommandHistory::<i32>::with_capacity(0).is_err());
    assert!(CommandHistory::<i32>::with_capacity(4097).is_err());
    assert!(history.begin(&mut state, "").is_err());
    assert!(history.begin(&mut state, "a".repeat(4097)).is_err());
}

#[test]
fn business_history_can_be_invoked_through_shared_registry_gate() {
    let model = Rc::new(RefCell::new((0, CommandHistory::default())));
    let mut commands = CommandRegistry::default();
    let edit = CommandId::from("edit");
    let undo = CommandId::from("undo");
    let redo = CommandId::from("redo");
    commands.register(edit.clone(), "Edit");
    commands.register(undo.clone(), "Undo");
    commands.register(redo.clone(), "Redo");
    let shared = model.clone();
    commands.on_execute_with(&edit, move |value: &i32| {
        let mut pair = shared.borrow_mut();
        let (state, history) = &mut *pair;
        history
            .execute(state, "Change", |s| {
                *s = *value;
                Ok(())
            })
            .unwrap();
    });
    let shared = model.clone();
    commands.set_can_execute(&undo, move || {
        let pair = shared.borrow();
        pair.1.can_undo(&pair.0)
    });
    let shared = model.clone();
    commands.on_execute(&undo, move || {
        let mut pair = shared.borrow_mut();
        let (state, history) = &mut *pair;
        history.undo(state).unwrap();
    });
    let shared = model.clone();
    commands.set_can_execute(&redo, move || {
        let pair = shared.borrow();
        pair.1.can_redo(&pair.0)
    });
    let shared = model.clone();
    commands.on_execute(&redo, move || {
        let mut pair = shared.borrow_mut();
        let (state, history) = &mut *pair;
        history.redo(state).unwrap();
    });
    assert!(!commands.activate(&undo));
    assert!(commands.activate_with(&edit, CommandParameter::new(42_i32)));
    assert!(commands.activate(&undo));
    assert_eq!(model.borrow().0, 0);
    assert!(commands.activate(&redo));
    assert_eq!(model.borrow().0, 42);
}
