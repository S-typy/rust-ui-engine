# Команды и история действий

`CommandRegistry` хранит одну запись приложения для всех её размещений:
Button, ToggleButton, Ribbon, QAT и пунктов меню. `CommandId` не зависит от
WidgetId. Подпись, enabled, checked, predicate, параметр и состояние выполнения
принадлежат записи. Hover и focus принадлежат размещению.

## Активация и параметры

`activate` проверяет доступность, один раз переключает `Some(checked)`, выполняет
callback или запускает задачу и создаёт один `CommandInvocation`. `execute`
использует тот же gate, сохраняя checked. Отказ ничего не переключает и не
создаёт invocation. `set_checked` сам по себе не выполняет действие.

`CommandParameter::new(value)` хранит исходный Rust-тип через разделяемое
неизменяемое значение `Send + Sync + 'static`. `set_parameter` задаёт общий
параметр для всех размещений. `activate_with` / `execute_with` используют
параметр только этого вызова, не меняя общий. Изначальное значение — `()`.

```rust
use rust_desktop_ui_controls::{CommandId, CommandParameter, CommandRegistry};
let mut commands = CommandRegistry::default();
let open = CommandId::from("open-record");
commands.register(open.clone(), "Открыть");
commands.set_parameter(&open, CommandParameter::new(42_i64));
commands.set_can_execute_with(&open, |key: &i64| *key > 0);
commands.on_execute_with(&open, |key: &i64| println!("Record {key}"));
assert!(commands.activate(&open));
assert!(!commands.activate_with(&open, CommandParameter::new("wrong type")));
```

`set_can_execute` и `set_can_execute_with` применяются совместно. Несовпадение
типа handler или typed predicate отклоняет вызов до callback и изменения checked.
`clear_parameter_gate` удаляет typed predicate. Predicates должны быть чистыми:
визуальные размещения могут проверять их несколько раз; сама активация вызывает
каждый подходящий predicate один раз. Изменение внешней модели требует запроса
обновления интерфейса.

`register` заменяет запись целиком; `register_toggle` дополнительно задаёт checked.
`unregister` удаляет её. Существующие размещения автоматически видят новое состояние.
Синхронные callbacks работают на вызывающем потоке и не должны паниковать или
повторно заимствовать registry через `RefCell`.

## Асинхронные действия

`on_async_execute::<P, R>` задаёт worker и completion. Worker получает `&P` и
`CancellationToken` в отдельном потоке; возвращает `Result<R, CommandTaskError>`.
Он не получает `Controls`, registry или ссылки на UI. `P: Send + Sync`, `R: Send`.
Completion работает только при `poll_async` на потоке владельца registry.

```rust
use rust_desktop_ui_controls::{CommandId, CommandParameter, CommandRegistry};
let mut commands = CommandRegistry::default();
let calculate = CommandId::from("calculate");
commands.register(calculate.clone(), "Рассчитать");
commands.set_parameter(&calculate, CommandParameter::new(21_u32));
commands.on_async_execute(&calculate, |value: &u32, cancel| {
    cancel.check()?;
    Ok(value * 2)
}, |result| {
    if let Ok(value) = result { println!("Result: {value}"); }
});
assert!(commands.activate(&calculate));
// Host schedules another UI update when its registered wake callback runs.
commands.poll_async();
```

Lifecycle читается через `async_state`: `Idle → Running → Finished`.
`Finished` содержит sequence и `Succeeded`, `Cancelled`, `Failed` или `Panicked`.
Пока задача не завершена и не обработана, команда недоступна во всех размещениях;
повторный запуск отклоняется. Изменение общего параметра не изменяет уже переданный
задаче снимок. Checked отражает принятую активацию: ошибка или отмена не меняет его
задним числом. Приложение может явно установить итоговое состояние в completion.

`cancel` запрашивает кооперативную отмену. Worker должен проверять токен между
порциями работы. Принудительного прерывания потоков нет. Registry остаётся busy
до фактического выхода worker; отмена выигрывает у уже готового, но ещё не
обработанного результата. Completion получает `Cancelled` ровно один раз.
Замена handler, повторная регистрация ID, `unregister` и drop registry отменяют
старую задачу и отбрасывают её будущий completion. Они не ждут зависший worker.

`set_waker(Some(Arc<dyn Fn() + Send + Sync>))` подключает пробуждение host.
Callback только планирует UI-обновление и может быть вызван из worker.
Установка callback сразу отправляет hint, чтобы не потерять уже готовый результат.
После drop новые уведомления не планируются; уже начавшийся wake должен безопасно
обрабатываться самим host. `Controls::update` обрабатывает завершения.

На registry допускается не больше 16 фактически работающих workers. Отменённые
или заменённые tasks занимают слот до выхода, поэтому повторная регистрация не
обходит лимит. Размер параметров и результатов определяет приложение.
Completion очереди ограничены 1024 событиями; invocation очереди — 4096.
При переполнении удаляются старейшие уведомления, действие не повторяется.
`take_dropped_async_events` / `take_dropped_invocations` возвращают количество
потерь; приложение затем читает актуальные состояния. Текст worker-ошибки
ограничен 4096 байтами на границе UTF-8. Паника worker становится `Panicked`.

## Составные действия и undo/redo

`CommandHistory<S>` координирует бизнес-действия над снимком документа `S`.
Состояние документа остаётся у приложения. История текстового редактора работает
отдельно. `S: Clone + PartialEq` должен быть самостоятельным значением:
клонирование обязано изолировать изменяемые поля всех участников. Для этого
подходят owned структуры и коллекции; `Rc<RefCell<_>>` внутри снимка не подходит.

```rust
use rust_desktop_ui_controls::CommandHistory;
#[derive(Clone, PartialEq)]
struct Document { title: String, revision: u64 }
let mut document = Document { title: "Old".into(), revision: 0 };
let mut history = CommandHistory::default();
let mut group = history.begin(&mut document, "Переименовать документ")?;
group.apply(|draft| { draft.title = "New".into(); Ok(()) })?;
group.apply(|draft| { draft.revision += 1; Ok(()) })?;
group.commit()?;
history.undo(&mut document)?; // title и revision восстанавливаются вместе
history.redo(&mut document)?;
# Ok::<(), rust_desktop_ui_controls::CommandHistoryError>(())
```

Каждый `apply` изменяет только draft. Любой отказ окончательно отменяет группу;
даже после перехваченной паники её нельзя случайно подтвердить. Drop без `commit`
отбрасывает draft. Успешный commit заменяет живой снимок один раз. `execute` —
сокращение для группы с одним callback. Внешние I/O и shared mutable handles
не являются частью этой транзакции; такие действия приложение координирует отдельно.

Undo/redo восстанавливают соответствующий снимок всех участников без повторного
вызова callbacks. Если документ изменён вне истории, сравнение с ожидаемым снимком
даёт `Conflict`, сохраняя документ и курсор. `undo_checked` / `redo_checked`
дополнительно проверяют кандидат по актуальным доменным правилам перед заменой.
Отказ проверки также ничего не меняет. После загрузки другого документа используйте
`clear` или другую history.

История хранит до 128 групп по умолчанию; `with_capacity` принимает 1–4096.
Лимит относится к числу целых групп, размер снимка определяет приложение.
Новая успешная группа удаляет redo-ветку, no-op её сохраняет. `can_undo`, `can_redo`,
`undo_label` и `redo_label` позволяют подключить обычные команды Undo/Redo к общему
registry. Вложенные транзакции и одновременное изменение документа исключаются
mutable borrow на время группы.
