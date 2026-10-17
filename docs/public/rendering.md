# Scene и GPU-отрисовка

`Scene` задаёт порядок рисования независимо от оконной системы и GPU.
`GpuRenderer` принимает эту сцену и положительный DPI scale и выводит кадры
на sRGB surface аппаратного устройства wgpu. CPU/Noop adapters отклоняются.

Поддерживаются сплошные прямоугольники, скруглённая заливка и внутренняя рамка,
ограниченные `StrokePath`, текстовые runs и изображения. Массивы примитивов имеют
общий список `DrawCommand`; batching объединяет только соседние команды одного
типа. Поздняя заливка или картинка действительно перекрывает более ранний текст.
Backend повторно проверяет индексы, порядок, геометрию и рабочие лимиты перед upload.

## Изображения

`Bitmap::new_rgba8(width, height, pixels)` принимает плотно упакованные строки
straight-alpha sRGB RGBA8 сверху вниз. Размеры ненулевые, не больше 8192 по каждой
оси; ресурс не больше 64 MiB. Конструктор проверяет точную длину `width*height*4`.
Пиксели неизменяемы; clone разделяет allocation и устойчивый ID. Новый ресурс
получает новый ID, даже когда его пиксели совпадают. Декодирование файлов и кэш
источников находятся за пределами renderer.

```rust
use rust_desktop_ui_core::{Bitmap, ImagePrimitive, ImageSampling, Rect, Scene};

let bitmap = Bitmap::new_rgba8(1, 1, vec![40, 120, 240, 180])?;
let mut image = ImagePrimitive::new(bitmap, Rect::new(20.0, 20.0, 80.0, 60.0));
image.sampling = ImageSampling::Linear;
image.opacity = 0.8;
let mut scene = Scene::default();
scene.image(image);
# Ok::<(), rust_desktop_ui_core::BitmapError>(())
```

`bounds` и `clip` задаются в логических пикселях, `source` — в пикселях bitmap.
По умолчанию показывается весь источник. Crop должен находиться внутри источника;
его ширина и высота положительны. `Scene::image` пропускает ошибочные и полностью
обрезанные команды. Backend отклоняет повреждённый публичный массив `images`.
Прямоугольный clip сохраняет исходное сопоставление destination/source.

`Nearest` выбирает ближайший texel; `Linear` интерполирует premultiplied linear
RGB и alpha. Upload декодирует RGB, умножает его на alpha и кодирует в sRGB-текстуру;
GPU декодирует перед фильтрацией. Прозрачный цвет не создаёт цветной ореол.
UV ограничены центрами крайних texels выбранного crop, исключая соседние пиксели
при целочисленном выделении области. Opacity умножает все premultiplied каналы.
Alpha остаётся линейной; ICC, HDR и wide-gamut conversion не выполняются.

GPU кэш содержит только видимые ресурсы последнего подготовленного кадра.
Общий лимит — 128 MiB RGBA8 и 4096 уникальных bitmap; повторные размещения одного
ID используют одну текстуру. Неактивные текстуры освобождаются до новых allocations,
а текстуры текущего кадра не вытесняются во время отрисовки. Превышение лимита
возвращает ошибку. После пересоздания устройства исходная Scene повторно загружает
bitmap; GPU handles в модели приложения не сохраняются.

## Координаты и композиция

`append_view(source, offset, clip)` переносит независимую сцену.
`append_transformed_view(source, offset, scale, clip)` также применяет единый
положительный масштаб к геометрии, радиусам, толщине и размерам шрифта.
Clip задаётся в координатах назначения, source bitmap не масштабируется.
Ошибочный transform или повреждённая ссылка оставляет destination без изменений.
После масштабирования размер шрифта должен оставаться в пределах `(0, 512]`
логических пикселей; превышение возвращает ошибку без частичного добавления сцены.
В конце renderer применяет DPI ровно один раз. Alpha blending выполняется
в linear RGB, выход кодируется sRGB surface.

