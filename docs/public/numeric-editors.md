# Числовые редакторы

`NumericEdit` хранит `Option<DecimalValue>` без преобразования через `f64`.
`DecimalValue` использует коэффициент `i128` и масштаб до 28 знаков; представление
нормализовано удалением конечных нулей. Существующий `SpinBox` сохраняет API
для конечных `f64` и invariant-формат.

```rust
use rust_desktop_ui_controls::{Controls, DecimalValue, NumericEditSettings, UiCulture};
use rust_desktop_ui_core::LayoutStyle;

fn amount(ui: &mut Controls) -> Result<(), Box<dyn std::error::Error>> {
    let parts = ui.numeric_edit(
        ui.root(),
        "Сумма",
        Some("1234.56".parse::<DecimalValue>()?),
        NumericEditSettings {
            culture: UiCulture::Russian,
            grouping: true,
            ..Default::default()
        },
        LayoutStyle::default(),
    )?;
    assert_eq!(ui.numeric_value(parts.container), Some(Some("1234.56".parse()?)));
    Ok(())
}
```

`NumericEditSettings` задаёт `minimum`, `maximum`, положительный `step`,
`precision` (0–28), `allow_empty`, `culture`, `grouping`, `calculator`.
По умолчанию диапазон ±10¹⁵, шаг 1, точность 2, пустое значение разрешено,
культура русская, группировка выключена, калькулятор включён. Нормализованный
масштаб границ, шага и значения не превышает `precision`. Пустое значение
отличается от нуля. Первый шаг из пустого значения выбирает ноль, ограниченный
диапазоном; следующие шаги прибавляют или вычитают `step`, насыщаясь на границе.

`parse` проверяет display-текст, `parse_storage` — canonical storage, `format`
выдаёт ровно `precision` дробных знаков. Русский формат использует запятую и
неразрывный пробел U+00A0; английский — точку и запятую; арабский — разделители
U+066B/U+066C. Во всех трёх форматах используются латинские цифры 0–9.
Корректные группы по три цифры принимаются независимо от настройки отображения.
Разрешён внешний пробельный отступ и знак; экспоненты, NaN/Infinity, неверная
группировка, лишние разделители и ввод длиннее 512 байт отклоняются. Значащие
дробные знаки сверх `precision` отклоняются, а не округляются.

`set_numeric_value` проверяет и атомарно загружает значение через обычный
application setter редактора. `set_numeric_settings` требует чистый документ
без композиции и закрытый калькулятор; несовместимые настройки оставляют
значение и старую конфигурацию. Привязки читают актуальные настройки при смене
культуры. `EditorValue::Decimal` сообщает типизированное подтверждённое значение.
Ввод, стрелки шага, Enter, Escape и уход фокуса используют обычный editor session,
включая синхронные и асинхронные проверки. Программная загрузка значения
сохраняет стандартный контракт `set_editor_value`.

## Привязки и XAML

`Value` привязан к `DataValue::String` с canonical decimal или `Null`.
Строка использует точку, без группировки, начального плюса, лишних нулей и
конечных дробных нулей. `DataValue::Number` отклоняется: преобразование могло бы
потерять точность. `Text` остаётся отображаемым текстом. `LostFocus` публикует
accepted value; `PropertyChanged` публикует корректный draft по обычным правилам
bindings. При async validator публикация ждёт подтверждения и при
`PropertyChanged`.

```xml
<NumericEdit Value="{Binding Amount, Mode=TwoWay, UpdateSourceTrigger=LostFocus}"
             Minimum="0" Maximum="1000000" Increment="0.01"
             Precision="2" AllowEmpty="True" Culture="ru-RU"
             UseGrouping="True" ShowCalculator="True" />
```

Literal `Value`, `Minimum`, `Maximum`, `Increment` используют canonical strings;
display-культура от них независима. Содержимое формы обращается к
`EditorControl.container`, а настройки текста и validators — к `editor`.

## Калькулятор

`open_numeric_calculator`, `press_numeric_calculator`,
`apply_numeric_calculator`, `cancel_numeric_calculator` управляют retained
Flyout. Его кнопки и клавиатура используют отдельный `NumericCalculator`.
Цифры, десятичный разделитель, Backspace/Delete, `+`, `-`, `*`, `/`, `=`
редактируют вычисление; Enter применяет результат, Escape отменяет.
`CalculatorKey::Negate` меняет знак. Вычисления выполняются слева направо;
скобки, произвольные выражения, скрипты и финансовые функции не исполняются.

`calculate_decimal` поддерживает четыре операции и именованную политику
`NumericRounding::HalfAwayFromZero`: результат округляется до `precision`,
ровно половина округляется от нуля. Сложение и вычитание также вычисляются
точно до этого явно выбранного округления. Ограниченные десятичные промежуточные
числа исключают ложное переполнение при рескейле, сокращении и делении больших
коэффициентов. Непредставимый итог и деление на ноль возвращают ошибку;
состояние калькулятора остаётся прежним. Промежуточное значение может выходить
за диапазон редактора; диапазон проверяется при Apply.

До Apply калькулятор не меняет документ, accepted value, selection/undo и модель
формы. Apply сначала выполняет обычную валидацию, затем передаёт результат в
draft и запускает `commit_edit`. Ошибка сохраняет popup и вычисление; Pending
оставляет popup открытым до подтверждения. Cancel, потеря окна, скрытие,
отключение или удаление владельца отменяют незавершённое подтверждение.
После Apply/Pending отмена восстанавливает исходный локальный draft, selection
и undo с обычным `TextChanged`; поздний async ответ не принимается. Более новое
application value/draft имеет приоритет перед восстановлением. Отмена не
является транзакционным rollback внешней модели при параллельных изменениях.

Проверки включают точную арифметику и крайние коэффициенты, локализованные
форматы, nullable/bounds/step, typed bindings, синхронную и асинхронную валидацию,
отмену и lifecycle popup. Unit-события не являются проверкой нативного ввода
или экранного диктора.
