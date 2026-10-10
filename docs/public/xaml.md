# Форма и поле ввода на XAML

`rust-desktop-ui-xaml` компилирует собственный ограниченный профиль XAML в Rust.
Первый профиль описывает нативное окно `Window` с одним однострочным `TextBox`.
Пример [xaml-form](../../examples/xaml-form/MainWindow.xaml) подключает это
описание к существующим retained controls, платформенному host и GPU renderer.

## Запуск примера

```text
cargo run -p xaml-form --locked
cargo run -p xaml-form --locked -- --dark
cargo run -p xaml-form --locked -- --light --smoke-test
```

`--light` и `--dark` переопределяют тему из разметки. `--smoke-test` запускает
проверку кадров и изменения размера с автоматическим завершением.
Дополнительный `--diagnostics` выводит разрешение имени редактора и его значения
при изменениях в stdout, добавляет счётчики renderer к заголовку. Без этого флага
введённый текст не журналируется, заголовок соответствует `Window.Title`.

## Разметка

```xml
<Window xmlns="urn:rust-ui-engine:ui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Форма · Rust UI"
        Width="640" Height="280"
        Padding="32" Theme="Light">
    <TextBox x:Name="Message"
             Height="36"
             AutomationName="Текст"
             PlaceholderText="Введите текст…" />
</Window>
```

Элементы принадлежат `urn:rust-ui-engine:ui`. Директива `x:Name` принадлежит
`http://schemas.microsoft.com/winfx/2006/xaml`. Префиксы произвольны: проверяется
URI пространства имён. Пространство имён WPF presentation не поддерживается.

| Элемент | Свойство | Значение по умолчанию и назначение |
|---|---|---|
| Window | `Title` | `Rust UI Form`; заголовок окна |
| Window | `Width`, `Height` | `640`, `360`; начальный размер клиентской области |
| Window | `Padding` | `32`; внутренние отступы |
| Window | `Theme` | `Light`; допускаются `Light` и `Dark` |
| TextBox | `x:Name` | Отсутствует; имя в области имён формы |
| TextBox | `Text` | Пустая строка; начальный текст |
| TextBox | `PlaceholderText` | Пустая строка; подсказка внутри пустого поля |
| TextBox | `AutomationName` | `Text`; доступное имя, не отдельная видимая подпись |
| TextBox | `Width` | Отсутствует; поле занимает доступную ширину |
| TextBox | `Height` | `36`; высота поля |
| TextBox | `Margin` | `0`; внешние отступы |
| TextBox | `IsEnabled` | `True`; также допускаются `False`, `true`, `false` |

Все размеры задаются в логических пикселях. Размеры должны быть конечными,
больше нуля и не больше `16384`; размеры редактора должны оставаться больше
нуля после преобразования в `f32`. Отступы допускают диапазон `[0, 16384]`.
`Padding` и `Margin` принимают одно число, два числа `horizontal,vertical`
либо четыре `left,top,right,bottom`. Отрицательные отступы и размер `Auto`
в этом профиле не поддерживаются; для растяжения `TextBox` не задавайте `Width`.

`x:Name` соответствует `[A-Za-z_][A-Za-z0-9_]*`. Пример разрешает имя через
`Form::find_name`; генератор не создаёт поле Rust с таким именем. Текстовые
свойства поддерживают Unicode и стандартные XML character/entity references,
например `&amp;`, `&quot;`, `&#x1F44B;`.

## Компиляция при сборке

Пакет приложения подключает `rust-desktop-ui-xaml` как обычную зависимость и
как `build-dependency`. В workspace используются зависимости по локальному
пути, как в [Cargo.toml примера](../../examples/xaml-form/Cargo.toml).

```rust
// build.rs
fn main() {
    println!("cargo:rerun-if-changed=MainWindow.xaml");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap())
        .join("main_window.rs");
    rust_desktop_ui_xaml::compile_file("MainWindow.xaml", output)
        .unwrap_or_else(|error| panic!("{error}"));
}
```

```rust
// src/main.rs
mod generated {
    include!(concat!(env!("OUT_DIR"), "/main_window.rs"));
}

// Передать определение собственному адаптеру приложения.
// let definition = generated::window_definition();
```

Результат — функция `window_definition() -> WindowDefinition` с типизированными
начальными свойствами. `compile(source)` возвращает этот Rust-код строкой;
`parse(source)` возвращает определение без генерации. `compile_file(input,
output)` читает UTF-8, проверяет разметку и записывает результат. Каталог output
должен существовать. Ошибка разметки содержит `путь:строка:столбец: сообщение`,
координаты начинаются с единицы; ошибки файлового ввода-вывода содержат путь и
сообщение. Строки экранируются как Rust-литералы и не исполняются.

