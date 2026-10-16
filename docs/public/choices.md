# Выбор и переключатели

`CheckState` различает `Unchecked`, `Checked` и `Indeterminate`.
`checkbox_tristate` создаёт локальный трёхсостоянийный control; обычные CheckBox
и ToggleButton включают этот режим через `set_three_state(id, true)`.
`set_check_state` проверяет режим до изменения. Выключение трёхсостоянийного
режима переводит Indeterminate в Unchecked; обычный `set_checked(bool)` тоже
явно завершает неопределённое состояние.

Пользовательская активация циклически меняет Unchecked → Checked →
Indeterminate → Unchecked. В двухсостоянийном режиме Indeterminate пропускается.
Pointer release вне контрола и blur отменяют нажатие; Enter выполняет одну
активацию, Space — одну при отпускании. Disabled controls не меняют значение.
Semantic tree содержит `checked_mixed` отдельно от совместимого bool checked;
platform adapter передаёт mixed screen-reader API.

Зарегистрированные command toggles хранят bool. Поэтому включение трёх
состояний для привязанной к команде кнопки отклоняется до изменений. Локальное
nullable значение можно использовать с моделью данных независимо от команд.

`CheckPresentation::CheckBox`, `Toggle` и `Switch` выбираются через
`set_check_presentation`. Меняется представление и semantic role, а widget,
checked state, label, command и focus сохраняются. `check_presentation`
читает текущий выбор. Toggle рисует общую поверхность кнопки; mixed Toggle
показывает горизонтальную отметку. Switch рисует track и thumb, в mixed state
thumb находится посередине. Все представления используют общий ButtonStyle,
состояния hover/pressed/focus/disabled, custom brushes и high-contrast палитры.

`radio_choice_group(parent, items, selected_key, style)` создаёт группу по
`SelectionItem { key, text, enabled }`. `set_radio_items` принимает до 256
записей с уникальными непустыми keys и общим объёмом keys/text до 1 MiB UTF-8.
Ошибка в составе проверяется до изменения дерева. `radio_item_widget` даёт
WidgetId конкретного ключа, а `radio_selected_key` / `set_radio_selected_key`
работают с устойчивым ключом, включая отсутствие выбора.

При изменении подписи или порядка сохраняются surviving WidgetId, выбранный
ключ и фокус. Если сфокусированный item удалён либо отключён, focus переходит
на первый доступный item. Удалённый выбранный key не выбирает другой элемент
автоматически. Стрелки и Tab используют общий контракт RadioButton: одна
остановка Tab на группу и переходы по доступным элементам.

`set_button_content(radio_item_widget(...), Some(root))` устанавливает retained
item template из неинтерактивных элементов. Изменение ItemsSource сохраняет
такое содержимое для surviving keys; удаление key уничтожает принадлежащее
ему subtree. Доступное имя остаётся текстом item, а выбор и индикатор принадлежат
исходному RadioButton.
