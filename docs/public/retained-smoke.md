# Retained GPU smoke test on Windows

`tests/windows_retained_smoke.py` проверяет retained demo, который запускается
командой `gpu-shell` по умолчанию. Нужны интерактивный Windows desktop, Python
3.10+ и release-сборка. Общие Win32 wrappers находятся рядом в
`tests/windows_smoke.py`; сохраняйте оба файла вместе.

```powershell
cargo build --workspace --release --locked
$env:WGPU_BACKEND = 'dx12'
python tests/windows_retained_smoke.py --exe target/release/gpu-shell.exe --output-dir target/retained-dx12
$env:WGPU_BACKEND = 'vulkan'
python tests/windows_retained_smoke.py --exe target/release/gpu-shell.exe --output-dir target/retained-vulkan
Remove-Item Env:WGPU_BACKEND
```

Окно ищется по PID дочернего процесса и заголовку. Во время проверки не
перемещайте и не закрывайте его и не двигайте мышью. Скрипт не переводит окно
в foreground принудительно. Только собственное окно временно поднимается
поверх остальных с `SWP_NOACTIVATE`; перед вводом `WindowFromPoint` проверяет,
что нужная точка действительно принадлежит этому окну. Скрипт временно
перемещает системный курсор по точкам клиентской области и возвращает исходную
позицию при cleanup, включая завершение с ошибкой.

Проверки проходят через native event loop:

1. Первый GPU frame выполняет layout и paint; в покое все три счётчика стабильны.
2. Наведение на A меняет hover и paint без layout. Primary down активирует A,
   назначает focus и pointer capture. Capture сохраняется при движении за пределы
   A и освобождается после primary up.
3. Tab переводит focus на B, Enter активирует B. Следующий Tab выбирает C,
   Delete удаляет C: количество nodes уменьшается, focus/capture очищаются,
   layout пересчитывается.
4. Wheel прокручивает содержимое без layout. Отдельный posted blur очищает
   focus/capture без layout. Resize пересчитывает геометрию.
   Minimize/restore сохраняют счётчики активаций и удалений, число nodes и scroll
   с допустимым ограничением нового viewport. После восстановления окно снова
   остаётся в покое.
5. `WM_CLOSE` завершает процесс с кодом 0.

Ввод состоит из синтетических
[`PostMessageW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew)
сообщений, а не физического ввода человека. Pointer coordinates пересчитываются
по DPI окна. Позиция OS cursor синхронизируется через
[`SetCursorPos`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcursorpos):
winit включает native
[`TrackMouseEvent`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-trackmouseevent),
поэтому одни posted coordinates при курсоре вне окна немедленно вызывают
`WM_MOUSELEAVE`. Исходная позиция читается через
[`GetCursorPos`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos)
до запуска приложения; результат восстановления записывается в JSON.
Для [`WM_KEYDOWN`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keydown)
и [`WM_KEYUP`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-keyup)
формируются scan code, extended flag и transition bits; scan code получается
через [`MapVirtualKeyW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-mapvirtualkeyw).
Это не проверка IME, ввода текста, раскладок клавиатуры, физической мыши,
переноса между мониторами или GPU device loss. A/B/C являются интерактивными
примитивами, а не законченными controls.

Состояние focus для fixture задаётся posted `WM_NCACTIVATE` и
[`WM_SETFOCUS`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-setfocus);
отдельно проверяется
[`WM_KILLFOCUS`](https://learn.microsoft.com/en-us/windows/win32/inputdev/wm-killfocus).
Это синтетические уведомления своего HWND, не реальная смена foreground/focus
между приложениями. В JSON они явно указаны в `focus_source`; изменение
z-order — в `window_visibility`.

В каталоге результатов сохраняются `report.json`, `stdout.log`, `stderr.log`
и доступный снимок клиентской области `client.bmp` (также `client.png`, если
установлен Pillow). JSON содержит состояния после каждого действия, счётчики,
DPI, PID, backend environment, статус восстановления курсора и код завершения. Вывод приложения уточняет
фактически выбранный adapter/backend.

[`PrintWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow)
рисует клиентскую область нужного окна в memory bitmap. Рабочий стол и
перекрывающие окна не считываются. Захват выполняется в отдельном процессе
с ограничением до пяти секунд; ownership и retained title проверяются перед
захватом и после него. Перед записью файла проверяются цвета A, B и navigation
panel с допуском на sRGB encoding. Несоответствующий снимок не сохраняется.

Захват оценивается отдельно: `screenshot.status = "unavailable"` содержит
причину и не отменяет успешные проверки ввода/lifecycle. Используйте пути снимков
из текущего JSON, чтобы не принять старое изображение за новое. Доступное
изображение следует просмотреть отдельно: контрольные точки не заменяют
визуальную проверку всего кадра.

Timeout одной операции по умолчанию составляет 20 секунд (`--timeout`).
При ошибке скрипт сохраняет причину в JSON, завершает только собственный дочерний
процесс и возвращает код 1. Быстрая проверка parser, keyboard message bits и
валидатора изображения не запускает приложение:

```text
python tests/windows_retained_smoke.py --self-test
```

Результат относится к конкретному Windows-сеансу и backend. Он не доказывает
GPU runtime Linux/macOS или корректность других конфигураций DPI.
