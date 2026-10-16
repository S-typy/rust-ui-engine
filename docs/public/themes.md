# Темы, палитры и плотность интерфейса

`rust-desktop-ui-controls` использует семантические роли `ThemeTokens` и
`ChromeTokens`. Оформление кнопок, редакторов, Ribbon, меню, групп и статусных
панелей разрешается относительно текущей темы. Явные свойства `ButtonStyle`,
`EditBoxStyle`, `TextStyle` и размеров приложения имеют приоритет.

## Выбор темы

Сохраняются `Theme::Light`, `Dark`, `Compact`, `FluentLight`, `FluentDark`.
`HighContrastLight` и `HighContrastDark` задают контрастные поверхности, видимые
рамки и монохромные значки. Режим и плотность можно настраивать независимо:

```rust
use rust_desktop_ui_controls::{
    Controls, Theme, ThemeConfig, ThemeDensity, ThemeMode,
};

let mut ui = Controls::new(Theme::FluentLight)?;
let config = ThemeConfig::new(ThemeMode::Light)
    .with_density(ThemeDensity::Compact)
    .with_accent([112, 52, 180])?;
ui.set_theme(Theme::Configured(config))?;
# Ok::<(), rust_desktop_ui_controls::ControlError>(())
```

`ThemeConfig` имеет закрытые поля и остаётся `Copy + Eq`. `with_density` меняет
только метрики; `with_accent` и `with_palette` возвращают `Result` и проверяют
конечные цвета до применения. Исходная конфигурация при ошибке сохраняется.
`Theme::is_dark`, `is_high_contrast`, `density`, `is_compact` позволяют выбирать
представление без сравнения с отдельным вариантом перечисления.

| `ThemeDensity` | Высота контрола | Размер шрифта | Интервал |
|---|---:|---:|---:|
| Comfortable | 36 | 14 | 8 |
| Compact | 28 | 13 | 4 |
| Touch | 44 | 16 | 12 |

Метрики выражены в logical pixels. Старый `Theme::Compact` сохраняет высоту 26.
Плотность меняет размеры по умолчанию, текст и минимальные размеры строк,
QAT и вкладок Ribbon. `LayoutStyle` с явными размерами, padding или gap остаётся
настройкой приложения; токен `spacing` доступен контейнерам для собственного
расчёта интервалов. Преобразование к DPI выполняется при подготовке сцены и текста.

## Собственная палитра

`ThemePalette` хранит непрозрачные sRGB цвета как `[u8; 3]`. Палитра содержит
canvas, surface, text, muted, border, accent и error; hover, pressed, selected
и поверхности chrome выводятся из этих ролей.

```rust
use rust_desktop_ui_controls::{Theme, ThemeConfig, ThemeMode, ThemePalette};

let palette = ThemePalette {
    canvas: [246, 242, 233],
    surface: [255, 251, 241],
    text: [40, 27, 18],
    muted: [96, 73, 52],
    border: [130, 97, 60],
    accent: [116, 61, 26],
    error: [170, 20, 20],
};
let theme = Theme::Configured(
    ThemeConfig::new(ThemeMode::Light).with_palette(palette)?
);
assert_eq!(theme.tokens().canvas, rust_desktop_ui_core::Color::rgb(246, 242, 233));
# Ok::<(), rust_desktop_ui_controls::ControlError>(())
```

`ThemePalette::for_mode(mode)` возвращает исходные роли выбранного режима.
Отдельный accent override имеет приоритет над `palette.accent`.
Контраст основного текста проверяется на normal/hover/pressed/selected,
editor/card/flyout поверхностях: минимум 4.5:1, в high-contrast минимум 7:1.
Для вторичного текста на базовых поверхностях требуется 4.5:1; для focus/error
— 3:1. High-contrast дополнительно проверяет рамку 3:1.

`Theme::on_accent()` выбирает чёрный или белый цвет для отметки на accent fill.
В high-contrast обычные кнопки и редакторы получают рамку 2 logical pixels,
disabled text остаётся читаемым, значки не используют декоративные цветовые
акценты, тень отсутствует. В обычном режиме `Theme::Configured` декоративные
акценты значков Ribbon/QAT используют настроенный accent; остальные значки
сохраняют foreground. При необходимости явные цветовые overrides сохраняют
приоритет и могут менять исходное соотношение контраста.

## Смена темы в работающей форме

`Controls::set_theme` не пересоздаёт обычные контролы. Черновик и принятое
значение редактора, selection, undo/redo, фокус, checked state и явные стили
сохраняются. После смены темы выполните обычный цикл update/layout/paint.
Ribbon пересчитывает своё представление с сохранением общей команды и её
checked state; размеры и палитра не активируют команды.

High-contrast включается явно через тему. Автоматическое чтение системных
настроек оформления и подписка на их изменения принадлежат приложению/host.
Пользовательские фиксированные размеры не растягиваются автоматически;
достаточность места для выбранного текста проверяется компоновкой приложения.
Адаптер compiled XAML пересчитывает стандартные высоты в обычном цикле
`update_form_layout`. Явные `Height`, привязки высоты и изменённые приложением
размеры сохраняются. Настройки плотности не переписывают заданные в разметке
отступы и расстояния между элементами.

Тесты проверяют контраст состояний, независимость density/palette,
атомарное отклонение плохой палитры, монохромные high-contrast значки,
смену оформления живой формы и адаптацию Ribbon. Оконные GPU проверки
учитываются отдельно от unit-тестов.
