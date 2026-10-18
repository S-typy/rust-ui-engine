# Geographic map canvas

`spatial::Map` owns a bounded geographic view, vector polylines, markers and an
application-managed raster tile cache. `Controls::map_view` mounts it into a
retained form. No network provider, credentials, external map data or download
policy is built into the control.

`GeoPoint` requires finite longitude in −180…180 degrees and latitude in
−85.0511287798066…85.0511287798066 degrees. Web Mercator and equirectangular
projections share this supported latitude interval. Longitude wraps across the
date line. `screen_point`, `geo_point`, pan, zoom and marker picking use the same
projection. `zoom_at` preserves the geographic position below its anchor except
at a clamped latitude limit. Zoom is bounded to 0…20.

Markers have stable IDs and labels. Polylines use stable IDs and finite
geographic vertices; lines split at the world-wrap discontinuity rather than
crossing the whole viewport. The maximum is 4096 markers, 1024 polylines and
16384 total line vertices. Layer replacement validates the entire candidate
before publishing it and keeps selection only while its ID still exists.

Raster tiles use Web Mercator XYZ coordinates. Equirectangular mode displays
vectors and markers without requesting an incompatible raster projection.
`request_tiles` returns at most 256 outstanding requests. Each request includes
provider ID, view generation, sequence and tile key. The application can dispatch
these requests on any executor. `complete_tile` accepts only an outstanding
exact token from the current generation. Late or duplicate completions are
rejected without changing accepted pixels. Accepted square RGBA bitmaps have
dimensions 128, 256 or 512. Existing image primitives upload/render them on GPU.

`MapTileResult::Missing` records a diagnostic and paints a missing-tile cell.
It is not retried on every frame; `retry_missing_tiles` explicitly makes it
eligible again. The default LRU capacity is 64 entries and can be set to 1…256,
with a separate hard 32 MiB pixel budget. Missing entries count toward the entry
limit. Provider/projection changes discard incompatible resources.

`cancel_requests` invalidates publication immediately and returns outstanding
tokens. Changes to the view also invalidate outstanding publication. Transport
owners can use `is_current_tile_request` and `is_current_service_request` to
cancel their own background work. Publication cancellation does not claim to
abort an application's socket or thread. The application owns timeout,
authentication, retry and rate-limit policy.

`MapServiceRequest` and `MapServiceResult` provide search and routing contracts.
New service requests supersede earlier ones; result kind, identity, layer bounds
and generation are validated atomically. `MapServices` is an optional dispatch/
cancel interface. Geocoding and route computation remain provider operations.

Provider ID and attribution are required. Attribution is painted in a footer
and included in the accessible description. Missing/pending/ready counts,
cache bytes and the last source error are exposed by `diagnostics`. The footer
does not select a marker hidden beneath it.

Primary click selects the topmost visible marker; drag pans. The wheel zooms
around its pointer, Control+arrows pans, Page Up/Down zooms and plain arrows
navigate markers. `table` returns descriptive records for an accessible
alternative. The example uses a synchronized ListBox with compact rounded
coordinates and full-precision accessible names, and a synthetic offline
tile grid, with explicit missing tiles, search and route fixtures:
`cargo run -p xaml-form -- --map`.

`Controls::map_tile_requests` and `complete_map_tile` deliver raster resources
without cancelling a live pan gesture. Other application replacements use the
atomic `edit_spatial` API. The current renderer draws polylines and point
markers; polygon fills, geographic labels collision management, clustering and
external network adapters are not included.
