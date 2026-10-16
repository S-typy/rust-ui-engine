# Редакторы, привязки и компоновка форм

Пример [EditorsWindow.xaml](../../examples/xaml-form/EditorsWindow.xaml) объединяет
списки, справочник, встроенные кнопки, числовые и календарные поля с наблюдаемой
моделью. Разметка компилируется в Rust при сборке приложения.

```text
cargo run --release --locked -p xaml-form -- --editors
cargo run --release --locked -p xaml-form -- --editors --dark
```

## Выбор из коллекции

`ListBox` поддерживает одиночный и множественный выбор, клавиатурную навигацию,
прокрутку и поиск по первым символам. `ComboBox` открывает список; `IsEditable`
разрешает ввод свободного текста. `LookupEdit` ищет по отображаемым колонкам,
но принимает только значение из справочника. Все три элемента используют
устойчивые ключи: изменение порядка коллекции не меняет выбранный объект.

```xml
<ComboBox ItemsSource="{Binding People}"
          DisplayMemberPath="Name" ValueMemberPath="Id"
          SelectedValue="{Binding Form.Person}" />
```

`ItemsSource` — привязанный список `DataValue`. Строковые элементы сами служат
ключом и подписью. Для объектов пути задают подпись и ключ; ключи должны быть
уникальными, а их значения — строками, числами или логическими значениями.
Модель получает исходное значение ключа, а не строковое представление или индекс.
`SelectedValues` используется с `SelectionMode="Multiple"`; `SelectedValue` —
с одиночным выбором. Потерявший элемент выбор очищается при обновлении коллекции.

Статический список содержит `ListBoxItem Value="..." Content="..."`.
`IsEnabled="False"` исключает пункт из пользовательского выбора.
Статические пункты и `ItemsSource` нельзя задавать одновременно.
В runtime одна коллекция ограничена 100000 пунктами и 16 МиБ суммарного текста.
Списки виртуализируют строки фиксированной высоты 32 логических пикселя;
произвольные шаблоны и переменная высота строк пока не поддерживаются.

```xml
<LookupEdit ItemsSource="{Binding People}" DisplayMemberPath="Name"
            ValueMemberPath="Id" SelectedValue="{Binding Form.Person}">
  <LookupEdit.Columns>
    <LookupColumn Field="Name" Header="Имя" Width="180" />
    <LookupColumn Field="Department" Header="Отдел" Width="160" />
  </LookupEdit.Columns>
</LookupEdit>
```

Первая колонка представляет `DisplayMemberPath`, следующие задают дополнительные
поля поиска. При заданном `DisplayMemberPath` её `Field` должен совпадать с этим
путём; иначе компиляция завершается ошибкой. Если путь не задан, используется
`Field` первой колонки. Список закрывается при Escape, потере владельца или фокуса окна;
закрытие без выбора сохраняет ранее принятое значение.

## Редакторы значений

| XAML | Значение и поведение |
|---|---|
| `ButtonEdit` | Обычный текст и от одной до восьми встроенных `EditorButton` с зарегистрированными командами |
| `SpinBox` | Конечное число, `Minimum`, `Maximum`, положительный `Increment`, кнопки изменения |
| `DateEdit` | Дата григорианского календаря, ручной ввод, всплывающий календарь, границы диапазона |
| `TimeEdit` | Время суток, ручной ввод и шаг кнопками; без часового пояса |

Общие параметры редактора — `IsReadOnly`, `MaxLength`, `IsRequired`,
`PlaceholderText`, `CommitOnLostFocus`, сообщение ошибки и оформление TextBox.
Enter подтверждает корректный черновик; Escape восстанавливает принятое
значение. По умолчанию корректный ввод подтверждается при уходе фокуса.
Некорректный ввод остаётся в поле с ошибкой и не удерживает фокус.

```xml
<SpinBox Value="{Binding Quantity}" Minimum="1" Maximum="1000" Increment="1" />
<DateEdit Value="{Binding Date}" Format="dd.MM.yyyy" />
<TimeEdit Value="{Binding Time}" Format="HH:mm" />
<ButtonEdit Text="{Binding Reference}">
  <ButtonEdit.Buttons>
    <EditorButton Content="Выбрать" Command="ChooseReference" Icon="ChevronDown" />
  </ButtonEdit.Buttons>
</ButtonEdit>
```

