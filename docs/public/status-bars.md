# Строки состояния

`StatusBar` и `RibbonStatusBar` размещают сведения и команды в левой и правой
частях строки. Оба используют постоянные дочерние контролы, общую систему
команд и темы Light/Dark. Ribbon-вариант имеет высоту 36 вместо 32 логических
пикселей и поверхность, согласованную с лентой. Он работает и без `Ribbon`.

Пример с обоими вариантами, прогрессом и привязками:

```text
cargo run --release --locked -p xaml-form -- --status
cargo run --release --locked -p xaml-form -- --status --dark
```

Основной пример также содержит `RibbonStatusBar` под документом. Его команда
«Закрепить» использует то же состояние, что кнопки Ribbon, QAT и формы.
Регулятор масштаба меняет размер текста редактора, сохраняя документ и черновик.

## XAML и размещение

```xml
<Grid>
  <Grid.RowDefinitions>
    <RowDefinition Height="*"/>
    <RowDefinition Height="Auto"/>
  </Grid.RowDefinitions>
  <TextBox Text="Документ"/>
  <StatusBar Grid.Row="1" AutomationName="Состояние документа">
    <StatusBarItem>
      <TextBlock Text="{Binding Message}" FontSize="12"/>
    </StatusBarItem>
    <StatusBarItem Placement="Right" Width="120">
      <ProgressBar Value="{Binding Progress}" AutomationName="Выполнение"/>
    </StatusBarItem>
    <StatusBarItem Placement="Right" Width="224">
      <ZoomControl Value="{Binding Zoom}" AutomationName="Масштаб"/>
    </StatusBarItem>
  </StatusBar>
</Grid>
```

Для варианта Ribbon достаточно заменить имя внешнего элемента на
`RibbonStatusBar`. Каждый прямой ребёнок панели — `StatusBarItem` с одним
необязательным обычным элементом формы. Внутри можно разместить StackPanel,
TextBlock, Icon, Separator, Button, ToggleButton, меню или редактор.
`StatusBarItem` вне панели и вложенные строки состояния отклоняются.
Обычные общие свойства, имена, DataContext и привязки сохраняются.

`Placement="Left"` по умолчанию; `Right` закрепляет секцию справа. Порядок
сохраняется отдельно внутри каждой стороны. Левые секции без Width делят
свободное пространство; правые получают явно заданную или измеренную ширину.
При нехватке места сначала сокращается левая область. Содержимое обрезается
внутри своей области и не перекрывает соседние секции. Это совместимый режим
непосредственных дочерних контролов. Для компактных кнопок задавайте Height 24–26
и небольшие Padding. Управляемые секции с переполнением описаны ниже.

