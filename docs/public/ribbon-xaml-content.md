# Содержимое Ribbon в XAML

`RibbonContent` размещает обычное дерево контролов внутри группы Ribbon.
Редакторы, их документы и привязки создаются один раз; изменение ширины,
активной вкладки и свёрнутого состояния меняет размещение существующих узлов.
Слот располагается после команд своей группы. Команду после `RibbonContent`
нужно перенести выше либо в следующую группу.

```xml
<Ribbon CustomizeQuickAccess="True">
  <RibbonTab Id="Home" Header="Главная" KeyTip="H">
    <RibbonGroup Id="Text" Header="Текст">
      <Button Command="Save" KeyTip="S" />
      <RibbonContent Id="Font" Header="Шрифт" KeyTip="FF"
                     Width="180" MinWidth="120" Height="84">
        <StackPanel Spacing="4">
          <TextBlock Text="Название шрифта" />
          <ComboBox x:Name="Font" IsEditable="True"
                    ItemsSource="{Binding Fonts}"
                    SelectedValue="{Binding Font}" />
        </StackPanel>
      </RibbonContent>
    </RibbonGroup>
  </RibbonTab>
</Ribbon>
```

Разметка использует пространства имён формы: `xmlns="urn:rust-ui-engine:ui"`
и `xmlns:x="http://schemas.microsoft.com/winfx/2006/xaml"`. Обработчики команд
регистрируются приложением до создания формы.

| Свойство RibbonContent | Контракт |
|---|---|
| `Id` | Обязательный устойчивый идентификатор, уникальный во всём Ribbon; 1–128 ASCII-символов: буквы, цифры, `_`, `-`, `.` |
| `Header` | Обязательное доступное название, до 4096 байт UTF-8 |
| `KeyTip` | Обязательная последовательность до 32 ASCII-букв/цифр, без совпадающих префиксов с командами и другими слотами вкладки |
| `Width` | Предпочтительная ширина 24–4096, по умолчанию 180 |
| `MinWidth` | Минимальная ширина от 24 до `Width`, по умолчанию 120 |
| `Height` | Высота 24–512, по умолчанию 84 |

Допускается до 64 слотов. Каждый содержит ровно один корневой элемент;
для нескольких контролов используется панель. Группа может состоять только
из слотов. При нехватке ширины содержимое открывается через общий механизм
свёрнутой группы. KeyTip переводит фокус на доступный дочерний контрол.

`x:Name`, `ElementName`, `DataContext`, bindings, Styles и Resources внутри
содержимого работают в области содержащей формы. Для отдельного пути данных
задайте `DataContext` на корневой панели слота. Общее число элементов и глубина
включают RibbonTab, RibbonGroup и RibbonContent. Вложенный Ribbon и DockWorkspace
в слоте или странице Backstage не поддерживаются.

## Backstage

`Ribbon.Backstage` содержит один `Backstage` со страницами и командами.
Страница имеет собственный постоянный корень содержимого. Закрытие поверхности
не пересоздаёт редакторы страницы; подтверждение значения по уходу фокуса
подчиняется обычному `CommitOnLostFocus` конкретного редактора.

```xml
<Ribbon>
  <Ribbon.Backstage>
    <Backstage Header="Файл" KeyTip="F">
      <BackstagePage Id="Info" Header="Сведения" KeyTip="I">
        <StackPanel Spacing="8">
          <TextBlock Text="Название документа" />
          <TextBox x:Name="DocumentTitle" Text="{Binding Document.Title}" />
        </StackPanel>
      </BackstagePage>
      <BackstageCommand Command="Save" KeyTip="S" />
    </Backstage>
  </Ribbon.Backstage>
  <RibbonTab Id="Home" Header="Главная" KeyTip="H">
    <RibbonGroup Id="Document" Header="Документ">
      <Button Command="Save" KeyTip="S" />
    </RibbonGroup>
  </RibbonTab>
</Ribbon>
```

Backstage принимает 1–32 элемента. `Header` и `KeyTip` обязательны. Корневой
KeyTip не пересекается с вкладками и зарезервированными цифровыми KeyTips QAT.
Внутри Backstage KeyTips страниц и команд также не имеют общих префиксов.
Идентификаторы страниц уникальны в Backstage и используют правила `RibbonContent.Id`.
Названия ограничены 4096 байтами, KeyTips — 32 символами. Страница содержит ровно
один корень; `BackstageCommand` содержит только `Command` и `KeyTip`, а подпись,
доступность и поведение получает из `CommandRegistry`.

В Rust определения представлены `RibbonContentDefinition`,
`RibbonBackstageDefinition` и `RibbonBackstageItemDefinition`. Адаптер регистрирует
их в `Ribbon` до монтажа, получает `content_host`/`backstage_page_host` после
монтажа и наполняет каждый корень ровно один раз. `open_backstage`,
`close_backstage` и `select_backstage_page` управляют уже созданными страницами.

## Настройка панели быстрого доступа

`CustomizeQuickAccess="True"` включает интерфейс явного изменения QAT.
Каталог содержит уникальные команды, перечисленные в Ribbon, его меню,
BackstageCommand и исходной QAT; максимум — 256 команд. Обычные кнопки внутри
произвольного содержимого не добавляются в каталог автоматически. Каталог
показывает текущие подписи зарегистрированных команд.

Добавление, удаление, перестановка и сброс сначала изменяют локальный черновик;
применение подтверждает весь новый порядок. Отмена сохраняет предыдущую QAT.
По умолчанию `CustomizeQuickAccess` выключен. Разметка не выполняет файловое
сохранение настроек; приложение использует отдельный `RibbonSettings` API.

Общий контракт команд, адаптивных размеров и клавиатуры описан в
[Ribbon](ribbon.md); параметры редакторов — в [формах](forms.md).