Дата по умолчанию отображается как `ДД.ММ.ГГГГ`, время — `ЧЧ:ММ`.
Также поддерживаются `yyyy-MM-dd` и `HH:mm:ss`. Значения `Value`, `Minimum`,
`Maximum` в разметке и модели используют ISO: `2026-10-15`, `09:30:00`.
Пустая дата или время представлены `DataValue::Null`; `IsRequired` запрещает
подтверждение пустого поля. Диапазон дат — годы 1–9999, включая проверку високосных
лет. В формате без секунд нельзя незаметно отбросить ненулевые секунды.
Пустой календарь открывается на текущей дате UTC, ограниченной допустимым
диапазоном; это не устанавливает значение поля. Шаг времени по умолчанию равен
минуте. Runtime допускает настройку шага через `TimeEditSettings::step_seconds`;
при скрытых секундах он должен быть кратен 60. В XAML отдельного свойства шага
времени пока нет.

## Наблюдаемая модель

`BindingContext` хранит дерево `DataValue`: объект, список, строку, конечное
число, логическое значение или null. Пути поддерживают вложенные поля и индексы:
`Customer.Name`, `Rows[0].Amount`. Изменения выполняются в UI-потоке; скрытого
цикла событий и фоновой диспетчеризации нет.

```rust
use rust_desktop_ui_controls::{BindingContext, DataValue};

let mut model = BindingContext::new(DataValue::object([
    ("Name", DataValue::from("Документ")),
    ("Quantity", DataValue::from(12.0)),
]))?;
model.set("Name", "Новый документ".into())?;
model.set_many([("Name", "Заказ".into()), ("Quantity", 24.0.into())])?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`set_many` атомарно проверяет итоговое дерево. `ValueSchema` описывает допустимые
типы, а `BindingValidator` проверяет модель целиком, включая зависимости полей.
Неуспешная проверка не меняет данные или ревизию. Поля не создаются неявно;
для изменения структуры заменяется соответствующий объект или список.
Копия callback валидатора разделяет его реализацию через `Rc`.

`revision` и `changes_since` позволяют нескольким представлениям независимо
наблюдать один контекст. История ограничена 256 пакетами; отсутствующая история
требует повторного чтения значений. Ограничения дерева — 100000 узлов, глубина
64, 4 МиБ текста. `ControlBindings` сопоставляет свойства с данными, не потребляя
очереди событий приложения.

## Контракты привязок

```xml
<Grid DataContext="{Binding Customer}">
  <TextBox Text="{Binding Name, Mode=TwoWay, UpdateSourceTrigger=LostFocus}" />
</Grid>
```

`DataContext` задаёт путь относительно унаследованного контекста. Замена объекта
по этому пути обновляет потомков. `ElementName` выбирает именованный элемент
той же смонтированной формы; `Path` тогда означает его поддерживаемое свойство.
Явные циклы таких зависимостей отклоняются.

| Mode | Направление |
|---|---|
| `OneWay` | Модель → свойство |
| `TwoWay` | Модель ↔ свойство |
| `OneTime` | Однократное начальное чтение модели |
| `OneWayToSource` | Свойство → модель, включая начальное значение |

Для `Text` редакторов и типизированного `Value` по умолчанию используется
`TwoWay` / `LostFocus`. Этот trigger публикует принятое значение, поэтому Enter
также обновляет модель. Выбор и `IsChecked` используют `TwoWay` /
`PropertyChanged`; отображаемый текст — `OneWay`. `PropertyChanged` у редактора
наблюдает черновик, исключая незавершённый IME preedit. `Explicit` ждёт
`ControlBindings::update_sources`.

Поддерживаемые свойства: `Text`, `Content`, `Value`, `SelectedValue`,
`SelectedValues`, `ItemsSource`, `IsChecked`, `IsEnabled`, `IsReadOnly`,
`Visibility`, `Width`, `Height`, `PlaceholderText`. В разметке доступность
привязки дополнительно проверяется для конкретного элемента. `Visibility`
принимает `Visible`, `Hidden`, `Collapsed`: Hidden сохраняет место в компоновке,
Collapsed убирает его до восстановления видимости. В привязке используются эти
строки; логические true/false соответствуют Visible/Hidden. Min/max размеров,
`AutomationName` и `ToolTip` пока задаются
литералами. Путь XAML содержит ASCII-идентификаторы и неотрицательные индексы;
произвольные выражения и обращения к полям через кавычки не разрешены.

Конвертеры регистрируются приложением в `BindingConverterRegistry`.
Для записи в модель нужен обратный конвертер. `Converter=Name` выбирает его
по имени; `FallbackValue` задаёт резервное значение при ошибке чтения/конверсии.
Ошибки доступны через `drain_diagnostics`; ошибки бизнес-проверки показываются
редактором до принятия значения. Изменение постороннего поля модели сохраняет
черновик, undo и IME-состояние. При одновременном изменении одного свойства
в модели и интерфейсе приоритет имеет новое значение модели.

Для общего ItemsSource можно зарегистрировать наблюдаемую коллекцию через
`BindingContext::register_collection`. ListBox, ComboBox и LookupEdit принимают
изменения состава и полей при обычном `sync`, сохраняя выбор по ключу.
Привязки используют журнал изменений и не перечитывают неизменённые списки.
Контракт и ограничения проекции — [наблюдаемые коллекции](observable-collections.md).

## Компоновка

| Контейнер | Семантика |
|---|---|
| `StackPanel` | Последовательная вертикальная или горизонтальная компоновка |
| `Grid` | Строки и колонки `Auto`, пиксели, `*`, `2*`; размещение и spans |
| `WrapPanel` | Перенос элементов на следующую строку или колонку |
| `DockPanel` | Последовательное резервирование краёв, `LastChildFill=True` по умолчанию |
| `Border` | Один ребёнок, padding, фон, граница и скругление |
| `GroupBox` | Заголовок и одно содержимое; Card, Outline, Plain, Standard; необязательное сворачивание |
| `Expander` | Сворачиваемая группа; поддерево и привязки сохраняются |
| `TabbedGroup` | Несколько GroupTab с сохранением неактивных страниц; SelectedIndex |
| `ScrollViewer` | Один ребёнок, две оси, отсечение и полосы прокрутки |

```xml
<Grid Spacing="12">
  <Grid.RowDefinitions>
    <RowDefinition Height="Auto"/><RowDefinition Height="*"/>
  </Grid.RowDefinitions>
  <Grid.ColumnDefinitions>
    <ColumnDefinition Width="160"/><ColumnDefinition Width="*"/>
  </Grid.ColumnDefinitions>
  <TextBlock Text="Название" VerticalAlignment="Center" />
  <TextBox Grid.Column="1" Text="{Binding Name}" />
  <ScrollViewer Grid.Row="1" Grid.ColumnSpan="2" HorizontalScrollBarVisibility="Disabled">
    <StackPanel><TextBlock Text="Содержимое" /></StackPanel>
  </ScrollViewer>