Панель занимает назначенное родителем место: закрепление у нижнего края задаёт
Grid со строками `*,Auto` или DockPanel. С объединённым заголовком окна допустим
корневой Grid `Auto,*,Auto`: первая строка только для Ribbon, последняя для
статуса. Ограничения заголовка описаны в [XAML](xaml.md#заголовок-окна-и-ribbon).

## Прогресс и масштаб

| Элемент | Свойства и значения по умолчанию | Привязка Value |
|---|---|---|
| ProgressBar | Minimum=0, Maximum=100, Value=0 | OneWay; также OneTime |
| ZoomControl | Minimum=25, Maximum=400, Value=100, Step=10 | TwoWay, PropertyChanged |

Числа и разность Maximum−Minimum должны быть конечными; Minimum меньше Maximum,
Value входит в диапазон, Step положителен. Некорректная программная установка
отклоняется без изменения текущего значения. Ошибка данных привязки доступна
в диагностике bindings. При связанном Value начальная заглушка XAML ограничивается
диапазоном до применения данных. Настройки диапазона задаются при создании.

ProgressBar показывает определённый процент выполнения и не получает фокус.
`IsIndeterminate=True` включает неопределённый прогресс; `BusyIndicator`
использует этот режим по умолчанию. Сохранённое Value не меняется, но числовое
значение не публикуется в accessibility, пока продолжительность неизвестна.
Rust API: `set_progress_indeterminate`, `progress_indeterminate`, `busy_indicator`.

`Controls::set_reduced_motion(true)` включает общую настройку окна: busy-сегмент
становится неподвижным, а раскрытие групп завершается сразу. XAML
`ReducedMotion=True` на индикаторе включает эту настройку для его Controls host.
Скрытые и неактивные окна не планируют анимацию прогресса. Host использует
`Controls::next_deadline()` и `tick(Instant)` для progress, раскрытия групп,
повторяющихся кнопок, ToolTip и независимого таймера caret.
Для ZoomControl доступен Explicit trigger; LostFocus следует общему контракту
не-текстовых свойств и публикует значение сразу, как PropertyChanged.
ZoomControl — один Tab stop с кнопками минус/плюс, треком, ползунком и процентом.
Стрелки меняют значение на Step, Home/End выбирают границы. Перетаскивание
изменяет значение непрерывно, шаг относится к кнопкам и клавишам. Пользовательское
изменение ограничивается диапазоном. Потеря захвата, blur, скрытие, отключение,
удаление или обновление модели отменяют текущий жест; поздний release не
перезаписывает новое значение. Сам ZoomControl не масштабирует другие контролы:
приложение использует Value для своей области документа.

Для Rust доступны `Controls::status_bar` и `ribbon_status_bar`, возвращающие
`StatusBar { container, left, right }`. Дочерние элементы добавляются в нужный
host обычными методами Controls. Hosts принадлежат контейнеру; отдельно удалять
их нельзя. `progress_bar` и `zoom_control` возвращают WidgetId и принимают
`ProgressBarSettings` и `ZoomControlSettings`. Методы `progress_value`,
`set_progress_value`, `zoom_value`, `set_zoom_value` читают и меняют значение.
`progress_settings` и `zoom_settings` возвращают диапазон и текущие настройки.

## Доступность и пределы

Controls передаёт роли StatusBar, ProgressBar и Slider. Пример проецирует их в
AccessKit Status, ProgressIndicator и Slider, включая числовой диапазон, шаг,
значение и SetValue для доступного регулятора. Текст `Status` основного примера
имеет отдельное вежливое объявление обновлений; весь контейнер не объявляется
при каждом изменении масштаба. Декорации и прогресс не создают Tab stops.

Общий механизм live-region в XAML не входит в этот контракт. Настоящие системные
клики, screen reader и IME проверяются отдельно от unit-тестов и отрисовки окна.

## Управляемые секции, переполнение и настройка

```xml
<StatusBar IsCustomizable="True" ShowResizeGrip="True">
  <StatusBarItem Key="message" Label="Сообщение">
    <TextBlock Text="{Binding Message}" />
  </StatusBarItem>
  <StatusBarItem Key="progress" Label="Выполнение" Placement="Right" Width="120">
    <ProgressBar Value="{Binding Progress}" />
  </StatusBarItem>
  <StatusBarItem Key="zoom" Label="Масштаб" Placement="Right" Width="224" CanHide="False">
    <ZoomControl Value="{Binding Zoom}" />
  </StatusBarItem>
</StatusBar>
```

IsCustomizable и ShowResizeGrip по умолчанию False. Любая из этих настроек либо
явный Key/Label/CanHide=False секции включает управляемое размещение всех секций
данной строки. Key — непустая уникальная строка до 256 байт UTF-8; если она не
задана, используется x:Name либо индекс создания. Label задаёт непустую подпись
в списке настройки. Начальная видимость берётся из общего Visibility. Обязательная
секция CanHide=False должна быть видимой. Неизвестный ключ и попытка скрыть
обязательную секцию отклоняются без изменения состояния.

В Rust `StatusBarItemSettings` задаёт `key`, `label`, `placement`, `width`,
`visible`, `can_hide`. `StatusItemWidth::Fixed(width)` сохраняет заданную ширину,
`Stretch { minimum }` резервирует минимум и делит оставшееся место с другими
растягиваемыми секциями. Width должен быть конечным и находиться в 1..=16384.
XAML Width задаёт Fixed; без Width используется Stretch с минимумом не менее 80.
До 256 секций добавляются через `status_bar_item`, возвращающий постоянный host,
либо `register_status_bar_item` для существующего прямого дочернего host.

При недостатке места сначала сохраняются правые секции, затем левые. Остальные
переходят в прокручиваемую поверхность кнопки «More status sections». Переносятся
те же hosts: редактор, документ, выделение, значение прогресса, команды и привязки
продолжают существовать. После увеличения ширины секции возвращаются на свои
стороны в исходном порядке. Явно скрытая секция не показывается в переполнении.
Автоматическая высота управляемой строки равна высоте обычного контрола темы +2,
чтобы рамки и фокус дочерних кнопок помещались целиком. Явная внешняя Height
остаётся ограничением компоновки.

`configure_status_bar` принимает `StatusBarOptions { customizable, resize_grip }`.
Кнопка настройки открывает retained список CheckBox с подписями секций; клавиатура
и доступность используют обычные checkbox/flyout контракты. `status_item_visible`
и `set_status_item_visible` читают и меняют то же состояние. Скрытие сохраняет
содержимое и отменяет недоступный ввод; изменения данных скрытой секции остаются
живыми. Автоматического сохранения выбора на диск нет.

Resize grip — доступная кнопка «Resize window» в правом нижнем углу. PointerDown
либо активация создаёт один `WindowResizeDirection::BottomRight`; приложение
забирает его через `take_window_resize_request` и передаёт нативному оконному
host. Controls сам не меняет размер окна ОС. Приложение, не обрабатывающее этот
запрос, должно оставить resize_grip=False. Запрос не создаёт новое окно и не
меняет размер дочерних панелей. Системные ограничения окна сохраняются.