Поддерживаемый контракт — прямоугольные clips и последовательная композиция
примитивов. Rotation/skew, произвольные clip masks, изолированные opacity layers,
blend modes и эффекты фильтров не поддерживаются. Скругление фигуры не создаёт
скруглённый clip для её потомков. Эти ограничения относятся к формату Scene,
а не к автоматическому software fallback.

Кадр ограничен 16384 прямоугольниками/скруглениями/strokes/images суммарно,
16384 text runs, 1 MiB входного UTF-8 и 65536 glyph instances. Глифы используют
собственный ограниченный atlas и лимит bitmap allocations 64 MiB.

## Восстановление и проверка

`render` возвращает `Presented`, `Skipped` либо `RecoveryRequired`.
Нулевые размеры паркуют surface; resize, timeout, occlusion и reconfiguration
не уничтожают модель приложения. При потере surface/device host ограниченно
пересоздаёт renderer и повторяет кадр с актуальной Scene.

`RunOptions { smoke_test: true, smoke_device_recovery: true, .. }` включает
диагностику реального `device.destroy()` после resize. Обычный single/multi-window
host обязан пройти ветку `RecoveryRequired`, пересоздать renderer и представить
новый кадр до завершения smoke. Опция запрещена без `smoke_test`.

```powershell
$env:WGPU_BACKEND = 'dx12' # повторить с vulkan
cargo test -p rust-desktop-ui-render-wgpu -- --ignored --nocapture
cargo run -p rust-desktop-ui-platform-winit --example recovery_smoke
cargo run -p rust-desktop-ui-platform-winit --example recovery_smoke -- --multi
```

Аппаратный pixel-readback test проверяет alpha, painter order, nearest/linear,
crop, clip, масштаб, cache reuse, реальный device-loss callback и равенство пикселей
после создания нового устройства. Native example сохраняет Scene и bitmap ID,
проверяет повторный upload и наличие глифов в одиночном и двухоконном host.
Контролируемое уничтожение устройства не подтверждает восстановление после
физического сбоя драйвера или отключения адаптера. Windows DX12/Vulkan проверяются
локально; Linux/macOS и физические отказы требуют отдельных runtime-проверок.

## Контекст устройства и поверхности

`GpuRenderer::new(window)` создаёт независимый контекст. Для нескольких окон
`GpuRenderer::new_with_context(window, &mut context)` использует явный
`GpuContext`: Instance, Adapter, Device, Queue и состояние их поколения.
Контекст не владеет Window/Surface. У каждого renderer собственные surface,
pipelines, buffers, glyph atlas и image cache; их лимиты не объединяются между
окнами. Многооконный platform host сохраняет контекст до своего завершения.

`GpuContext::generation()` возвращает 0 до первого устройства и увеличивается
при создании нового поколения; renderer сообщает своё поколение через
`context_generation()`. После device loss `GpuContext::is_lost()` позволяет
обнаружить общее состояние. Новый renderer получает свежие Adapter/Device,
а host пересоздаёт связанные ресурсы других окон. Surface loss при здоровом
device требует только локального восстановления. Несовместимая surface и
backend validation остаются явными ошибками; software fallback не добавляется.

Device recovery ограничен тремя последовательными пересозданиями без успешного
кадра. `Presented` любого окна сбрасывает общий счётчик; обновление поколения
у свёрнутого или нулевого viewport само по себе не расходует повторную попытку.
`RendererStats::device_generation` позволяет связать показанный кадр с поколением.
Это диагностический номер, не идентификатор документа или GPU adapter.

Сцены и исходные bitmap остаются у приложения, поэтому восстановление GPU
не пересоздаёт документы. Контекст не разделяется между независимыми host
и не выполняет миграцию между adapter. [ADR-012](adr/ADR-012-shared-gpu-context.md)
фиксирует время жизни, recovery и границы проверки.
