# Windows runtime smoke test

`tests/windows_smoke.py` запускает собранный `gpu-shell.exe` на интерактивном
Windows desktop. Требуется Python 3.10+; сторонние Python-зависимости не обязательны.

```powershell
cargo build --workspace --release --locked
python tests/windows_smoke.py --exe target/release/gpu-shell.exe --output-dir target/windows-smoke
```

Во время короткого теста не перемещайте и не закрывайте его окно. Скрипт не
переключает foreground принудительно. Он находит окно по PID своего дочернего
процесса и заголовку `Rust UI Engine |`, исключая вспомогательные окна драйвера,
проверяет первый presented frame, переключение демонстрационной вкладки, выбор
строки и прокрутку. Затем изменяет размер через `SetWindowPos`, сворачивает и
восстанавливает окно, проверяет новые кадры и сохранение состояния, закрывает
приложение через `WM_CLOSE`. Отдельные интервалы наблюдения проверяют отсутствие
непрерывной перерисовки в покое и при сворачивании.

Мышь проверяется **синтетическими Win32-сообщениями** через
[`PostMessageW`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew).
Это прохождение сообщений через нативный event loop, а не физический ввод человека.
Координаты пересчитываются в физические пиксели по
[`GetDpiForWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getdpiforwindow).
Тест не подтверждает перенос окна между мониторами с разным DPI, IME, accessibility
или восстановление после фактической потери GPU device.

Результаты находятся в `--output-dir`:

- `report.json` — действия, проверенные состояния, DPI, PID и код завершения;
- `stdout.log`, `stderr.log` — вывод приложения, включая adapter/backend;
- `client.bmp` — снимок клиентской области, если GPU-окно поддерживает захват;
- `client.png` — тот же снимок, если установлен Pillow.

Снимок запрашивается у конкретного окна через
[`PrintWindow`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-printwindow)
с `PW_CLIENTONLY | PW_RENDERFULLCONTENT` в отдельный memory bitmap. Пиксели
рабочего стола и перекрывающих окон не считываются. Синхронный `PrintWindow`
выполняется в отдельном процессе с пределом ожидания до пяти секунд.

Перед сохранением проверяются три характерные точки сцены: белая активная
вкладка, синее подчёркивание и тёмный маркер строки. Пустой или несоответствующий
снимок не сохраняется. Если захват недоступен, `report.json` содержит
`screenshot.status = "unavailable"` и причину; результат проверок ввода и lifecycle
оценивается отдельно. Используйте пути снимков из текущего JSON, чтобы не принять
файлы предыдущего запуска за новый результат.

Просмотрите доступное изображение отдельно: контрольные точки и успешные проверки
заголовка не заменяют визуальную оценку всего кадра.

При несовпадении состояния, timeout или ошибке приложения скрипт завершится с
кодом 1 и сохранит причину в JSON. `--timeout` задаёт предел одной операции в
секундах, по умолчанию 20. При ошибке cleanup закрывает или завершает только
созданный скриптом процесс.

Для отдельных запусков DX12/Vulkan задайте backend и разные каталоги результатов:

```powershell
$env:WGPU_BACKEND = 'dx12'
python tests/windows_smoke.py --exe target/release/gpu-shell.exe --output-dir target/windows-smoke-dx12
$env:WGPU_BACKEND = 'vulkan'
python tests/windows_smoke.py --exe target/release/gpu-shell.exe --output-dir target/windows-smoke-vulkan
Remove-Item Env:WGPU_BACKEND
```

Каждый результат относится только к указанному adapter/backend и текущему Windows
сеансу. Сборки Linux/macOS и unit-тесты не заменяют такой runtime-протокол.