При запуске примера XML не загружается: используется скомпилированное определение.
Адаптер находится в [form.rs](../../examples/xaml-form/src/form.rs), а библиотека
компилятора не зависит от controls, winit или wgpu.

## Поведение поля

`TextBox` монтируется через `Controls::edit_box`. Темы разметки `Light` и `Dark`
соответствуют `Theme::FluentLight` и `Theme::FluentDark` в controls. Это собственное
оформление со скруглённым полем, подсказкой и состояниями фокуса/доступности.

Поле использует общий `TextDocument`: Unicode-ввод, выделение, перемещение
каретки, undo/redo и маршруты clipboard/IME. CR, LF и tab удаляются из начального,
вставленного и программно заданного текста; Enter не добавляет строку.
Tab/Shift+Tab используют навигацию фокуса. При изменении размера сохраняются
документ и текущее редактирование. `AutomationName`, значение, геометрия и
доступные действия передаются через AccessKit.

## Границы профиля

Допускаются ровно один `Window` и один дочерний `TextBox`. Комментарии и
межэлементные пробелы разрешены. Неизвестные элементы, свойства и директивы,
повторяющиеся XML-атрибуты, непустой текст внутри элементов, processing
instructions и DTD приводят к ошибке. Внешние сущности не загружаются.
Лимит исходника — 1 MiB UTF-8; лимит XML-узлов — 128, включая комментарии.

Bindings, markup extensions (включая литеральное экранирование `{}`), ресурсы,
стили, шаблоны, панели раскладки и несколько полей ещё не реализованы. Значение
атрибута, начинающееся с `{` после начальных пробелов, отклоняется. Runtime XAML,
hot reload, WPF/DevExpress-совместимость и дизайнер Visual Studio не входят
в этот профиль.

Тесты компилятора проверяют диагностику, пространства имён, ограничения и
экранирование. Тесты примера сравнивают скомпилированное определение с исходником,
проверяют монтаж, resize, ввод и semantic actions. Эти проверки моделей не
заменяют отдельную проверку native IME, screen reader и GPU на каждой ОС.

На Windows 2026-10-10 выполнена проверка светлой и тёмной формы на DX12 и Vulkan:
размер клиентской области из XAML, Unicode-ввод, Backspace, потеря/возврат фокуса,
resize, minimize/restore и закрытие. Снимки проверяют цвета, глифы, скругления и
подчёркивание фокуса при масштабе 150%. Повторяемый сценарий:

```text
cargo build --release --locked -p xaml-form
python tests/windows_xaml_smoke.py --exe target/release/xaml-form.exe --output-dir target/xaml-smoke
```

Для тёмной темы добавьте `--dark`; backend выбирается переменной `WGPU_BACKEND`
(`dx12` или `vulkan`). Сценарий использует Win32-сообщения только своему окну;
он не подтверждает работу настоящей IME, внешнего clipboard или screen reader.
Перенос между мониторами с разным DPI и GPU-запуски Linux/macOS ещё не проверены.

## Происхождение и лицензии

Собственная реализация распространяется под Apache-2.0; код и ресурсы WPF или
DevExpress не заимствованы. XML разбирает `roxmltree 0.21.1`, доступный по
[MIT OR Apache-2.0](https://docs.rs/crate/roxmltree/0.21.1/source/Cargo.toml.orig).

Языковой ориентир — MS-XAML-2012. Microsoft разрешает использовать опубликованную
документацию для разработки реализаций и необходимые фрагменты для их описания;
эти разрешения на документацию сами по себе не предоставляют патентную лицензию.
[Условия MS-XAML-2012](https://learn.microsoft.com/en-us/openspecs/microsoft_domain_specific_languages/ms-xaml-2012/de14496a-389b-45b1-a52c-6b6731507b55)

MS-XAML-2012 включена в Open Specification Promise. Обещание относится к
определённым патентным требованиям Microsoft в пределах соответствующей
спецификации; FAQ допускает частичные реализации. Собственные расширения не
получают автоматического покрытия, а OSP не гарантирует отсутствие прав третьих
лиц и содержит условие о патентных исках против соответствующей реализации
Microsoft. [OSP](https://learn.microsoft.com/en-us/openspecs/dev_center/ms-devcentlp/1c24c7c8-28b0-4ce1-a47d-95fe1ff504bc),
[FAQ](https://learn.microsoft.com/en-us/openspecs/dev_center/ms-devcentlp/03347966-f8ff-4d53-a05e-63419d4132e2)

Архитектурное решение: [ADR-005](adr/ADR-005-compiled-xaml.md).
