# SQLite: транзакции, запросы и страницы

`SqliteAdapter` доступен с feature `sqlite` у `rust-desktop-ui-data`.
Приложение создаёт и мигрирует таблицы. Адаптер читает одну настроенную таблицу,
строгие значения схемы и устойчивый signed `i64` key. Форматы всех scalar types,
открытие файла и исходные limits приведены в [контракте данных](data.md#sqlite).
Существующий `DataSource` продолжает хранить полный bounded snapshot; getters
и `DataSource::evaluate_query` никогда не вызывают SQLite.

## CRUD и конфликты

SQLite поддерживает `WriteCapabilities::transactional_crud()` и общий
`DataSource::write_batch(&[DataWrite])`. Вставка явно задаёт key и все начальные
значения; если key также отображается полем схемы, оба значения обязаны совпадать.
Обновление сравнивает ожидаемую ячейку, удаление — всю ожидаемую строку.
Все операции и чтение итогового снимка выполняются в одной immediate transaction.
Идентификаторы экранируются, значения и ключи передаются SQL-параметрами.

Перед записью адаптер проверяет текущий snapshot и весь последовательный пакет.
Внутри write lock выполняется точное сравнение typed значений; SQLite NOCASE
не скрывает изменение регистра, affinity не заменяет проверку storage class.
Итоговый снимок считывается и валидируется до commit. Его строки и поля должны
соответствовать запрошенному результату пакета. Trigger, изменяющий значения,
ключи или состав настроенной таблицы сверх пакета, вызывает rollback. Trigger
может изменять другие таблицы; эти действия участвуют в той же транзакции.

Constraint, ignored operation, invalid scalar, превышение limits и ошибка commit
(включая deferred foreign key) откатывают пакет. Принятые snapshot/query/revision
источника и параметры черновика сохраняются. Ожидание блокировки по-прежнему
250 мс; по истечении возвращается Adapter error. После освобождения блокировки
приложение может повторить тот же пакет, если ожидания остаются действительными.

Legacy `write_cell` сохраняет прежний контракт: проверяет записанную ячейку
и валидность итогового снимка. Пакетный контракт дополнительно проверяет весь
запрошенный результат настроенной таблицы. Schema changes не входят в CRUD.

## План выполнения

`SqliteAdapter::query_execution(&request)` проверяет AST и возвращает
`QueryExecution::{Pushdown, Local}` без I/O. Это план по текущей конфигурации;
само исполнение повторно проверяет schema/encoding в read transaction.
`QueryEngine::execute_provider(&mut adapter, &request)` — явный I/O API.

SQL поддерживает следующий профиль в базе с UTF-8 encoding:

| Возможность | SQL-профиль |
|---|---|
| Выражение | Field или Literal |
| Условия | Constant, Compare, IsNull, Not, All, Any |
| Сортировка | Field/Literal, оба направления, null first ascending / last descending |
| Типы сравнения и порядка | Bool, Integer, finite Real, Text, Date, Time, Binary; Null сохраняет трёхзначную логику |
| Decimal | IsNull допустим; сравнение и сортировка целиком local |
| Прочее | Арифметика, Lowercase, TextLength, Coalesce, TextMatch, grouping и aggregates целиком local |

Text сравнивается по BINARY с отключённой column affinity, независимо от
NOCASE/RTRIM колонки. В UTF-16 базе запрос выполняется локально. Date/Time имеют
каноническое TEXT-представление; численное сравнение Decimal не заменяется
лексикографическим. При равных sort terms всегда добавляется key ASC, включая
пустой order_by. Литерал не становится SQL-фрагментом или именем колонки.

`query_capabilities_for(request)` объявляет только поддержку конкретного полного
запроса. Неподдержанный терм переводит весь запрос в Local; частичного pushdown
нет. Ошибка выбранного SQL исполнения возвращается вызывающему коду, скрытого
повторного чтения нет. Смена SQLite schema cookie между планированием и открытием
read snapshot возвращает Conflict; новый explicit вызов планирует заново.

Непостраничный provider API сохраняет все snapshot bounds. До SQL исполнения
валидируются полный typed snapshot и scalar/working limits локального Query
Engine. SQL выполняет фильтрацию и порядок; такой путь не обещает сокращение
объёма загрузки или indexed-only обработку. Он сохраняет одинаковые ошибки
неверных данных, в том числе строк, скрытых фильтром.

## Постраничное чтение

`adapter.query_page(&request, offset, limit)` возвращает `SqliteQueryPage`:
`table`, `offset`, `total_rows`, `execution`. Это отдельная синхронная операция
чтения, без обновления DataSource и без UI callbacks. Страница содержит строки
после filter/order; count относится ко всему отфильтрованному набору.
Offset начинается с нуля, limit находится в `1..=DataLimits.max_rows`.
Переполнение диапазона отклоняется; offset после конца возвращает пустую таблицу
с корректным total_rows. Grouping/aggregates здесь отклоняются: для них служит
непостраничный Query Engine.

SQL-страница может читать таблицу больше `max_rows`; этот лимит ограничивает
возвращаемую страницу. Перед SELECT COUNT/LIMIT адаптер потоково проверяет
storage types, значения, уникальность ключей и выражения без загрузки всей
таблицы в память. Проверка охватывает также скрытые фильтром строки. Один
временный row и итоговая страница ограничены `max_bytes`, индивидуальные значения
— text/binary limits. SQL validation/count/page происходят в одной read
transaction, поэтому concurrent WAL writer не смешивает их снимки.

Для неподдержанного SQL AST применяется полный local load в обычных snapshot
limits, затем выбирается страница результата. Такая операция не позволяет
обойти `max_rows` или `max_bytes` полного локального снимка. Потоковая валидация
поддержанного пути сканирует таблицу: время работы не ограничено размером страницы,
SQLite может использовать собственный временный storage для сортировки.

Разные вызовы query_page могут видеть разные данные. Offset paging не хранит
транзакцию между вызовами и не обещает неизменный состав при concurrent insert/
delete. Приложение повторно загружает страницы после изменения источника.
Асинхронный worker, cancellation и cache pages находятся за границей адаптера.

Проверки с реальной SQLite покрывают scalar roundtrip, пакетный rollback,
конфликты и locks, parity SQL/local для null/type/order, UTF-16 fallback,
schema change, bounded pages и согласованность count/rows при WAL записи.
