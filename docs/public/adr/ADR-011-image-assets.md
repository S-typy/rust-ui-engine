# ADR-011: статические изображения и GPU-композиция

Статус: принято. Дата: 2026-10-16.

## Задача

Формы должны показывать растровые изображения и статические векторные ресурсы
с корректными alpha, DPI, обрезкой и повторным использованием. Привязка
исходного файла к GPU handle усложнила бы смену устройства и работу нескольких
окон; декодирование во время каждого кадра создало бы лишнюю работу и allocations.

## Решение

`ui-core::Bitmap` хранит immutable straight-alpha sRGB RGBA8 с общей allocation
и стабильным process-local ID. `ImagePrimitive` задаёт destination, source crop,
clip, opacity и sampling; общий `DrawCommand` сохраняет painter order. Формат
не содержит типов декодера, оконной платформы или wgpu.

`ui-images` владеет подготовкой ресурсов. `image` декодирует PNG/JPEG/BMP/ICO;
`resvg`/`usvg` подготавливают статический SVG через Rust rasterizer `tiny-skia`.
SVG defaults text/system-fonts/raster-images выключены. Собственная проверка
разрешает ограниченный self-contained профиль shapes/paths/gradients/clips/masks,
отклоняет внешний I/O, активное содержимое и превышение structural/render budgets.
Исходники, cache entries, output pixels и фактические compositing layers имеют
отдельные лимиты. SVG преобразуется в bitmap для заданных scale/currentColor.

CPU-подготовка относится к декодированию одного ресурса. Она не принимает Scene,
не рисует контролы, текст формы или окно и не выбирается при отсутствии GPU.
`ui-render-wgpu` остаётся единственным compositor UI: загружает bitmap,
фильтрует premultiplied linear samples и смешивает их с остальными примитивами.
Аппаратный adapter обязателен; ошибки GPU возвращаются host.

`ImageCache` ключуется source identity и параметрами подготовки, возвращая
разделяемый `ImageAsset`. Controls владеют placement/state. Каждый renderer
держит собственные GPU textures, ограниченные видимым набором. После device
recreation retained Scene повторно загружает исходные bitmap без повторного
чтения файлов или декодирования. Владение ресурсом не зависит от HWND.

## Последствия

SVG масштабируется повторной подготовкой bitmap; это не произвольный vector
primitive в Scene. Runtime SVG text, анимация, filters, embedded images и
внешние шрифты/ссылки не поддерживаются. Сложные изображения могут быть
отклонены даже при маленьком выходном размере: reference expansion, compositing
allocations, path/dash segments и raster work проверяются до rasterization.

Scene сохраняет axis-aligned primitives и rectangular clipping. Произвольные
GPU masks/effects/rotations и isolated opacity layers остаются отдельным
расширением renderer. Статическая SVG-маска становится частью подготовленного
bitmap и не меняет этот контракт Scene.

Новые зависимости проходят source-license inventory. Собственный код остаётся
Apache-2.0; MIT/Apache/BSD notices зависимостей сохраняются отдельно.
Контракты: [отрисовка](../rendering.md), [зависимости](../dependencies.md).
