# Изображения

`rust-desktop-ui-images` декодирует статические изображения в неизменяемый
`Bitmap`. `Controls` отвечает за размер, размещение и семантику Image;
`Scene::image` передаёт ресурс renderer для GPU-композиции вместе с остальными
командами сцены. Декодирование файлов и подготовка статического SVG выполняются
на CPU; показ окна и композиция выполняются через wgpu.

## Источники и декодирование

Поддерживаются PNG, JPEG, BMP, ICO и ограниченный статический SVG.
`ImageSource::from_bytes(bytes, format)` создаёт источник с общей неизменяемой
памятью и стабильной идентичностью. `from_file(path)` читает явно указанный
локальный файл и определяет формат. Сетевой загрузки и разрешения путей
относительно документа нет. Создание источника проверяет размер bytes;
декодирование выполняется при `decode` или загрузке через cache.

`ImageAsset` предоставляет `bitmap()`, `natural_size()` и `format()`.
`Bitmap` хранит RGBA8, sRGB, straight alpha, строки сверху вниз.
JPEG orientation применяется при декодировании. `DecodeOptions.svg_scale`
меняет разрешение подготовленного SVG, сохраняя его логический natural size.
`current_color` задаёт наследуемый RGB для SVG `currentColor`; явные цвета
источника сохраняют свой приоритет.

SVG-профиль поддерживает геометрические фигуры и path, группы, transforms,
заливки и strokes, линейные/радиальные градиенты, clip paths и masks.
Разрешены только внутренние ссылки `#id`. Scripts, обработчики событий,
DTD/entities, внешние ресурсы, embedded images, text, filters, animation,
`use` и stylesheets не входят в профиль. Для inline style действует список
поддерживаемых свойств; неизвестные свойства возвращают диагностическую ошибку.
Циклы и чрезмерное расширение внутренних ссылок отклоняются до rasterization.

Основные ограничения:

| Ресурс | Лимит |
|---|---|
| Encoded source | От 1 байта до 16 MiB |
| Bitmap | До 8192 пикселей на сторону и 64 MiB RGBA |
| SVG bitmap | До 16 MiB RGBA |
| SVG scale | 0.125–8, конечное число |
| SVG source XML | До 4096 узлов, глубина до 64 |
| SVG XML names | До 256 байт на имя элемента или атрибута |
| SVG expanded graph | До 16384 узлов и 16384 связей |
| SVG path/dash expansion | До 65536 сегментов |
| SVG compositing | До 8 isolated group uses; суммарно до 128 MiB выходных и временных пиксельных буферов |
| SVG raster work | Оценка работы растеризации до 128 Mi пикселей |

Лимиты проверяются как по исходной структуре, так и по нормализованному дереву
отрисовки. Они могут отклонять сложное изображение, даже если его внешний размер
мал. Ошибки различают `Limit`, `Invalid`, `Unsupported` и `Io`.
Текст декодирующей диагностики ограничен 4096 UTF-8 байтами, включая сокращение.
Строковые сообщения декодера ограничены 4 KiB UTF-8; усечение сохраняет
категорию ошибки и отмечается `...`. Служебный префикс Display в этот размер
не входит.
Лимит пиксельных буферов не является лимитом всей памяти процесса:
структуры парсера и контуров отдельно ограничены размером исходника,
числом узлов, ссылок и сегментов.

## Image в форме

