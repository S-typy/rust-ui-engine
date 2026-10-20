# Медиа-источники и представление

`ui-core::media::MediaProvider` описывает источник кадров. Он создаётся, используется
и освобождается на одном рабочем потоке. `MediaInfo` объявляет название,
продолжительность и `MediaCapabilities`: seek, capture, volume и live.
`MediaFrame` содержит временную позицию и immutable RGBA `Bitmap`.

`ui-controls::media::MediaSession` предоставляет open, play, pause, stop, seek,
capture, volume и poll. `MediaView` монтирует поверхность, кнопки, редакторы позиции
и громкости. Недоступные функции отключены по capabilities. Кадр масштабируется
с сохранением пропорций, выводится через Scene и общий GPU renderer.

```text
cargo run -p xaml-form -- --media
cargo run -p xaml-form -- --media --media-file C:\Videos\sample.mp4
```

Без файла пример использует собственную шестисекундную последовательность кадров,
первоначально на паузе. `--composition-preview` включает её воспроизведение.
«Открыть» позволяет выбрать до 128 файлов; предыдущий/следующий переключают
playlist. Открытие не означает автоматический запуск. «Снимок кадра» предлагает
явно сохранить PNG. Запись и PNG encoding выполняются на worker.
Space на поверхности переключает play/pause, стрелки перематывают на пять секунд,
Escape останавливает источник.

## Конкурентность и lifecycle

UI не ждёт декодирования. В mailbox хранится последний запрос кадра и последний
результат; очередь не растёт с числом кадров. Громкость имеет отдельный ограниченный
канал и публикуется после подтверждения provider. Изменение позиции получает
новый токен: результат прежнего seek не заменяет новый кадр.

Pause прекращает продвижение времени, seek допустим только внутри duration.
Stop очищает видимый кадр и отменяет worker; явный play после stop вновь создаёт
источник. End сохраняет последний кадр для capture и освобождает provider.
Ошибка уничтожает provider и переводит session в Error. Close также забывает
factory и metadata. Drop отменяет работу при уничтожении владельца.

Отмена кооперативная: provider обязан проверять флаг между блокирующими вызовами.
UI не делает блокирующий join. Пока предыдущий provider завершает вызов, возможны
текущий и один завершающийся worker. Следующее открытие возвращает диагностируемую
занятость до завершения предыдущего; бесконечная очередь декодеров не создаётся.
Это не изоляция недоверенного декодера и не гарантия времени завершения системного
I/O. Приложению с требованиями жёсткого deadline нужен отдельный процесс.

`ImageSequenceProvider` — конечная последовательность до 3600 кадров/128 MiB
уникальных изображений, интервал 10 ms–60 s. Общие bitmap limits сохраняются.
Позиция кадра определяется дискретностью источника. Capture копирует immutable
ссылку на уже показанный кадр, не инициирует доступ к камере.

## Windows video provider

`ui-platform-winit::media::open_video_file` использует системный Media Foundation
Source Reader, явно выбирает video stream и преобразует RGB32 в RGBA.
COM и Media Foundation имеют сбалансированный lifecycle на потоке provider.
Код обрабатывает фактический stride, проверяет размеры и границы строк,
отклоняет смену неподдержанного формата, rotation metadata и некорректные timestamps.
Выбор формата/декодера определяется установкой Windows; наличие расширения файла
не гарантирует поддержку его содержимого.

Источник принимает существующий локальный файл, не открывает сетевой URL, аудио
или камеру. Максимальная продолжительность — семь дней, размер кадра ограничен
общим Bitmap, поиск кадра — 600 samples. Системный `ReadSample` синхронный, поэтому
его выполняет worker; флаг отмены проверяется между samples.
Декодирование/цветовая конверсия могут выполняться CPU средствами ОС; отображение
по-прежнему использует GPU. Аппаратное декодирование и zero-copy не обещаются.

Для проверки доступны `media_fixture` (создание собственного короткого H.264 MP4
по новому пути) и `media_probe` (чтение, seek, cancel/reopen без окна и аудио):

```text
cargo run -p rust-desktop-ui-platform-winit --example media_fixture -- sample.mp4
cargo run -p rust-desktop-ui-platform-winit --example media_probe -- sample.mp4
```

Основания платформенного контракта:
[Source Reader](https://learn.microsoft.com/en-us/windows/win32/medfound/processing-media-data-with-the-source-reader),
[stride](https://learn.microsoft.com/en-us/windows/win32/medfound/image-stride),
[Lock2D](https://learn.microsoft.com/en-us/windows/win32/api/mfobjects/nf-mfobjects-imf2dbuffer-lock2d).

Системный backend камеры, синхронное audio/video playback и аудиовывод ещё не
предоставлены. Live/capture capabilities доступны для внешнего provider; это
не утверждение о проверке конкретного устройства. Linux/macOS file providers,
native ввод/AT и смешанный DPI требуют отдельных реализаций/проверок.
