# Формы и элементы управления на XAML

`rust-desktop-ui-xaml` компилирует собственный ограниченный профиль XAML в Rust.
Профиль описывает нативное окно `Window`, вложенные `StackPanel`, подписи
`TextBlock`, однострочные `TextBox`, кнопки `Button`, переключатели `ToggleButton`,
флажки `CheckBox`, переключатели группы `RadioButton`, векторные `Icon`
и разделители `Separator`, `DropDownButton` и `SplitButton` со статическими меню,
а также командный `Ribbon` с вкладками, группами и панелью быстрого доступа
и дерево панелей `DockWorkspace`.
Пример [xaml-form](../../examples/xaml-form/MainWindow.xaml) подключает это
описание к существующим retained controls, платформенному host и GPU renderer.

## Запуск примера

```text
cargo run -p xaml-form --locked
cargo run -p xaml-form --locked -- --dark
cargo run -p xaml-form --locked -- --light --smoke-test
cargo run -p xaml-form --locked -- --controls
```

По умолчанию открывается [MainWindow.xaml](../../examples/xaml-form/MainWindow.xaml)
с Ribbon и редактором. `--controls` открывает
[ControlsWindow.xaml](../../examples/xaml-form/ControlsWindow.xaml), отдельную форму
с базовыми элементами и popup-меню. Обе разметки компилируются при сборке.

`--light` и `--dark` переопределяют тему из разметки. `--smoke-test` запускает
проверку кадров и изменения размера с автоматическим завершением.
Дополнительный `--diagnostics` выводит разрешение имени редактора и его значения
при изменениях в stdout, добавляет счётчики renderer к системному имени окна.
В режиме Native они видны в стандартном заголовке; название внутри Ribbon
остаётся без счётчиков. Без этого флага
введённый текст не журналируется, заголовок соответствует `Window.Title`.

## Заголовок окна и Ribbon

`Window TitleBar="Ribbon" Padding="0"` включает объединённую шапку на Windows.
`Native` — значение по умолчанию. Режим Ribbon требует либо Ribbon в корне,
либо вертикальную StackPanel с Ribbon первым ребёнком. У этой панели и Ribbon
должны отсутствовать фиксированные Width/Height и внешние отступы; Padding
панели также равен нулю. Spacing после Ribbon и отступы остальных элементов
остаются свободными. Title в этом режиме ограничен 4096 байтами UTF-8.
Нарушения обнаруживаются при компиляции; ручные определения проходят
`validate_window_chrome` до создания контролов.

Название и QAT располагаются в одной строке. Справа зарезервирована область
системных кнопок. При уменьшении ширины QAT переходит в overflow, название
сокращается по графемам; полное имя остаётся доступным. Меню, KeyTips,
минимизация Ribbon и состояние редактора сохраняют свои контракты.
Пустая QAT в обычном Ribbon больше не оставляет пустую строку.

Пример MainWindow включает этот режим и минимальный клиентский размер 240×240.
`--native-title-bar` явно возвращает стандартный заголовок и снимает этот
дополнительный минимум примера. На Linux/macOS или при недоступном DWM host
сохраняет обычную рамку без второго заголовка в Ribbon. Системные Mica/Acrylic
не включаются. Платформенный контракт — в [описании host](platform.md),
архитектурная граница — [ADR-006](adr/ADR-006-native-ribbon-title-bar.md).

## Разметка