</Grid>
```

Индексы Grid начинаются с нуля, spans положительные. Ось без явных tracks
растягивает единственную ячейку. Общие свойства: `Width`, `Height`,
`MinWidth`, `MinHeight`, `MaxWidth`, `MaxHeight`, `Margin`, `Visibility`,
`HorizontalAlignment`, `VerticalAlignment`. Приоритет явного размера сохраняется.
Выравнивание принимает `Left`/`Right` или `Top`/`Bottom`, `Center`, `Stretch`.
`Width="Auto"` и `Height="Auto"` используют правила автоматического размера
контрола. Явные размеры положительны, min/max могут быть нулевыми; значения
ограничены 16384 логическими пикселями. В XAML отступы неотрицательны.
Корневые Grid, Border, DockPanel и ScrollViewer с автоматической высотой и
`VerticalAlignment="Stretch"` занимают доступную высоту формы.

`DockPanel.Dock` принимает `Left`, `Top`, `Right`, `Bottom`; последняя область
по умолчанию занимает остаток. Разрешено до 48 детей одного DockPanel;
вложенные служебные контейнеры учитываются в пределе глубины layout.
`DockPanel` не является docking workspace.
Группы и их `Header`/`IsExpanded`/`SelectedIndex` bindings описаны в [отдельном контракте](groups.md).
Для перемещаемых панелей и отдельных окон используется [DockWorkspace](docking.md).

ScrollViewer использует `Auto`, `Visible`, `Hidden`, `Disabled` отдельно для
каждой оси. `Hidden` скрывает полосу, сохраняя прокрутку; `Disabled` отключает
прокрутку и ограничивает содержимое размером viewport по этой оси. Полосы
расположены поверх содержимого. Поддерживаются wheel, клавиши, track и thumb.
Обе оси по умолчанию используют `Auto` в XAML; задайте `Disabled` явно для
растягивания содержимого по ширине окна. Скругление Border относится к его
фону и рамке: отсечение дочернего содержимого остаётся прямоугольным.

Профиль форм поддерживает только перечисленные элементы и свойства. Общие
шаблоны, динамическая регистрация свойств, MultiBinding, expressions,
async validation, коллекционные шаблоны и произвольная
локаль форматирования не поддерживаются. Геометрия и состояние проверяются
unit/integration-тестами; это не заменяет проверки native IME и screen reader.

В Rust `EditorControl` раскрывает идентификаторы контейнера, текстового поля и
кнопок для настройки. Удаление всего редактора выполняется через `container`.
Отдельное удаление обязательного `editor` или `ListBox.content` отклоняется
до изменения дерева; необязательную встроенную кнопку можно удалить отдельно.