```rust
use rust_desktop_ui_controls::{Controls, ImageFormat, ImageSettings, ImageSource, Theme};
use rust_desktop_ui_core::LayoutStyle;
let source = ImageSource::from_bytes(
    br#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"><circle cx="12" cy="12" r="10" fill="currentColor"/></svg>"#.to_vec(),
    ImageFormat::Svg,
)?;
let mut ui = Controls::new(Theme::FluentLight)?;
ui.register_image_source("Logo", source)?;
let logo = ui.image_from_resource(ui.root(), "Logo", ImageSettings::default(),
    LayoutStyle::default(), "Логотип")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`image(parent, source, settings, style, alt)` принимает источник напрямую;
`image_from_resource` связывает control с именем в локальном registry экземпляра
`Controls`. Имя содержит 1–256 ASCII букв, цифр и символов `_`, `.`, `-`.
Оно является ключом, а не файловым путём. `register_image_source` заменяет ресурс
с тем же именем; связанные controls обновляются на следующем `update`.
`unregister_image_source` удаляет связь registry. `image_resource_size` получает
логический размер через общий cache и возвращает ошибку для недоступного ресурса.

Image не принимает input или focus. Alt-текст доступен через semantic Image role.
При отсутствии ресурса или первом отказе декодирования показывается собственный
placeholder с intrinsic size 32×32. Форма продолжает работать;
`image_error(id)` возвращает причину. Если раньше было корректное изображение,
неудачная замена сохраняет последний asset и его размер, дополнительно записывая
ошибку. После исправления ресурса следующий update очищает ошибку.

`set_image_source`, `set_image_resource` меняют источник, `set_image_settings` —
параметры отображения. `image_asset`, `image_settings` читают действующее состояние.
Неверные ID, настройки или превышение encoded budget отклоняются без изменений.
Detach сохраняет asset; удаление узла освобождает sidecar, поэтому старый WidgetId
не может изменить новое изображение.

## Размер и представление

`ImageSettings` содержит `stretch`, `sampling`, `opacity`, `current_color`.

| Stretch | Поведение |
|---|---|
| None | Intrinsic размер, центрирование и clip по control bounds |
| Fill | Заполнение bounds с независимым масштабом осей |
| Uniform | Сохранение пропорций с размещением всего изображения |
| UniformToFill | Сохранение пропорций с заполнением bounds и обрезкой краёв |

По умолчанию используется Uniform, Linear sampling, opacity=1 и theme-relative
currentColor. Nearest подходит для пиксельной графики. Opacity должна быть конечной
и находиться в [0,1]; явный `current_color` принимает opaque RGB. Остальные цвета
SVG не меняются вместе с темой.

При двух Auto осях используется natural size. Если одна ось Auto, а другая Px,
Auto ось вычисляется по пропорциям. Для остальных единиц Auto использует intrinsic
размер. Явные размеры не переписываются. Внешняя замена ранее вычисленной оси
на другое явное значение передаёт её под управление приложения; значение Auto
возвращает автоматический размер. Равное прежнему вычисленному значению не имеет
отличимой provenance.

Host вызывает `set_image_scale` при изменении DPI. SVG получает новый bitmap,
логические размеры сохраняются. Raster pixels не пересэмплируются в декодере;
renderer использует выбранный sampling. Source mapping сохраняется при clipping,
поэтому обрезка не растягивает оставшуюся часть изображения.

## Кэширование и владение

`ImageCache` использует source identity и decode options, LRU и byte budget.
По умолчанию — 128 записей и 64 MiB с учётом encoded bytes и bitmap. Настраиваемые
верхние пределы — 4096 записей и 512 MiB. Корректный asset больше выбранного budget
возвращается без кэширования. Ошибка декодирования не вытесняет старые записи.
Clones сохранённых приложением assets живут независимо от eviction.

`Controls` отдельно ограничивает registry 256 именами и 64 MiB уникальных encoded
источников. Прямые источники контролов имеют отдельный encoded budget 64 MiB;
до 4096 image controls удерживают до 64 MiB уникальных bitmap. Cache имеет свой
отдельный budget. При отказе из-за live bitmap budget повторная загрузка возможна
после изменения набора удерживаемых assets; повторное декодирование не запускается
на каждом неизменившемся кадре. `image_cache_stats` сообщает entries, bytes,
hits и misses.

GPU renderer удерживает только изображения видимой сцены, до 4096 уникальных
ресурсов и 128 MiB texels, с дополнительным ограничением размера текстур adapter.
Один bitmap может использоваться многими Image commands; painter order,
opacity, clipping и настройки sampling сохраняются.