Следующий пример показывает форму базовых элементов (`--controls`). Профиль
Ribbon описан отдельно [ниже](#ribbon-в-xaml).

```xml
<Window xmlns="urn:rust-ui-engine:ui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Title="Форма · Rust UI"
        Width="640" Height="512"
        Padding="32" Theme="Light">
    <StackPanel x:Name="Form" Spacing="8">
        <TextBlock Text="Форма с несколькими элементами" Height="32" />
        <TextBlock Text="Текст сообщения" />
        <TextBox x:Name="Message"
                 Height="36"
                 FontSize="14" Padding="12,5" CornerRadius="4" BorderThickness="1"
                 IsReadOnly="False" MaxLength="64"
                 IsRequired="True" CommitOnLostFocus="True"
                 ValidationMessage="Введите текст перед подтверждением."
                 AutomationName="Текст"
                 PlaceholderText="Введите текст…" />
        <CheckBox x:Name="Option" Content="Только чтение" FontSize="14" Padding="8,5" CornerRadius="4" BorderThickness="1" />
        <StackPanel Orientation="Horizontal" Spacing="8" Height="36">
            <Button x:Name="Confirm" Content="Подтвердить" Command="Confirm" Icon="Check" Width="160"
                    FontSize="14" Padding="8,5" CornerRadius="4" BorderThickness="1" />
            <ToggleButton x:Name="Pin" Content="Закрепить" Command="Pin" Icon="Pin" Width="120"
                          FontSize="14" Padding="8,5" CornerRadius="4" BorderThickness="1" />
            <Button x:Name="Disabled" Content="Недоступно" Width="112" IsEnabled="False" />
        </StackPanel>
        <StackPanel Orientation="Horizontal" Spacing="8" Height="32">
            <RadioButton x:Name="AlignLeft" Content="Слева" GroupName="Alignment" IsChecked="True" Width="120" />
            <RadioButton x:Name="AlignCenter" Content="По центру" GroupName="Alignment" Width="120" />
            <RadioButton x:Name="AlignRight" Content="Справа" GroupName="Alignment" IsEnabled="False" Width="120" />
        </StackPanel>
        <StackPanel Orientation="Horizontal" Spacing="8" Height="32">
            <Icon x:Name="CheckIcon" Symbol="Check" AutomationName="Готово" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="CloseIcon" Symbol="Close" AutomationName="Закрыть" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="AddIcon" Symbol="Add" AutomationName="Добавить" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="RemoveIcon" Symbol="Remove" AutomationName="Убрать" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="ChevronDownIcon" Symbol="ChevronDown" AutomationName="Вниз" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="ChevronRightIcon" Symbol="ChevronRight" AutomationName="Вправо" Width="24" Height="24" Margin="0,4" />
            <Icon x:Name="SaveIcon" Symbol="Save" AutomationName="Сохранить" Width="24" Height="24" Margin="0,4" />
            <Separator x:Name="VRule" Orientation="Vertical" />
            <Icon x:Name="DisabledIcon" Symbol="Pin" AutomationName="Закреплено" IsEnabled="False" Width="24" Height="24" Margin="0,4" />
        </StackPanel>
        <Separator x:Name="HRule" />
        <TextBlock x:Name="Help" Height="40"
                   Text="Enter — подтвердить · Escape — отменить изменения" />
        <TextBlock x:Name="Status" Height="56" Text="Изменений нет." />
        <StackPanel Orientation="Horizontal" Spacing="8" Height="36">
            <DropDownButton x:Name="ActionMenu" Content="Действия" Width="160"
                            ToolTip="Открыть меню действий">
                <MenuItem Header="Режим">
                    <MenuItem Header="Закрепить" Command="Pin" />
                </MenuItem>
                <Separator />
                <MenuItem Header="Недоступно" Command="Confirm" IsEnabled="False" />
            </DropDownButton>
            <SplitButton x:Name="AcceptSplit" Content="Подтвердить" Command="Confirm" Width="240"
                         Icon="Check" ToolTip="Подтвердить или выбрать действие">
                <MenuItem Header="Режим">
                    <MenuItem Header="Закрепить" Command="Pin" />
                </MenuItem>
                <Separator />
                <MenuItem Header="Недоступно" Command="Confirm" IsEnabled="False" />
            </SplitButton>
        </StackPanel>
    </StackPanel>
</Window>
```

Элементы принадлежат `urn:rust-ui-engine:ui`. Директива `x:Name` принадлежит
`http://schemas.microsoft.com/winfx/2006/xaml`. Префиксы произвольны: проверяется
URI пространства имён. Пространство имён WPF presentation не поддерживается.

`Window` содержит ровно один корневой элемент. Для нескольких контролов используйте
`StackPanel`: он допускает вложенные панели и произвольное число поддерживаемых
элементов. Старый вариант `Window → TextBox` также принимается. Пустая панель
допустима; листовые элементы не принимают дочерние элементы или текстовое содержимое.
Текст задаётся атрибутами `Text` или `Content`.

| Элемент | Дополнительные свойства | Назначение |
|---|---|---|
| StackPanel | `Orientation="Vertical"`, `Spacing="0"`, `Padding="0"`, `Surface="Transparent"` | Последовательная компоновка; также доступны `Horizontal` и поверхность `Card` |
| Ribbon | необязательный `SelectedTab`, модель вкладок/групп/команд | Адаптивная командная лента; не больше одной на окно |
| TextBlock | `Text=""`, `TextTone="Primary"`, `TextWrapping="Wrap"`, необязательные `FontSize`, `FontWeight`, `Foreground` | Видимая подпись с настройкой типографики |
| Button | `Content=""`, необязательный `Command` | Кнопка, которую приложение связывает с командой Rust |
| ToggleButton | `Content=""`, `IsChecked="False"`, необязательный `Command` | Кнопка с выбранным состоянием |
| DropDownButton | `Content=""`, вложенные пункты меню | Открытие меню; свойство Command запрещено |
| SplitButton | `Content=""`, обязательный `Command`, вложенные пункты меню | Основное действие и отдельная кнопка открытия меню |
| CheckBox | `Content=""`, `IsChecked="False"` | Двухсостоянийный флажок |
| RadioButton | `Content=""`, `IsChecked="False"`, необязательный `GroupName` | Взаимоисключающий выбор в группе |
| Icon | обязательный `Symbol`, необязательный `Foreground` | Самостоятельная векторная пиктограмма |
| Separator | `Orientation="Horizontal"`, `Thickness="1"`, необязательный `Foreground` | Неинтерактивная линия |

Для каждого элемента содержимого доступны `x:Name`, `Width`, `Height`, `Margin`,
`IsEnabled`, `AutomationName` и буквальный `ToolTip`. Имена уникальны во всём дереве окна.
Отключённая панель блокирует ввод и фокус своих потомков, сохраняя их собственные
настройки `IsEnabled`. После включения панели локально отключённый контрол остаётся
отключённым. Вкладочный порядок соответствует порядку интерактивных элементов
в дереве; панели и подписи не получают Tab-фокус.

`Command` — буквальный идентификатор `[A-Za-z_][A-Za-z0-9_]*`, а не выражение
binding. Его регистрацией и обработчиком управляет Rust-код приложения.
Конструкция `Click="..."`, исполняемый код в разметке и markup extensions
не поддерживаются. Подробности примера и команды приведены ниже.

У `ToggleButton` без команды начальное состояние задаёт `IsChecked`.
Для командного переключателя приложение заранее регистрирует команду и её
`checked: Some(bool)`, например через `register_toggle("Pin", "Закрепить", false)`:
это общее состояние имеет приоритет над `IsChecked`
в разметке. Неизвестная команда или отсутствие её checked state отклоняются
до монтажа дерева. `CheckBox` в этом профиле не принимает `Command`.

После монтажа связанный `Button` также отображает checked state команды:
вид контрола определяет оформление, а зарегистрированная команда — поведение
активации. Pointer, Space/Enter и accessibility Click используют одну активацию,
проверяют доступность команды и меняют её состояние один раз. Все связанные
представления получают результат. `set_checked(command, None)` снимает toggle-режим
у уже смонтированных представлений; возврат `Some(bool)` включает его снова.
Нативная проекция таких кнопок использует AccessKit Button с опциональным Toggled.

`RadioButton` не принимает `Command`. Его `GroupName`, если задан, должен
соответствовать `[A-Za-z_][A-Za-z0-9_]*`; пустое значение запрещено. Именованная
группа объединяет RadioButton во всём одном монтаже дерева, включая разные
StackPanel. Без GroupName объединяются только RadioButton с одним непосредственным
родителем. Именованная и безымянная группы не смешиваются; регистр имени значим.
Повторный монтаж той же разметки получает независимые группы, даже в одном Controls.

В каждой группе допустим максимум один начальный `IsChecked="True"`, включая
отключённые элементы. Неоднозначный выбор отклоняется компилятором; адаптер также
проверяет вручную созданный AST до изменения retained tree. Все начальные значения
False допустимы. Активация выбирает элемент и снимает выбор с прежнего участника;
повторная активация выбранного элемента не снимает выбор. Tab останавливается на
одном доступном участнике группы. Стрелки Left/Up и Right/Down выбирают предыдущего
и следующего доступного участника с обходом по кругу, пропуская disabled и hidden.
Space, Enter, pointer и accessibility Click используют тот же выбор. Нативная
проекция передаёт роль RadioButton и checked-состояние; это не подтверждение
проверки с реальным screen reader.

У Button, ToggleButton, CheckBox и RadioButton доступны `FontSize`, `Padding`, `CornerRadius`,
`BorderThickness`, `Background`, `Foreground`, `BorderBrush` и `FocusBrush`.
Числа и цвета имеют тот же синтаксис и пределы, что у TextBox ниже.
Они компилируются в `ButtonStyleDefinition`, а адаптер применяет `ButtonStyle`.
Незаданные значения следуют Fluent-теме. У CheckBox фон, граница и скругление
относятся к компактному индикатору; строка подписи остаётся прозрачной.
RadioButton также имеет прозрачную строку и круглый индикатор с точкой выбора.
Его индикатор сохраняет круглую форму при любом CornerRadius; это свойство
задаёт скругление кольца фокуса всей строки.
Настройка отдельных состояний доступна в Rust через `ButtonStyle`;
XAML styles, triggers и templates пока не поддерживаются.

`Icon.Symbol` принимает только `Check`, `Close`, `Add`, `Remove`, `ChevronDown`,
`ChevronRight`, `Save`, `Pin`, `FolderOpen`, `Copy`, `Edit`, `Eraser`, `Sun`,
`Moon` или `Lock`. Это собственные векторные символы в координатах
16×16; пути файлов, SVG, шрифтовые имена и внешние ресурсы не загружаются. Разметка
без Symbol отклоняется. По умолчанию Icon занимает 16×16 logical pixels; явные
Width/Height задают область, внутри которой символ равномерно масштабируется
и центрируется. Foreground наследует цвет текста темы, а отключённый Icon получает
приглушённый цвет темы. AutomationName задаёт смысл самостоятельной пиктограммы
для accessibility и не рисуется как подпись.

`Separator` принимает Horizontal/Vertical. Thickness должна быть конечной,
положительной и не больше 16384 logical pixels. Foreground наследует цвет границы
темы. Линия центрируется внутри своих границ и обрезается ими. В вертикальном
StackPanel горизонтальный разделитель без размеров растягивается по ширине,
его высота равна Thickness. В горизонтальном StackPanel вертикальный разделитель
растягивается по высоте, его ширина равна Thickness. В противоположном направлении
длина по умолчанию равна 16. Явные Width/Height имеют приоритет. Icon и Separator
не участвуют в Tab, pointer hit testing или действиях accessibility. В нативном
дереве они представлены как Image и Splitter соответственно; это не интерактивный
разделитель с изменением размеров.

Button и ToggleButton дополнительно принимают `Icon`, `IconSize="16"` и
`IconSpacing="8"`. Имена символов те же, что у Icon. IconSize должна быть конечной
и принадлежать `(0, 512]`, IconSpacing — `[0, 16384]`. Метрики без Icon отклоняются.
Пиктограмма и подпись центрируются вместе; промежуток появляется только при
непустой подписи. Цвет пиктограммы следует текущему foreground кнопки, включая
hover, checked и disabled. CheckBox и RadioButton эти свойства не принимают.
Пиктограмма внутри кнопки не создаёт отдельный accessibility node.

Кнопка с Icon и пустой либо пробельной Content требует непустую AutomationName:
это имя её действия. В горизонтальной панели желаемая ширина учитывает размер
пиктограммы, подпись, промежуток и padding; минимальная ширина кнопки только с
пиктограммой — 36 вместо 80. Высота по умолчанию остаётся 36, вертикальная Auto
ширина растягивается по прежним правилам. Для крупного IconSize можно задать
Width/Height; иначе содержимое обрезается. Ручной AST проходит тот же preflight
метрик и доступного имени до монтажа; при отсутствии icon его числовые поля
icon_size/icon_spacing должны сохранять defaults 16/8.

Если размер не задан, адаптер примера растягивает элемент по ширине вертикальной
панели. В горизонтальной панели при монтаже он вычисляет желаемую ширину подписи и кнопки
по тексту, флажка — по тексту и индикатору; поле получает ширину 240. Минимальная
желаемая ширина кнопки — 80. Для кнопок и флажков измерение учитывает FontSize
и горизонтальный Padding. Высота по умолчанию: TextBox, Button, ToggleButton, DropDownButton и SplitButton — 36,
CheckBox и RadioButton — 32. TextBlock без Height получает измеренную высоту текста
с учётом доступной ширины, шрифта и переноса. Автоматический размер вложенной панели учитывает
её детей, отступы и интервалы. Явные Width/Height задают фиксированный размер.
Это ограниченная компоновка примера; полноценный WPF measure/arrange здесь не реализован.
StackPanel с `Surface="Transparent"` сохраняет фон родителя; `Surface="Card"`
добавляет фон, контур и скругление из темы. Surface не меняет Padding или Spacing
и не создаёт дополнительный интерактивный элемент.
Корень формы ограничен размером viewport. Ряд фиксированных кнопок в узком
окне обрезается; он не растягивает окно или соседнее поле и не переносится
автоматически на следующую строку.

| Элемент | Свойство | Значение по умолчанию и назначение |
|---|---|---|
| Window | `Title` | `Rust UI Form`; заголовок окна |
| Window | `Width`, `Height` | `640`, `360`; начальный размер клиентской области |
| Window | `Padding` | `32`; внутренние отступы |
| Window | `Theme` | `Light`; допускаются `Light` и `Dark` |
| Window | `TitleBar` | `Native`; `Ribbon` объединяет заголовок и QAT в поддерживаемом Windows host |
| TextBox | `x:Name` | Отсутствует; имя в области имён формы |
| TextBox | `Text` | Пустая строка; начальный текст |
| TextBox | `PlaceholderText` | Пустая строка; подсказка внутри пустого поля |
| TextBox | `AutomationName` | `Text`; доступное имя, не отдельная видимая подпись |
| TextBox | `Width` | Отсутствует; ширина зависит от ориентации контейнера, как описано выше |
| TextBox | `Height` | `36`; высота поля |
| TextBox | `Margin` | `0`; внешние отступы |
| TextBox | `IsEnabled` | `True`; также допускаются `False`, `true`, `false` |
| TextBox | `IsReadOnly` | `False`; разрешены `True`, `False`, `true`, `false` |
| TextBox | `MaxLength` | `0`; без ограничения количества графем. Положительное значение ограничивает число extended grapheme clusters |
| TextBox | `IsRequired` | `False`; при подтверждении требуется хотя бы один непробельный символ |
| TextBox | `ValidationMessage` | `A value is required.`; сообщение при нарушении `IsRequired`, до 4096 байт UTF-8 |
| TextBox | `CommitOnLostFocus` | `True`; подтверждать значение при потере фокуса. `False` оставляет явное подтверждение |
| TextBox | `FontSize` | Не задан; размер текста из текущей темы |
| TextBox | `Padding` | Не задан; внутренние отступы текста из текущей темы |
| TextBox | `CornerRadius` | Не задан; единый радиус скругления из текущей темы |
| TextBox | `BorderThickness` | Не задан; единая толщина рамки из текущей темы |
| TextBox | `Background`, `Foreground` | Не заданы; фон и цвет текста из текущей темы |
| TextBox | `BorderBrush`, `FocusBrush` | Не заданы; рамка и акцент фокуса из текущей темы |
| TextBox | `PlaceholderForeground`, `SelectionBrush` | Не заданы; цвет подсказки и фона выделения из текущей темы |
| TextBox | `ValidationBrush` | Не задан; цвет рамки и акцента ошибки из текущей темы |

Все размеры задаются в логических пикселях. Размеры должны быть конечными,
больше нуля и не больше `16384`; размеры редактора должны оставаться больше
нуля после преобразования в `f32`. Отступы допускают диапазон `[0, 16384]`.
`Padding` и `Margin` принимают одно число, два числа `horizontal,vertical`
либо четыре `left,top,right,bottom`. Отрицательные отступы и размер `Auto`
в этом профиле не поддерживаются; для растяжения `TextBox` в вертикальной панели
не задавайте `Width`.

`FontSize` принимает конечное число в `(0, 512]`, остающееся больше нуля в `f32`.
`CornerRadius` и `BorderThickness` принимают по одному конечному числу в
`[0, 16384]`; отдельные углы и толщины сторон в этом профиле не задаются.
Нулевой `BorderThickness` убирает рамку, сохраняя индикатор фокуса.
Радиус при отрисовке ограничивается половиной меньшей стороны поля, а большая
толщина рамки и отступы не создают отрицательную область текста.

Цвета принимают только `#RRGGBB` или `#AARRGGBB`, без пробелов; регистр hex-цифр
произвольный. Например, `SelectionBrush="#80336699"` задаёт полупрозрачный фон
выделения. Имена цветов, shorthand `#RGB`, brush-элементы, ресурсы и градиенты
не поддерживаются. В восьмизначной записи первые два разряда задают alpha;
сгенерированное определение хранит байты в порядке red/green/blue/alpha.
Цвета используют sRGB и straight alpha, как остальные примитивы Scene.

`x:Name` соответствует `[A-Za-z_][A-Za-z0-9_]*` и не может повторяться в дереве.
Пример разрешает имя любого элемента через
`Form::find_name`; генератор не создаёт поле Rust с таким именем. Текстовые
свойства поддерживают Unicode и стандартные XML character/entity references,
например `&amp;`, `&quot;`, `&#x1F44B;`.

`MaxLength` принимает десятичные цифры без знака и пробелов, от `0` до `1048576`.
Комбинируемый акцент и emoji с ZWJ считаются вместе со своим символом: `é👩‍💻`
занимает две графемы. Начальное `Text`, превышающее лимит после удаления
разделителей строки, вызывает ошибку компиляции у атрибута `Text`. Независимо
от `MaxLength` сохраняется предел одного документа в 1 MiB UTF-8; renderer
отдельно ограничивает суммарный текст и число глифов в кадре.

`IsRequired` и `CommitOnLostFocus` принимают `True`, `False`, `true`, `false`.
Пустое начальное `Text` разрешено и при `IsRequired="True"`: проверка выполняется
при подтверждении значения, а не при компиляции или открытии окна. Длина
`ValidationMessage` считается после декодирования XML entities; пустое или
пробельное сообщение использует стандартное `A value is required.` во время
проверки. Unicode-пробелы распознаются по `str::trim`, само значение не обрезается.

## Текст и карточки

`TextBlock` сохраняет обычное свойство `Text` и не принимает дочернюю разметку
или текст между открывающим и закрывающим тегами.

| Свойство | Значения и поведение |
|---|---|
| `FontSize` | Не задан: размер из темы. Явное значение — конечное число в `(0, 512]`, больше нуля после преобразования в `f32` |
| `FontWeight` | Не задан: обычное начертание. `Normal` = 400, `SemiBold` = 600, `Bold` = 700; также целое число от 1 до 1000 |
| `Foreground` | Не задан: цвет определяется TextTone и темой. Принимает `#RRGGBB` или `#AARRGGBB` |
| `TextTone` | `Primary` по умолчанию или `Secondary` для второстепенной подписи |
| `TextWrapping` | `Wrap` по умолчанию или `NoWrap` |

Явный Foreground имеет приоритет над TextTone. Без него основные и второстепенные
подписи выбирают цвет текущей Light/Dark темы. FontWeight использует регистр
имён из таблицы; числовая форма состоит из десятичных цифр без знака и пробелов.
Эти свойства относятся к TextBlock и не включают общую систему styles/templates.

В адаптере формы отсутствие Height включает измерение высоты подписи через
Controls. При Wrap изменение доступной ширины может менять число строк и
высоту. При NoWrap текст не переносится для подгонки к ширине. Явная Height
остаётся фиксированной: не поместившийся текст обрезается. Полный Text
сохраняется для accessibility. Для ручного AST `validate_text_block` проверяет
диапазоны FontSize/FontWeight до создания элементов.

Минимальная карточка использует цвета темы без явных кистей:

```xml
<Window xmlns="urn:rust-ui-engine:ui"
        Title="Документ" Width="640" Height="360"
        Padding="24" Theme="Light">
    <StackPanel Surface="Card" Padding="20" Spacing="8">
        <TextBlock Text="Новый документ" FontSize="24" FontWeight="SemiBold" />
        <TextBlock Text="Укажите название и сохраните изменения."
                   TextTone="Secondary" TextWrapping="Wrap" />
        <TextBox PlaceholderText="Название" />
    </StackPanel>
</Window>
```

Для тёмного оформления достаточно `Theme="Dark"`; фон карточки, контур и
цвета обеих подписей следуют теме. `Surface="Transparent"` убирает оформление
карточки, сохраняя её компоновку. Иные значения Surface отклоняются.

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
начальными свойствами и деревом содержимого. `compile(source)` возвращает этот Rust-код строкой;
`parse(source)` возвращает определение без генерации. `compile_file(input,
output)` читает UTF-8, проверяет разметку и записывает результат. Каталог output
должен существовать. Ошибка разметки содержит `путь:строка:столбец: сообщение`,
координаты начинаются с единицы; ошибки файлового ввода-вывода содержат путь и
сообщение. Строки экранируются как Rust-литералы и не исполняются.

При запуске примера XML не загружается: используется скомпилированное определение.
Рекурсивный монтаж и область имён находятся в
[view.rs](../../examples/xaml-form/src/view.rs), поведение примера — в
[form.rs](../../examples/xaml-form/src/form.rs). Библиотека компилятора не зависит
от controls, winit или wgpu.

Rust API описания формы изменён: вместо `WindowDefinition.edit_box` используется
`WindowDefinition.content: ElementDefinition`. Каждый узел содержит общие свойства
в `common` и конкретный вид в `kind: ElementKind`. Размеры, имя, доступное имя,
отступы и `IsEnabled` находятся в `CommonProperties`; `EditBoxDefinition` содержит
только свойства редактора. Пользователям прежних Rust struct literals нужно
обновить код. Совместимость существующей разметки `Window/TextBox` сохранена.

`MountedView` монтирует дерево и разрешает имена; неизвестный `Command` вызывает
ошибку до создания поддерева. Команды нужно заранее зарегистрировать в
`Controls::commands_mut()`. Кнопка без Command всё равно получает фокус и
публикует обычную активацию Controls. Форма может содержать несколько независимых
редакторов или обходиться без них; изменение размера не пересоздаёт контролы.

Прикладной `Form` регистрирует команду Confirm, подтверждающую TextBox с именем
Message. При использовании этой команды отсутствие такого редактора — ошибка
создания формы. CheckBox Option переключает read-only этого поля. Состояние
редактирования выводится в TextBlock Status, а TextBlock Help содержит подсказку.
Эти имена зарезервированы прикладным Form для указанных типов; общий MountedView
таких соглашений не вводит. При создании формы IsReadOnly у Message или IsChecked
у Option включает режим только чтения и отражается в состоянии флажка;
дальнейшие переключения Option управляют режимом поля.
Команда Pin зарегистрирована через `register_toggle` и связана с кнопкой
«Закрепить». Она демонстрирует общее состояние команды без изменения редактора
или закрепления окна; прикладное поведение можно подключить в Rust.
В Ribbon-примере `Clear` очищает текущий текст Message, если редактор не read-only,
не подтверждая пустое значение. `Light` и `Dark` переключают тему; `Disabled`
показывает недоступную команду. Эти команды зарегистрированы тем же Form и могут
использоваться обычными кнопками или Ribbon placements без отдельных обработчиков.
Всё визуальное дерево примера задано XAML: при старой разметке Window/TextBox
адаптер не добавляет подписи самостоятельно. Escape отменяет изменения активного
редактора; подтверждение корректного значения при уходе фокуса остаётся включённым.
При нажатии Confirm с некорректным текстом возможны две попытки проверки:
из-за ухода фокуса и из-за явной команды.

## Статические меню и подсказки

DropDownButton открывает меню. SplitButton содержит основную кнопку и отдельную
стрелку: основной Command не вызывается при открытии меню. Отключение этой
команды не отключает стрелку; IsEnabled=False у самого SplitButton отключает
обе части. SplitButton имеет два последовательных Tab stops. Его x:Name
обозначает стабильный контейнер; адаптер хранит идентификаторы primary/dropdown
в MountedView.splits. ButtonStyle применяется к обеим частям, Icon — к основной.
AutomationName переименовывает основную часть, ToolTip назначается обеим.

Внутри меню разрешены только MenuItem и Separator. MenuItem требует непустую
Header и либо Command, либо вложенное непустое подменю. Совмещать действие
и подменю запрещено. IsEnabled=False отключает пункт независимо от состояния
команды. Checked state определяется зарегистрированной командой; отдельного
IsChecked у MenuItem нет. Separator внутри меню не принимает свойств или детей,
не получает фокус и не выполняет действий. Обычный Separator в layout сохраняет
свои прежние свойства. MenuItem не является layout-контролом, не принимает
x:Name, размеры, стили или обработчики событий.

Компилятор проверяет синтаксис и границы дерева, а MountedView до создания
первого контрола проверяет регистрацию всех команд, включая глубоко вложенные
пункты и основной Command SplitButton. Ограничения меню: глубина до 8 от первого
уровня пунктов, не более 256 элементов, суммарный текст Header/Command до 1 MiB.
Пункты и разделители также учитываются в общем лимите 256 элементов и глубине
32 для окна. Эти проверки применяются и к вручную созданному AST.

ToolTip — строка до 4096 UTF-8 bytes, без ресурсов, bindings и markup extensions.
Пустое или пробельное значение убирает подсказку. Подсказка появляется после
задержки наведения, рисуется поверх содержимого и не получает фокус или hit test.
Уход указателя, ввод, потеря фокуса окна и открытие меню скрывают её. Пример
планирует timer через tooltip_deadline и обрабатывает tooltip_tick на Tick;
постоянный polling для подсказки не требуется.
В этом профиле визуальная подсказка работает у hit-testable владельца или его
предка. У самостоятельных TextBlock, Icon и Separator ToolTip передаётся как
accessibility metadata; эти неинтерактивные элементы не становятся hover target.

Accessibility передаёт HasPopup=Menu, expanded и действия Expand/Collapse
для открываемых меню, ToolTip — на владельце. Переключаемые пункты представлены как
MenuItemCheckBox с Toggled. Пункты меню остаются связанными
с общей командой. Программная проверка этой проекции не заменяет проверку
конкретным screen reader.

В форме `--controls` ряд «Действия»/SplitButton находится под Status, начиная с y=433;
прежние контролы сохраняют координаты. Оба меню показывают вложенный «Режим»
с общей командой Pin, разделитель и отключённое действие. Основная часть
SplitButton подтверждает Message той же командой Confirm.

При запуске с `--diagnostics` пример дополнительно пишет `popup-state ` с JSON:
menus, controls, focused, pin_checked, tooltip_bounds, tooltip_pending и hovered. Это состояния меню,
доступные действия и logical bounds; текст поля в этот snapshot не включён.
Остальные диагностические строки примера по-прежнему могут содержать ввод.
Windows-сценарий меню запускается отдельно:

```powershell
$env:WGPU_BACKEND = 'dx12'
python tests/windows_popup_smoke.py --exe target/release/xaml-form.exe --output-dir target/popup-dx12
python tests/windows_popup_smoke.py --exe target/release/xaml-form.exe --output-dir target/popup-dark-dx12 --dark
```

Для Vulkan задайте WGPU_BACKEND='vulkan' и другую output-dir. Наличие сценария
не означает успешного прохождения на конкретной ОС, GPU или с screen reader.

## Поведение поля

`TextBox` монтируется через `Controls::edit_box`. Темы разметки `Light` и `Dark`
соответствуют `Theme::FluentLight` и `Theme::FluentDark` в controls. Это собственное
оформление со скруглённым полем, подсказкой и состояниями фокуса/доступности.

Адаптер применяет поведение одним `EditorSettings` (`is_read_only`, `max_length`,
`placeholder`, `commit_on_focus_lost`), правила одним `EditorValidation` и
оформление одним `EditBoxStyle`. Метрики и цвета в
`EditBoxDefinition` хранятся как `Option`: отсутствие атрибута сохраняет выбор
текущей темы, в том числе после её переключения. Явные значения сохраняются при
смене темы; цвета состояний могут переопределять базовые цвета, как описано ниже.
Обновление settings/style не меняет текст и не создаёт `TextChanged`;
недопустимое значение отклоняется до применения всего набора.

XAML-цвета задают общие переопределения `EditBoxStyle.normal`; `FocusBrush`
переопределяет цвет акцента. Hover использует фон наведения из темы, read-only —
фон canvas, disabled — фон canvas и приглушённые цвета текста/подсказки из темы;
эти значения имеют приоритет над базовыми XAML-цветами. Затем применяются явные
цвета активного состояния, заданные в Rust API `EditBoxStyle`. Поле read-only
сохраняет свою палитру и показывает индикатор фокуса, если оно сфокусировано.
XAML `Style`, setters, triggers и control templates ещё не поддерживаются.
`ValidationBrush` задаёт border/accent в `EditBoxStyle.invalid`, который
накладывается на обычное состояние при наличии ошибки проверки. Ошибка также
показывается текстом под полем в примере; распознавание не зависит только от цвета.
Сведения о Rust-настройках и состояниях —
[контракт controls](controls.md).

Поле использует общий `TextDocument`: Unicode-ввод, выделение, перемещение
каретки, undo/redo и маршруты clipboard/IME. CR, LF, tab, NEL (`U+0085`),
LINE SEPARATOR (`U+2028`) и PARAGRAPH SEPARATOR (`U+2029`) удаляются из начального,
вставленного и программно заданного текста; Enter не добавляет строку.
Tab/Shift+Tab используют навигацию фокуса. При изменении размера сохраняются
документ и текущее редактирование. `AutomationName`, значение, геометрия и
доступные действия передаются через AccessKit.

`IsReadOnly="True"` сохраняет фокус, выделение, навигацию и копирование. Ввод,
paste, cut, undo/redo и accessibility `SetValue` не меняют текст; IME для такого
поля не активируется. AccessKit получает признак read-only и действие `Focus`,
без `SetValue`. `IsEnabled="False"` дополнительно исключает взаимодействие и
получение фокуса. Программный `Controls::set_text` работает для обоих состояний
и соблюдает ограничения длины.

`Controls::set_max_length(id, Some(limit))` задаёт предел графем; `None` снимает
этот предел. Значение Rust API `Some(0)` разрешает только пустой документ, тогда
как `MaxLength="0"` в XAML компилируется в `None`. Установка предела меньше
текущего текста возвращает ошибку и сохраняет прежний предел и документ.
Вставка сверх лимита отклоняется целиком, без усечения и без изменения выделения
или undo-истории. Проверка относится к итоговому документу после замены выделения.
Программный `set_text` сообщает ошибку, а пользовательский ввод, paste, IME commit
и accessibility `SetValue` просто оставляют прежнее значение.

После изменения текста `Controls::drain_text_changes()` возвращает события
`TextChanged { id, text }` и очищает очередь. Пример формы использует эти события
для диагностического `edit-value=...`; предварительное сравнение строк не нужно.
Preedit, перемещение каретки, повторная установка того же значения и отклонённая
вставка не создают `TextChanged`. Начальный текст при создании поля также не
считается изменением. Обычный ввод, undo/redo и программная замена создают событие,
если текст изменился. Это контракт изменения текста; валидация бизнес-значения,
подтверждение и отмена используют отдельный контракт ниже.

## Подтверждение, отмена и проверка значения

Редактор хранит текущий текст и последнее принятое значение отдельно. Начальное
`Text` служит принятой исходной строкой. Ввод меняет текущий текст и публикует
`TextChanged`; `accepted_value(id)` остаётся прежним до успешного подтверждения.
`is_edit_dirty(id)` показывает, отличаются ли строки. IME preedit в это сравнение
не входит, а IME commit завершает ввод текста и сам по себе не подтверждает значение.

Enter или `Controls::commit_edit(id)` проверяет текущий текст. Успех обновляет
принятое значение; ошибка сохраняет текст, выделение, историю и прежнее принятое
значение. `CommitOutcome` различает `Committed`, `Unchanged`, `Invalid(message)`
и `CompositionActive`: активная IME composition не подтверждается как значение.
`IsRequired` запрещает пустую или полностью пробельную строку, не изменяя её.
Следующее фактическое изменение текста снимает прежнюю ошибку до новой проверки.

При `CommitOnLostFocus="True"` уход фокуса также пытается подтвердить значение.
Ошибка остаётся видимой, фокус не захватывается обратно. При `False` уход фокуса
сохраняет текущий текст без подтверждения; Enter и программный `commit_edit`
остаются доступны. Read-only/disabled редакторы не подтверждаются и не отменяются
пользовательскими клавишами или уходом фокуса, но программные операции разрешены.

Escape или `Controls::cancel_edit(id)` восстанавливает принятое значение,
снимает ошибку и сбрасывает undo/redo: отменённый текст нельзя вернуть через Undo.
При активной IME composition первый Escape отменяет только preedit; следующий
Escape может отменить изменение значения. Enter во время composition не принимает
незавершённый ввод как значение.
Повторная отмена без изменений не создаёт событие. `Controls::set_text` изменяет
текущий текст, сохраняя принятое значение; загрузка и принятие значения приложением
описаны в [контракте controls](controls.md).

`Controls::drain_edit_events()` возвращает `Committed { id, value }`,
`Canceled { id }` и `ValidationFailed { id, message }`. Это отдельная очередь от
`TextChanged`; подтверждение неизменившегося допустимого значения не публикует
повторный `Committed`. Пример формы очищает обе очереди. Только с
`--diagnostics` он выводит `edit-value=...`, `edit-committed=...`,
`edit-invalid=...` и `edit-canceled=true`, а также `edit-input=...` с клавишами,
текстом, IME и состоянием фокуса для разбора порядка событий.

TextBlock Status показывает состояние текущего значения: изменено, подтверждено,
отменено либо сообщение проверки. Без Height подпись получает измеренную высоту;
явная Height ограничивает область и обрезает не поместившийся текст. Полный текст доступен через
accessibility. AccessKit получает иерархию всех элементов, устойчивые при resize
идентификаторы, checked/disabled/read-only и актуальные доступные действия.
AutomationName меняет доступное имя, сохраняя видимую подпись.
Для каждого редактора передаются required, `Invalid::True` и отдельный узел
полного сообщения ошибки со связью `error_message`. Status имеет live-status.
Это программный контракт, не утверждение о проверке конкретным экранным диктором.

## Ribbon в XAML

В пределах одного `Window` допускается один `Ribbon`. Он монтирует существующий
компонент [Ribbon](ribbon.md); разметка задаёт статическую модель команд,
а Rust-код регистрирует их подписи, доступность, checked state и обработчики.

```xml
<Window xmlns="urn:rust-ui-engine:ui"
        xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"
        Width="960" Height="640" Padding="24">
    <StackPanel Spacing="12">
        <Ribbon x:Name="MainRibbon" SelectedTab="Home" IsMinimized="False">
            <Ribbon.QuickAccessToolbar>
                <Button Command="Confirm" />
                <Button Command="Pin" />
            </Ribbon.QuickAccessToolbar>
            <RibbonTab Id="Home" Header="Главная" KeyTip="H">
                <RibbonGroup Id="Document" Header="Документ" CollapsePriority="1">
                    <Button Command="Confirm" KeyTip="C" Icon="Check" MinSize="Small"
                            ToolTip="Подтвердить значение" />
                    <ToggleButton Command="Pin" KeyTip="P" Size="Small" Icon="Pin" />
                    <SplitButton Command="Confirm" KeyTip="S">
                        <MenuItem Command="Pin" />
                    </SplitButton>
                    <DropDownButton Command="Confirm" KeyTip="M" Icon="Check">
                        <MenuItem Header="Документ">
                            <MenuItem Command="Confirm" />
                            <MenuSeparator />
                            <MenuItem Command="Pin" />
                        </MenuItem>
                    </DropDownButton>
                </RibbonGroup>
            </RibbonTab>
        </Ribbon>
        <TextBox x:Name="Message" Height="36" IsRequired="True" />
    </StackPanel>
</Window>
```

| Узел модели | Свойства и содержимое |
|---|---|
| Ribbon | Общие свойства элемента, необязательный `SelectedTab`, `IsMinimized="False"`; вкладки и не больше одного `Ribbon.QuickAccessToolbar` |
| Ribbon.QuickAccessToolbar | Только пустые `<Button Command="Id" />`, без повторов Command; атрибутов нет |
| RibbonTab | Обязательные `Id`, непустой `Header`, `KeyTip`; необязательный `Context`; минимум одна группа |
| RibbonGroup | Обязательные `Id`, непустой `Header`; `CollapsePriority="0"` в диапазоне 0–65535; минимум одна команда |
| Button / ToggleButton внутри группы | Обязательные `Command`, `KeyTip`; preferred `Size="Large"` или `Small`, `MinSize="IconOnly"`, необязательные `Icon`, `ToolTip` |
| SplitButton / DropDownButton внутри группы | Те же свойства размеров и оформления; непустое дерево меню |
| MenuItem внутри Ribbon-меню | Лист: только `Command`; подменю: непустой `Header` и дочерние пункты, без Command |
| MenuSeparator / Separator внутри Ribbon-меню | Разделитель без атрибутов и дочерних узлов |

`MinSize` допускает `Large`, `Small`, `IconOnly` и не может быть больше `Size`.
Предпочтительный `Size="IconOnly"` не принимается: IconOnly — минимальный
адаптивный вариант. При `Size="Small"` допустимы MinSize Small или IconOnly.
Ограничение относится к размещению, а не к общей команде: два размещения одной
Command могут иметь разные пределы. Отсутствие Icon сохраняет текстовый fallback.

Узлы внутри Ribbon — части модели, а не обычные элементы StackPanel. Они не
принимают `x:Name`, размеры, стили кнопок, `Content`, `IsEnabled` или `IsChecked`.
Только сам Ribbon входит в namescope формы. `Command`, `Id`, `Context` и
`SelectedTab` имеют синтаксис `[A-Za-z_][A-Za-z0-9_]*`. Id вкладок уникальны в
Ribbon, Id групп — внутри своей вкладки. Должна существовать обычная вкладка
без `Context`; `SelectedTab`, если задан, указывает на такую вкладку. Иначе
выбирается первая обычная вкладка. Contextual tabs скрыты, пока приложение
не передаст соответствующий context через `Ribbon::set_context`.

KeyTip состоит из ASCII-букв и цифр, регистр нормализуется. Вкладки имеют общий
набор KeyTips, команды — отдельный набор на каждую вкладку. В одном наборе
недопустимы повторы и префиксы: `A` и `AB` конфликтуют. KeyTip вкладки не может
начинаться с 1–9: эти цифры зарезервированы для первых девяти команд QAT.
Пустой KeyTip отклоняется. Изменение ширины не меняет назначенные клавиши.

До монтажа всего дерева адаптер проверяет регистрацию всех команд, включая
QAT, скрытые вкладки и secondary menu. Для `ToggleButton` требуется заранее
зарегистрированный `checked: Some(bool)`, например `register_toggle`; разметка
не преобразует обычную команду в переключатель. Command state общий для Ribbon,
QAT, меню и кнопок за пределами ленты. Кнопка `Button` также отражает состояние
зарегистрированной toggle-команды. Primary SplitButton выполняет команду,
стрелка только открывает меню. DropDownButton использует Command для подписи
и доступности; его активация открывает меню без выполнения или переключения
владельца. Пункты-листья выполняют свои команды. Лист Ribbon-меню не принимает
Header или IsEnabled: его подпись и состояние принадлежат registry.
Статическое подменю принимает Header и непустые дочерние пункты.
Прежний плоский синтаксис SplitButton с `<MenuItem Command="Id" />` сохраняется.
Самостоятельный DropDownButton вне Ribbon по-прежнему использует Content
и не принимает Command; его контракт описан в разделе обычных меню.

Свойства `Icon` и `ToolTip` описывают оформление команды в Ribbon/QAT.
`Icon` использует тот же набор собственных символов, что обычные кнопки;
ToolTip — до 4096 UTF-8 байт. Это свойства представления, а не ресурсные ссылки.
Подпись и состояние команды по-прежнему принадлежат registry.
Если команда встречается несколько раз, незаданные Icon/ToolTip наследуют
описание другого её размещения. Два явно разных значения одного свойства
для одной команды отклоняются адаптером до монтажа.

Вкладки и группы участвуют в общих ограничениях XAML на 256 элементов и
32 уровня, включая QAT property element, все MenuItem и разделители меню.
Глубина отдельного дерева меню ограничена восемью уровнями; верхние пункты
имеют глубину один. Размер исходного файла ограничен 1 MiB. Ручной Rust AST проходит проверку структуры и ссылок
до изменения retained tree. Это не runtime XAML loading и не data binding.
`RibbonMenuEntryDefinition` содержит варианты `Command(String)`,
`Submenu { header, items }` и `Separator`. Runtime дополнительно проверяет
фактически создаваемые collapsed/overflow меню: их обёртки также расходуют
лимиты восьми уровней, 256 пунктов и 1 MiB текста.

Ширина Ribbon берётся из фактического layout его контейнера, включая отступы
родителей. При сужении элементы уменьшаются Large → Small → IconOnly до MinSize;
первыми рассматриваются группы с большим CollapsePriority. Уменьшение
применяется только при реальном выигрыше измеренной ширины группы. Если места
всё ещё недостаточно, группы переходят в collapsed/overflow меню. Смена
ширины, темы или вкладки перестраивает внутренние placements Ribbon;
идентификаторы соседних элементов, document, accepted value и undo редактора
сохраняются. Автоматическая высота использует измеренные метрики Ribbon.
Явный Height задаёт границу его области и может обрезать содержимое.
Размеры команд зависят от представления: Large — значок 32 logical pixels
сверху и до двух строк подписи, Small — значок 16 слева, IconOnly — компактный
значок без подписи; Small/IconOnly укладываются колонками по три.
QAT также использует компактные значки. Без Icon видимый текст сохраняется.
Не поместившийся остаток QAT переносится в More. Длинная крупная подпись
сокращается визуально; полная строка сохраняется в registry и доступном имени.
ToolTip по умолчанию показывает её, но может быть заменён или отключён.
Вкладки используют подчёркивание активной, группы — общую скруглённую
поверхность и вертикальные разделители. Крупный SplitButton разделяет верхнюю
primary область и нижнюю область подписи со стрелкой. Без Icon подпись остаётся
в primary, снизу отображается стрелка. В Small/IconOnly области расположены
горизонтально. Точный план групп доступен через `Ribbon::presentations()`,
размеры размещений — через `item_presentations()`; helper `layout_groups`
даёт оценку по базовым размерам.

`IsMinimized="True"` первоначально оставляет QAT и вкладки без тела групп.
Активация вкладки временно раскрывает тело в обычном layout и изменяет
автоматическую высоту Ribbon; соседние controls смещаются, сохраняя состояние.
Это раскрытие внутри потока формы, без отдельного popup-окна. Последующие
переходы управляются `Ribbon::set_minimized`, `is_minimized` и `is_expanded`.
Ctrl+F1 этим профилем не назначен.

Форма направляет обычный input в `Ribbon::handle_input` до Controls, затем
обрабатывает активации вкладок через `Ribbon::process`. Alt/F10 включает
KeyTips — отдельные метки поверх контролов, без изменения геометрии и
доступных имён. KeyTip DropDown открывает меню, KeyTip Split выполняет primary
команду. Парные Text events клавиш KeyTips подавляются, включая событие после
выполнения последней команды; следующий обычный ввод остаётся доступен.
Escape закрывает один уровень KeyTips, завершение режима возвращает фокус
доступному прежнему контролу. Переход с редактора по-прежнему соблюдает его
`CommitOnLostFocus`. Потеря focus окна, указатель и недоступность Ribbon
отменяют режим. Tab, keyboard popup и accessibility используют общие Controls.
Явное accessibility action также отменяет KeyTips и подавление Text events,
сохраняя focus для запрошенного действия.

Этот профиль не содержит backstage, gallery items, embedded ComboBox,
runtime binding, drag customization, Ribbon styles/templates или полного
WPF Ribbon API. Цвета и плотность следуют собственным темам Controls.

## Панели DockWorkspace

`DockWorkspace` описывает начальную структуру панелей. `DockSplit` делит область
между двумя дочерними узлами, `DockGroup` объединяет вкладки, `DockPane` содержит
один обычный элемент формы. Содержимым панели может быть StackPanel с редакторами
и кнопками; Id панели не заменяет x:Name её элементов.

```xml
<DockWorkspace xmlns="urn:rust-ui-engine:ui"
               xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml">
    <DockSplit Orientation="Horizontal" Ratio="0.25">
        <DockGroup>
            <DockPane Id="Explorer" Header="Обозреватель" MinWidth="160">
                <TextBlock Text="Содержимое проекта" />
            </DockPane>
        </DockGroup>
        <DockGroup SelectedPane="Document">
            <DockPane Id="Document" Header="Документ" CanClose="False">
                <StackPanel Padding="16" Spacing="12">
                    <TextBlock Text="Текст документа" />
                    <TextBox x:Name="Message" />
                </StackPanel>
            </DockPane>
        </DockGroup>
    </DockSplit>
</DockWorkspace>
```

Этот фрагмент размещается внутри Window. В shell с `TitleBar="Ribbon"` первым
ребёнком вертикального StackPanel остаётся Ribbon, а рабочая область располагается
после него. Компиляция не меняет обычное поведение размеров StackPanel: оставшееся
место рабочей области назначает координатор docking-окон.
Текущий координатор примера принимает DockWorkspace в корне либо вертикальный
StackPanel с одной рабочей областью или с Ribbon первым ребёнком и рабочей
областью последним. Window Padding должен быть нулевым; shell и DockWorkspace
используют автоматические размеры, нулевые отступы и остальные общие свойства
по умолчанию, кроме x:Name. У shell допускается Spacing, но не Padding или Card.
Эти ограничения монтажа дополнительно проверяются до создания окон;
обычный компилятор сохраняет общие свойства DockWorkspace в AST.

| Элемент | Свойства и ограничения |
|---|---|
| DockWorkspace | Один корневой DockSplit или DockGroup; общие свойства элемента, включая x:Name |
| DockSplit | Orientation=Horizontal или Vertical, по умолчанию Horizontal; Ratio от 0.1 до 0.9 включительно, по умолчанию 0.5; ровно два DockSplit/DockGroup |
| DockGroup | Непустой список DockPane; SelectedPane указывает на собственную панель, по умолчанию выбрана первая |
| DockPane | Обязательный Id вида `[A-Za-z_][A-Za-z0-9_]*`, до 128 байт; Header по умолчанию равен Id; CanClose=True; MinWidth=120, MinHeight=80 |

Header должен быть непустым и занимать не более 4096 байт UTF-8. Минимальные
размеры задаются в логических пикселях, должны быть конечными и принадлежать
диапазону (0, 16384]. В одном Window разрешена одна рабочая область и до 64 панелей;
Id панелей уникальны. Имена x:Name общие для всего Window. DockSplit, DockGroup
и DockPane не принимают x:Name и общие свойства контролов. Все внутренние узлы,
содержимое панелей и меню учитываются в общих лимитах 256 элементов и 32 уровней.
Внутри DockPane нельзя размещать Ribbon или вложенный DockWorkspace.
Каждая панель содержит независимый набор controls: один RadioButton GroupName
не может использоваться в нескольких DockPane. Внутри одной панели именованные
группы сохраняют обычную семантику; неименованные группы ограничены родителем.

Rust-описание использует `DockWorkspaceDefinition`, `DockNodeDefinition` и
`DockPaneDefinition`. `validate_docking` проверяет отдельную рабочую область,
`validate_window_docking` — полное дерево Window с общими именами и лимитами.
Эти функции нужны также для определений, созданных вручную. Регистрация команд
и проверка индивидуальных настроек контролов остаются задачами адаптера.
`DockWorkspaceDefinition::panes()` перечисляет панели в порядке начального дерева.

DockWorkspace требует координатор нескольких окон: обычный адаптер одной формы
отклоняет его до создания контролов. XML описывает начальное docked-состояние;
перемещения, отдельные системные окна и восстановление layout выполняет runtime.
DockFloating, bindings и шаблоны панелей в этот профиль не входят.

## Границы профиля

Допускаются ровно один `Window` и один корневой элемент содержимого. Комментарии и
межэлементные пробелы разрешены. Неизвестные элементы, свойства и директивы,
повторяющиеся XML-атрибуты, непустой текст внутри элементов, processing
instructions и DTD приводят к ошибке. Внешние сущности не загружаются.
Лимит исходника — 1 MiB UTF-8; лимит XML-узлов — 4096, включая комментарии.
Дерево содержимого содержит не больше 256 элементов и не глубже 32 уровней:
корневой элемент под Window имеет глубину 1, само Window не входит в эти лимиты.

Bindings, markup extensions (включая литеральное экранирование `{}`), ресурсы,
стили, шаблоны и Grid ещё не реализованы. Значение
атрибута, начинающееся с `{` после начальных пробелов, отклоняется. Runtime XAML,
hot reload, WPF/DevExpress-совместимость и дизайнер Visual Studio не входят
в этот профиль.

Тесты компилятора проверяют диагностику, пространства имён, ограничения и
экранирование. Тесты примера сравнивают скомпилированное определение с исходником,
проверяют монтаж, resize, ввод и semantic actions. Эти проверки моделей не
заменяют отдельную проверку native IME, screen reader и GPU на каждой ОС.

Дополнительные проверки покрывают `IsReadOnly`, компиляцию и применение
`MaxLength`, combining/ZWJ graphemes, отказ при превышении лимита и доступные
действия read-only поля. Windows-сценарий также заполняет поле до 64 графем,
проверяет отказ при 65-й без события изменения и продолжение редактирования.

Сценарий валидации дополнительно проверяет отказ Required по Enter, отображение
ошибки и её цвета, подтверждение допустимого значения, отмену изменённого текста
по Escape и принятие при потере фокуса. Отказ проверки при blur не закрывает окно.

Тесты оформления проверяют диапазоны метрик, порядок ARGB/RGBA, диагностику
ошибочных цветов, генерацию optional-свойств, применение стиля к форме и
сохранение явных цветов при смене темы. Accessibility использует актуальную
подсказку из `EditorSettings` после программного обновления.

На Windows 2026-10-10 выполнена проверка светлой и тёмной формы на DX12 и Vulkan:
размер клиентской области из XAML, Unicode-ввод, Backspace, потеря/возврат фокуса,
resize, minimize/restore и закрытие. Снимки проверяют цвета, глифы, скругления и
подчёркивание фокуса при масштабе 150%. Повторяемый сценарий:

```text
cargo build --release --locked -p xaml-form
python tests/windows_xaml_smoke.py --exe target/release/xaml-form.exe --output-dir target/xaml-smoke
python tests/windows_xaml_smoke.py --exe target/release/xaml-form.exe --controls-only --output-dir target/xaml-controls
```

Для тёмной темы добавьте `--dark`; backend выбирается переменной `WGPU_BACKEND`
(`dx12` или `vulkan`). Сценарий использует Win32-сообщения только своему окну;
он не подтверждает работу настоящей IME, внешнего clipboard или screen reader.
Режим `--controls-only` проверяет Button/ToggleButton/CheckBox/RadioButton, их палитры,
нажатие и его отмену, Enter/Space, Tab с пропуском disabled, resize и закрытие.
Для RadioButton дополнительно проверяются исключительный выбор мышью и четырьмя
стрелками, круглый индикатор с точкой и независимый фокус, пропуск disabled,
сохранение выбранного состояния после Space/Enter и выход из группы по Tab.
Для Icon/Separator снимки проверяют восемь разных векторных силуэтов, сглаживание
диагоналей, muted-цвет отключённого символа и тонкие линии обеих ориентаций.
Клики по иконке и разделителям не активируют команды и не создают Tab stops.
Этот режим не вводит текст и не проверяет accepted value или отмену редактора.
2026-10-12 этот сценарий прошёл на NVIDIA RTX 3090 с DX12 и Vulkan в обеих
темах при DPI 144: по 30 проверок и 8 снимков на сочетание. Проверены пиксельные
палитры и контраст отметки, скругления, глифы, фокус и pointer/keyboard activation.
2026-10-13 расширенный сценарий RadioButton прошёл на том же аппаратном GPU:
DX12/Vulkan, Light/Dark, DPI 144 — по 53 проверки и 12 снимков; resize 640×440 →
480×400 logical pixels. Снимки обеих тем и уменьшенной формы просмотрены.
Нативный сценарий проверяет обычный Tab и возврат к первому контролу. Shift+Tab
проверяется на уровне модели ввода; posted Win32-сообщения не устанавливают
реальное состояние Shift, которое читает host.
Версия с Icon/Separator проверена 2026-10-13 на DX12/Vulkan в обеих темах,
DPI 144: по 59 проверок и 12 снимков; resize 640×488 → 480×448 logical pixels.
Последовательность синтетических KEYDOWN/CHAR не атомарна: если winit обработал
KEYDOWN до появления CHAR в очереди, текст может не попасть в событие ввода.
Такой тайм-аут сценария требует разбора журнала и не доказывает неисправность
обычного клавиатурного ввода. Автоматических повторов ввода в сценарии нет.
Перенос между мониторами с разным DPI и GPU-запуски Linux/macOS ещё не проверены.

Во время активного preedit Enter не принимает business value. Если ОС передаст
клавишу выбора IME-кандидата после события окончания composition, её порядок
должен быть проверен с настоящим input method: обычный KeyDown сам по себе
не сообщает, была ли эта клавиша уже использована IME.

2026-10-12 проверки версии ReadOnly/MaxLength/TextChanged повторены на DX12/Vulkan в обеих темах,
включая достижение MaxLength, отказ сверх лимита без события и продолжение ввода.

Отдельный GPU-тест прозрачных rounded borders использует production shader,
рисует в offscreen sRGB texture и проверяет прочитанные пиксели. Он исключён из
обычного `cargo test`, поскольку требует аппаратный GPU:

```text
cargo test -p rust-desktop-ui-render-wgpu --locked rounded_stroke_gpu -- --ignored --nocapture
cargo test -p rust-desktop-ui-render-wgpu --locked stroke_path_gpu_union -- --ignored --nocapture
```

Backend выбирается через `WGPU_BACKEND`. Такой тест подтверждает композицию
пикселей, но не заменяет оконные сценарии ввода и accessibility.
Тест StrokePath проверяет пересечения с одним alpha blend, круглые концы,
диагонали, clipping и painter order при масштабе 1/1.5/2. Оба теста прошли
2026-10-13 на DX12 и Vulkan с NVIDIA RTX 3090.

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
