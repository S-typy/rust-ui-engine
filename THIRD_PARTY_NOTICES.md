# Third-party notices

Snapshot: 2026-10-10. The project code is licensed under Apache-2.0. Dependencies
retain the licenses declared below. The table covers all 349 registry packages in
`Cargo.lock`, including packages for targets outside the desktop build.
It is not a list of libraries linked into every executable.

License declarations were read from the downloaded Cargo packages. Legacy `/`
expressions are shown as `OR`; the original declaration, archive checksum,
features and license file hashes are preserved in
[dependency-inventory.json](docs/public/dependency-inventory.json). `AND`
requirements are retained, including the additional MIT terms in `dpi` and the
Unicode terms in `unicode-ident`. Alternative LGPL, Unlicense or LLVM-exception
licenses in an `OR` expression are not required when a listed MIT/Apache
alternative is selected.

This source repository does not vendor the dependencies. Preserve the applicable
copyright notices and license texts when distributing a binary or vendored
sources. Platform SDKs, the Rust standard library, drivers and optional external
tools have separate distribution terms. This index and automated SPDX checks do
not constitute a complete binary redistribution review.

## Evidence and open items

- Direct dependency license texts were inspected in their published packages or
  exact source revisions. Taffy omits LICENSE from its crate archive; its MIT
  text is linked below at the revision recorded in `.cargo_vcs_info.json`.
- Exact-revision upstream evidence is linked for packages whose crate archives
  omit separate license texts. `r-efi` carries its licensing and attribution in
  `AUTHORS`. The inventory records these files and their hashes.
- `dispatch 0.2.0` declares MIT in its manifest; a separate license or copyright
  notice was not found in its package or source revision. Resolve attribution
  packaging before distributing a macOS binary.
- Current objc2 upstream licensing notes discuss the Apple SDK terms in addition
  to crate licenses. Review those terms for macOS binary distribution. See the
  exact-revision links in the inventory.
- AccessKit evidence includes its Chromium notice and AUTHORS file alongside
  MIT/Apache texts. Preserve applicable attribution beyond manifest SPDX labels.
- Windows clipboard transitives `clipboard-win` and `error-code` use BSL-1.0.
  The former's license was retrieved from its exact source revision because the
  crate archive omits it; the latter includes LICENSE. System fonts are discovered
  locally and are not bundled by this source repository.

## Resolved dependencies

| Crate | Version | Declared SPDX expression | License / source evidence |
|---|---|---|---|
| accesskit | 0.25.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_atspi_common | 0.21.0 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_consumer | 0.39.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_ios | 0.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_macos | 0.27.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_unix | 0.24.0 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_windows | 0.35.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| accesskit_winit | 0.34.1 | Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE-MIT), [LICENSE.chromium (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/LICENSE.chromium), [AUTHORS (upstream)](https://raw.githubusercontent.com/AccessKit/accesskit/ce8164ba92995cfa86005b6259115e08c8244253/AUTHORS) |
| ahash | 0.8.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/ahash/0.8.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/ahash/0.8.12/source/LICENSE-MIT) |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/allocator-api2/0.2.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/allocator-api2/0.2.21/source/LICENSE-MIT) |
| android-activity | 0.6.1 | MIT OR Apache-2.0 | [LICENSE](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE), [LICENSE-APACHE](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE-MIT) |
| android-properties | 0.2.2 | MIT | [LICENSE](https://docs.rs/crate/android-properties/0.2.2/source/LICENSE) |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/android_system_properties/0.1.6/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/android_system_properties/0.1.6/source/LICENSE-MIT) |
| arboard | 3.6.1 | MIT OR Apache-2.0 | [LICENSE-APACHE.txt](https://docs.rs/crate/arboard/3.6.1/source/LICENSE-APACHE.txt), [LICENSE-MIT.txt](https://docs.rs/crate/arboard/3.6.1/source/LICENSE-MIT.txt) |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/arrayvec/0.7.8/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/arrayvec/0.7.8/source/LICENSE-MIT) |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/as-raw-xcb-connection/1.0.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/as-raw-xcb-connection/1.0.1/source/LICENSE-MIT) |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/ash/0.38.0%2B1.3.281/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/ash/0.38.0%2B1.3.281/source/LICENSE-MIT) |
| async-broadcast | 0.7.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/async-broadcast/0.7.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-broadcast/0.7.2/source/LICENSE-MIT) |
| async-channel | 2.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-channel/2.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-channel/2.5.0/source/LICENSE-MIT) |
| async-executor | 1.14.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-executor/1.14.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-executor/1.14.0/source/LICENSE-MIT) |
| async-io | 2.6.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-io/2.6.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-io/2.6.0/source/LICENSE-MIT) |
| async-lock | 3.4.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-lock/3.4.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-lock/3.4.2/source/LICENSE-MIT) |
| async-process | 2.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-process/2.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-process/2.5.0/source/LICENSE-MIT) |
| async-recursion | 1.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/async-recursion/1.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-recursion/1.2.0/source/LICENSE-MIT) |
| async-signal | 0.2.14 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-signal/0.2.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-signal/0.2.14/source/LICENSE-MIT) |
| async-task | 4.7.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/async-task/4.7.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-task/4.7.1/source/LICENSE-MIT) |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/async-trait/0.1.92/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/async-trait/0.1.92/source/LICENSE-MIT) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-MIT), [LICENSE-THIRD-PARTY](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-THIRD-PARTY) |
| atspi | 0.29.0 | Apache-2.0 OR MIT | [LICENSE-APACHE2.txt](https://docs.rs/crate/atspi/0.29.0/source/LICENSE-APACHE2.txt), [LICENSE-MIT.txt](https://docs.rs/crate/atspi/0.29.0/source/LICENSE-MIT.txt) |
| atspi-common | 0.13.0 | Apache-2.0 OR MIT | [LICENSE-APACHE2.txt](https://docs.rs/crate/atspi-common/0.13.0/source/LICENSE-APACHE2.txt), [LICENSE-MIT.txt](https://docs.rs/crate/atspi-common/0.13.0/source/LICENSE-MIT.txt) |
| atspi-proxies | 0.13.0 | Apache-2.0 OR MIT | [LICENSE-APACHE2.txt](https://docs.rs/crate/atspi-proxies/0.13.0/source/LICENSE-APACHE2.txt), [LICENSE-MIT.txt](https://docs.rs/crate/atspi-proxies/0.13.0/source/LICENSE-MIT.txt) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/autocfg/1.5.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/autocfg/1.5.1/source/LICENSE-MIT) |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bit-set/0.10.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bit-set/0.10.0/source/LICENSE-MIT) |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bit-vec/0.9.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bit-vec/0.9.1/source/LICENSE-MIT) |
| bitflags | 1.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bitflags/1.3.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bitflags/1.3.2/source/LICENSE-MIT) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bitflags/2.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bitflags/2.13.2/source/LICENSE-MIT) |
| block2 | 0.5.1 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/4fc083f1c6d6784577e38b0ee8dbd344481e2fd2/LICENSE.txt) |
| block2 | 0.6.2 | MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/b4167b582b2f75f9a1be75495c41b765344fd03c/LICENSE.md) |
| blocking | 1.7.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/blocking/1.7.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/blocking/1.7.0/source/LICENSE-MIT) |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bumpalo/3.20.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bumpalo/3.20.3/source/LICENSE-MIT) |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-ZLIB) |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-ZLIB) |
| bytes | 1.12.1 | MIT | [LICENSE](https://docs.rs/crate/bytes/1.12.1/source/LICENSE) |
| calloop | 0.13.0 | MIT | [LICENSE.txt](https://docs.rs/crate/calloop/0.13.0/source/LICENSE.txt) |
| calloop-wayland-source | 0.3.0 | MIT | [LICENSE.txt](https://docs.rs/crate/calloop-wayland-source/0.3.0/source/LICENSE.txt) |
| cc | 1.6.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/cc/1.6.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/cc/1.6.0/source/LICENSE-MIT) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/cfg-if/1.0.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/cfg-if/1.0.5/source/LICENSE-MIT) |
| cfg_aliases | 0.2.2 | MIT | [LICENSE](https://docs.rs/crate/cfg_aliases/0.2.2/source/LICENSE), [NOTICES.md](https://docs.rs/crate/cfg_aliases/0.2.2/source/NOTICES.md) |
| clipboard-win | 5.4.1 | BSL-1.0 | [LICENSE (upstream)](https://raw.githubusercontent.com/DoumanAsh/clipboard-win/3b27cf2bfd1adcfa6e0264eb51c1025ddaf0f342/LICENSE) |
| codespan-reporting | 0.13.1 | Apache-2.0 | [LICENSE](https://docs.rs/crate/codespan-reporting/0.13.1/source/LICENSE) |
| combine | 4.6.8 | MIT | [LICENSE](https://docs.rs/crate/combine/4.6.8/source/LICENSE) |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/concurrent-queue/2.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/concurrent-queue/2.5.0/source/LICENSE-MIT) |
| core-foundation | 0.9.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/core-foundation/0.9.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/core-foundation/0.9.4/source/LICENSE-MIT) |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/core-foundation-sys/0.8.7/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/core-foundation-sys/0.8.7/source/LICENSE-MIT) |
| core-graphics | 0.23.2 | MIT OR Apache-2.0 | [COPYRIGHT](https://docs.rs/crate/core-graphics/0.23.2/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/core-graphics/0.23.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/core-graphics/0.23.2/source/LICENSE-MIT) |
| core-graphics-types | 0.1.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/core-graphics-types/0.1.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/core-graphics-types/0.1.3/source/LICENSE-MIT) |
| crossbeam-utils | 0.8.23 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/crossbeam-utils/0.8.23/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/crossbeam-utils/0.8.23/source/LICENSE-MIT) |
| crunchy | 0.2.4 | MIT | [LICENSE](https://docs.rs/crate/crunchy/0.2.4/source/LICENSE) |
| cursor-icon | 1.2.0 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE](https://docs.rs/crate/cursor-icon/1.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/cursor-icon/1.2.0/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/cursor-icon/1.2.0/source/LICENSE-ZLIB) |
| dispatch | 0.2.0 | MIT | [Manifest declaration only](https://docs.rs/crate/dispatch/0.2.0/source/Cargo.toml) |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/8852b424193ca41602281b3d7540d7c8ed51e49a/LICENSE.md) |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/displaydoc/0.2.7/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/displaydoc/0.2.7/source/LICENSE-MIT) |
| dlib | 0.5.3 | MIT | [LICENSE.txt](https://docs.rs/crate/dlib/0.5.3/source/LICENSE.txt) |
| document-features | 0.2.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/document-features/0.2.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/document-features/0.2.12/source/LICENSE-MIT) |
| downcast-rs | 1.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/downcast-rs/1.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/downcast-rs/1.2.1/source/LICENSE-MIT) |
| dpi | 0.1.2 | Apache-2.0 AND MIT | [LICENSE](https://docs.rs/crate/dpi/0.1.2/source/LICENSE), [LICENSE-LIBM-MIT](https://docs.rs/crate/dpi/0.1.2/source/LICENSE-LIBM-MIT) |
| endi | 1.1.1 | MIT | [LICENSE-MIT](https://docs.rs/crate/endi/1.1.1/source/LICENSE-MIT) |
| enumflags2 | 0.7.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/enumflags2/0.7.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/enumflags2/0.7.12/source/LICENSE-MIT) |
| enumflags2_derive | 0.7.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/enumflags2_derive/0.7.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/enumflags2_derive/0.7.12/source/LICENSE-MIT) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/equivalent/1.0.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/equivalent/1.0.2/source/LICENSE-MIT) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/errno/0.3.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/errno/0.3.14/source/LICENSE-MIT) |
| error-code | 3.4.0 | BSL-1.0 | [LICENSE](https://docs.rs/crate/error-code/3.4.0/source/LICENSE) |
| event-listener | 5.4.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/event-listener/5.4.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/event-listener/5.4.2/source/LICENSE-MIT) |
| event-listener-strategy | 0.5.4 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/event-listener-strategy/0.5.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/event-listener-strategy/0.5.4/source/LICENSE-MIT) |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/fastrand/2.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/fastrand/2.5.0/source/LICENSE-MIT) |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/find-msvc-tools/0.1.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/find-msvc-tools/0.1.14/source/LICENSE-MIT) |
| fixedbitset | 0.5.7 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/fixedbitset/0.5.7/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/fixedbitset/0.5.7/source/LICENSE-MIT) |
| foldhash | 0.1.5 | Zlib | [LICENSE](https://docs.rs/crate/foldhash/0.1.5/source/LICENSE) |
| foldhash | 0.2.0 | Zlib | [LICENSE](https://docs.rs/crate/foldhash/0.2.0/source/LICENSE) |
| font-types | 0.12.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/font-types/0.12.6/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/font-types/0.12.6/source/LICENSE-MIT) |
| fontique | 0.11.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/fontique/0.11.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/fontique/0.11.1/source/LICENSE-MIT) |
| foreign-types | 0.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types/0.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types/0.5.0/source/LICENSE-MIT) |
| foreign-types-macros | 0.2.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types-macros/0.2.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types-macros/0.2.4/source/LICENSE-MIT) |
| foreign-types-shared | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types-shared/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types-shared/0.3.1/source/LICENSE-MIT) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-core/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-core/0.3.34/source/LICENSE-MIT) |
| futures-io | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-io/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-io/0.3.34/source/LICENSE-MIT) |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/futures-lite/2.6.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-lite/2.6.1/source/LICENSE-MIT), [LICENSE-THIRD-PARTY](https://docs.rs/crate/futures-lite/2.6.1/source/LICENSE-THIRD-PARTY) |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-macro/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-macro/0.3.34/source/LICENSE-MIT) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-task/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-task/0.3.34/source/LICENSE-MIT) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-util/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-util/0.3.34/source/LICENSE-MIT) |
| gethostname | 1.1.0 | Apache-2.0 | [LICENSE](https://docs.rs/crate/gethostname/1.1.0/source/LICENSE) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/getrandom/0.3.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/getrandom/0.3.4/source/LICENSE-MIT) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/getrandom/0.4.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/getrandom/0.4.3/source/LICENSE-MIT) |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/gpu-allocator/0.28.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/gpu-allocator/0.28.0/source/LICENSE-MIT) |
| half | 2.7.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/half/2.7.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/half/2.7.1/source/LICENSE-MIT) |
| harfrust | 0.12.0 | MIT | [LICENSE](https://docs.rs/crate/harfrust/0.12.0/source/LICENSE) |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hashbrown/0.15.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hashbrown/0.15.5/source/LICENSE-MIT) |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hashbrown/0.16.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hashbrown/0.16.1/source/LICENSE-MIT) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hashbrown/0.17.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hashbrown/0.17.1/source/LICENSE-MIT) |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hermit-abi/0.5.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hermit-abi/0.5.3/source/LICENSE-MIT) |
| hex | 0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hex/0.4.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hex/0.4.3/source/LICENSE-MIT) |
| icu_collections | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_collections/2.3.0/source/LICENSE) |
| icu_locale_core | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_locale_core/2.3.0/source/LICENSE) |
| icu_locale_fallback | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_locale_fallback/2.3.0/source/LICENSE) |
| icu_locale_fallback_data | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_locale_fallback_data/2.3.0/source/LICENSE) |
| icu_normalizer | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_normalizer/2.3.0/source/LICENSE) |
| icu_normalizer_data | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_normalizer_data/2.3.0/source/LICENSE) |
| icu_properties | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_properties/2.3.0/source/LICENSE) |
| icu_properties_data | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_properties_data/2.3.0/source/LICENSE) |
| icu_provider | 2.3.1 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_provider/2.3.1/source/LICENSE) |
| icu_segmenter | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_segmenter/2.3.0/source/LICENSE) |
| icu_segmenter_data | 2.3.0 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/icu_segmenter_data/2.3.0/source/LICENSE) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/indexmap/2.14.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/indexmap/2.14.2/source/LICENSE-MIT) |
| jni | 0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/jni-rs/jni-rs/5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/jni-rs/jni-rs/5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7/LICENSE-MIT) |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/jni-rs/jni-rs/33045a124105c939d1e2cbdcb5a39e5d868ffa03/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/jni-rs/jni-rs/33045a124105c939d1e2cbdcb5a39e5d868ffa03/LICENSE-MIT) |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jni-sys/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jni-sys/0.3.1/source/LICENSE-MIT) |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jni-sys/0.4.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jni-sys/0.4.1/source/LICENSE-MIT) |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/jni-rs/jni-sys/64d77b7a5f119d7b55b4e2c169a4668067ff59e6/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/jni-rs/jni-sys/64d77b7a5f119d7b55b4e2c169a4668067ff59e6/LICENSE-MIT) |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jobserver/0.1.35/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jobserver/0.1.35/source/LICENSE-MIT) |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/js-sys/0.3.106/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/js-sys/0.3.106/source/LICENSE-MIT) |
| libc | 0.2.190 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/libc/0.2.190/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/libc/0.2.190/source/LICENSE-MIT) |
| libloading | 0.8.9 | ISC | [LICENSE](https://docs.rs/crate/libloading/0.8.9/source/LICENSE) |
| libm | 0.2.16 | MIT | [LICENSE.txt](https://docs.rs/crate/libm/0.2.16/source/LICENSE.txt) |
| libredox | 0.1.25 | MIT | [LICENSE](https://docs.rs/crate/libredox/0.1.25/source/LICENSE) |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/linebender_resource_handle/0.1.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/linebender_resource_handle/0.1.1/source/LICENSE-MIT) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/linux-raw-sys/0.12.1/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-MIT) |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/linux-raw-sys/0.4.15/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-MIT) |
| litemap | 0.8.3 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/litemap/0.8.3/source/LICENSE) |
| litrs | 1.0.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/litrs/1.0.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/litrs/1.0.0/source/LICENSE-MIT) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/lock_api/0.4.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/lock_api/0.4.14/source/LICENSE-MIT) |
| log | 0.4.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/log/0.4.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/log/0.4.34/source/LICENSE-MIT) |
| memchr | 2.8.3 | Unlicense OR MIT | [COPYING](https://docs.rs/crate/memchr/2.8.3/source/COPYING), [LICENSE-MIT](https://docs.rs/crate/memchr/2.8.3/source/LICENSE-MIT) |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/memmap2/0.9.11/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/memmap2/0.9.11/source/LICENSE-MIT) |
| memoffset | 0.9.1 | MIT | [LICENSE](https://docs.rs/crate/memoffset/0.9.1/source/LICENSE) |
| naga | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/naga/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/naga/30.0.1/source/LICENSE.MIT) |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/naga-types/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/naga-types/30.0.1/source/LICENSE.MIT) |
| ndk | 0.9.0 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-MIT) |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/rust-windowing/android-ndk-rs/10f2ba388fca20f7349996ebae26ccda7a6fda5c/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/rust-windowing/android-ndk-rs/10f2ba388fca20f7349996ebae26ccda7a6fda5c/LICENSE-MIT) |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-MIT) |
| nom | 8.0.0 | MIT | [LICENSE](https://docs.rs/crate/nom/8.0.0/source/LICENSE) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num-traits/0.2.19/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/num-traits/0.2.19/source/LICENSE-MIT) |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-MIT) |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-MIT) |
| objc-sys | 0.3.5 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/4fc083f1c6d6784577e38b0ee8dbd344481e2fd2/LICENSE.txt) |
| objc2 | 0.5.3 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/5237cbf081d04ef75a1f6a207b4018e7c1ae7438/LICENSE.txt) |
| objc2 | 0.6.5 | MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/d7d2fa23ceaa5e6096c923b081040e5d81b3b9df/LICENSE.md) |
| objc2-app-kit | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-app-kit | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-cloud-kit | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-contacts | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-data | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-core-image | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-location | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-text | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-encode | 4.1.0 | MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/8d214f5477365ffcbcbb7de058c86ed9a518efb7/LICENSE.md) |
| objc2-foundation | 0.2.2 | MIT | [src/copying.rs](https://docs.rs/crate/objc2-foundation/0.2.2/source/src/copying.rs), [src/tests/copying.rs](https://docs.rs/crate/objc2-foundation/0.2.2/source/src/tests/copying.rs) |
| objc2-foundation | 0.3.2 | MIT | [src/copying.rs](https://docs.rs/crate/objc2-foundation/0.3.2/source/src/copying.rs), [src/tests/copying.rs](https://docs.rs/crate/objc2-foundation/0.3.2/source/src/tests/copying.rs) |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-link-presentation | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-metal | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-quartz-core | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE.md (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-symbols | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-ui-kit | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-uniform-type-identifiers | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-user-notifications | 0.2.2 | MIT | [LICENSE.txt (upstream)](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/once_cell/1.21.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/once_cell/1.21.4/source/LICENSE-MIT) |
| orbclient | 0.3.55 | MIT | [LICENSE](https://docs.rs/crate/orbclient/0.3.55/source/LICENSE) |
| ordered-float | 5.5.0 | MIT | [LICENSE-MIT](https://docs.rs/crate/ordered-float/5.5.0/source/LICENSE-MIT) |
| ordered-stream | 0.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/ordered-stream/0.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/ordered-stream/0.2.0/source/LICENSE-MIT) |
| os_pipe | 1.2.3 | MIT | [LICENSE](https://docs.rs/crate/os_pipe/1.2.3/source/LICENSE) |
| parking | 2.2.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/parking/2.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parking/2.2.1/source/LICENSE-MIT), [LICENSE-THIRD-PARTY](https://docs.rs/crate/parking/2.2.1/source/LICENSE-THIRD-PARTY) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/parking_lot/0.12.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parking_lot/0.12.5/source/LICENSE-MIT) |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/parking_lot_core/0.9.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parking_lot_core/0.9.12/source/LICENSE-MIT) |
| parlance | 0.1.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/parlance/0.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parlance/0.1.0/source/LICENSE-MIT) |
| parley | 0.11.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/parley/0.11.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parley/0.11.1/source/LICENSE-MIT) |
| parley_data | 0.11.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/parley_data/0.11.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parley_data/0.11.1/source/LICENSE-MIT) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/percent-encoding/2.3.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/percent-encoding/2.3.2/source/LICENSE-MIT) |
| petgraph | 0.8.3 | MIT OR Apache-2.0 | [assets/images/LICENSE.md](https://docs.rs/crate/petgraph/0.8.3/source/assets/images/LICENSE.md), [LICENSE-APACHE](https://docs.rs/crate/petgraph/0.8.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/petgraph/0.8.3/source/LICENSE-MIT) |
| phf | 0.14.0 | MIT | [LICENSE](https://docs.rs/crate/phf/0.14.0/source/LICENSE) |
| phf_generator | 0.14.0 | MIT | [LICENSE](https://docs.rs/crate/phf_generator/0.14.0/source/LICENSE) |
| phf_macros | 0.14.0 | MIT | [LICENSE](https://docs.rs/crate/phf_macros/0.14.0/source/LICENSE) |
| phf_shared | 0.14.0 | MIT | [LICENSE](https://docs.rs/crate/phf_shared/0.14.0/source/LICENSE) |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project/1.1.13/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project/1.1.13/source/LICENSE-MIT) |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project-internal/1.1.13/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project-internal/1.1.13/source/LICENSE-MIT) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project-lite/0.2.17/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project-lite/0.2.17/source/LICENSE-MIT) |
| piper | 0.2.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/piper/0.2.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/piper/0.2.5/source/LICENSE-MIT) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/pkg-config/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pkg-config/0.3.34/source/LICENSE-MIT) |
| plain | 0.2.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/plain/0.2.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/plain/0.2.3/source/LICENSE-MIT) |
| polling | 3.11.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/polling/3.11.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/polling/3.11.0/source/LICENSE-MIT) |
| pollster | 0.4.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pollster/0.4.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pollster/0.4.0/source/LICENSE-MIT) |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/portable-atomic/1.15.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/portable-atomic/1.15.0/source/LICENSE-MIT) |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/portable-atomic-util/0.2.8/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/portable-atomic-util/0.2.8/source/LICENSE-MIT) |
| potential_utf | 0.1.6 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/potential_utf/0.1.6/source/LICENSE) |
| presser | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/presser/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/presser/0.3.1/source/LICENSE-MIT) |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/proc-macro-crate/3.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/proc-macro-crate/3.5.0/source/LICENSE-MIT) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/proc-macro2/1.0.107/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/proc-macro2/1.0.107/source/LICENSE-MIT) |
| profiling | 1.0.18 | MIT OR Apache-2.0 | [LICENSE-APACHE (upstream)](https://raw.githubusercontent.com/aclysma/profiling/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-APACHE), [LICENSE-MIT (upstream)](https://raw.githubusercontent.com/aclysma/profiling/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-MIT) |
| quick-xml | 0.41.0 | MIT | [LICENSE-MIT.md](https://docs.rs/crate/quick-xml/0.41.0/source/LICENSE-MIT.md) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/quote/1.0.47/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/quote/1.0.47/source/LICENSE-MIT) |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [AUTHORS](https://docs.rs/crate/r-efi/5.3.0/source/AUTHORS), [AUTHORS (upstream)](https://raw.githubusercontent.com/r-efi/r-efi/97b55bed1c2c91dcbf787674849f05337ff80b33/AUTHORS) |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [AUTHORS](https://docs.rs/crate/r-efi/6.0.0/source/AUTHORS), [AUTHORS (upstream)](https://raw.githubusercontent.com/r-efi/r-efi/7e1b0322d31d625f81a5656096330934f9cd835d/AUTHORS) |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/range-alloc/0.1.5/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/range-alloc/0.1.5/source/LICENSE.MIT) |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-APACHE.md), [LICENSE-MIT.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-MIT.md), [LICENSE-ZLIB.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-ZLIB.md) |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/raw-window-metal/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/raw-window-metal/1.1.0/source/LICENSE-MIT) |
| read-fonts | 0.41.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/read-fonts/0.41.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/read-fonts/0.41.0/source/LICENSE-MIT) |
| redox_syscall | 0.4.1 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.4.1/source/LICENSE) |
| redox_syscall | 0.5.18 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.5.18/source/LICENSE) |
| redox_syscall | 0.9.4 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.9.4/source/LICENSE) |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/renderdoc-sys/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/renderdoc-sys/1.1.0/source/LICENSE-MIT) |
| roxmltree | 0.21.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/roxmltree/0.21.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/roxmltree/0.21.1/source/LICENSE-MIT) |
| rustc-hash | 1.1.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/rustc-hash/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/rustc-hash/1.1.0/source/LICENSE-MIT) |
| rustc_version | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/rustc_version/0.4.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/rustc_version/0.4.1/source/LICENSE-MIT) |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/rustix/0.38.44/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/rustix/0.38.44/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/rustix/0.38.44/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/rustix/0.38.44/source/LICENSE-MIT) |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/rustix/1.1.5/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/rustix/1.1.5/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/rustix/1.1.5/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/rustix/1.1.5/source/LICENSE-MIT) |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/rustversion/1.0.23/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/rustversion/1.0.23/source/LICENSE-MIT) |
| same-file | 1.0.6 | Unlicense OR MIT | [COPYING](https://docs.rs/crate/same-file/1.0.6/source/COPYING), [LICENSE-MIT](https://docs.rs/crate/same-file/1.0.6/source/LICENSE-MIT) |
| scoped-tls | 1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/scoped-tls/1.0.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/scoped-tls/1.0.1/source/LICENSE-MIT) |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/scopeguard/1.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/scopeguard/1.2.0/source/LICENSE-MIT) |
| semver | 1.0.28 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/semver/1.0.28/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/semver/1.0.28/source/LICENSE-MIT) |
| serde | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/serde/1.0.229/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/serde/1.0.229/source/LICENSE-MIT) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/serde_core/1.0.229/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/serde_core/1.0.229/source/LICENSE-MIT) |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/serde_derive/1.0.229/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/serde_derive/1.0.229/source/LICENSE-MIT) |
| serde_repr | 0.1.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/serde_repr/0.1.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/serde_repr/0.1.21/source/LICENSE-MIT) |
| shlex | 2.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/shlex/2.0.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/shlex/2.0.1/source/LICENSE-MIT) |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/signal-hook-registry/1.4.8/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/signal-hook-registry/1.4.8/source/LICENSE-MIT) |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/simd_cesu8/1.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/simd_cesu8/1.2.0/source/LICENSE-MIT) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [LICENSE-Apache](https://docs.rs/crate/simdutf8/0.1.5/source/LICENSE-Apache), [LICENSE-MIT](https://docs.rs/crate/simdutf8/0.1.5/source/LICENSE-MIT) |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | [COPYING](https://docs.rs/crate/siphasher/1.0.4/source/COPYING), [LICENSE-APACHE](https://docs.rs/crate/siphasher/1.0.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/siphasher/1.0.4/source/LICENSE-MIT) |
| skrifa | 0.44.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/skrifa/0.44.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/skrifa/0.44.0/source/LICENSE-MIT) |
| slab | 0.4.12 | MIT | [LICENSE](https://docs.rs/crate/slab/0.4.12/source/LICENSE) |
| slotmap | 1.1.1 | Zlib | [LICENSE](https://docs.rs/crate/slotmap/1.1.1/source/LICENSE) |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/smallvec/1.16.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/smallvec/1.16.2/source/LICENSE-MIT) |
| smithay-client-toolkit | 0.19.2 | MIT | [LICENSE.txt](https://docs.rs/crate/smithay-client-toolkit/0.19.2/source/LICENSE.txt) |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/smol_str/0.2.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/smol_str/0.2.2/source/LICENSE-MIT) |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | [LICENSE (upstream)](https://raw.githubusercontent.com/gfx-rs/rspirv/8afc3d0ac8e158128cd1410bb2e4b4c26ab11bb4/LICENSE) |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/stable_deref_trait/1.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/stable_deref_trait/1.2.1/source/LICENSE-MIT) |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/static_assertions/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/static_assertions/1.1.0/source/LICENSE-MIT) |
| swash | 0.2.10 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/swash/0.2.10/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/swash/0.2.10/source/LICENSE-MIT) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/syn/2.0.119/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/syn/2.0.119/source/LICENSE-MIT) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/syn/3.0.6/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/syn/3.0.6/source/LICENSE-MIT) |
| synstructure | 0.14.0 | MIT | [LICENSE](https://docs.rs/crate/synstructure/0.14.0/source/LICENSE) |
| taffy | 0.14.0 | MIT | [LICENSE (upstream)](https://raw.githubusercontent.com/DioxusLabs/taffy/77f385683c1d698c91a23a259f87fdddf26925fb/LICENSE) |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/tempfile/3.27.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/tempfile/3.27.0/source/LICENSE-MIT) |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror/1.0.69/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror/1.0.69/source/LICENSE-MIT) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror/2.0.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror/2.0.21/source/LICENSE-MIT) |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror-impl/1.0.69/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror-impl/1.0.69/source/LICENSE-MIT) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror-impl/2.0.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror-impl/2.0.21/source/LICENSE-MIT) |
| tinystr | 0.8.4 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/tinystr/0.8.4/source/LICENSE) |
| tokio | 1.53.2 | MIT | [LICENSE](https://docs.rs/crate/tokio/1.53.2/source/LICENSE) |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_datetime/1.1.2%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_datetime/1.1.2%2Bspec-1.1.0/source/LICENSE-MIT) |
| toml_edit | 0.25.17+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_edit/0.25.17%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_edit/0.25.17%2Bspec-1.1.0/source/LICENSE-MIT) |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_parser/1.1.5%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_parser/1.1.5%2Bspec-1.1.0/source/LICENSE-MIT) |
| tracing | 0.1.44 | MIT | [LICENSE](https://docs.rs/crate/tracing/0.1.44/source/LICENSE) |
| tracing-attributes | 0.1.31 | MIT | [LICENSE](https://docs.rs/crate/tracing-attributes/0.1.31/source/LICENSE) |
| tracing-core | 0.1.36 | MIT | [LICENSE](https://docs.rs/crate/tracing-core/0.1.36/source/LICENSE), [src/spin/LICENSE](https://docs.rs/crate/tracing-core/0.1.36/source/src/spin/LICENSE) |
| tree_magic_mini | 3.2.2 | MIT | [LICENSE](https://docs.rs/crate/tree_magic_mini/3.2.2/source/LICENSE) |
| uds_windows | 1.2.1 | MIT | [LICENSE](https://docs.rs/crate/uds_windows/1.2.1/source/LICENSE) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-MIT), [LICENSE-UNICODE](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-UNICODE) |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | [COPYRIGHT](https://docs.rs/crate/unicode-segmentation/1.13.3/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/unicode-segmentation/1.13.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-segmentation/1.13.3/source/LICENSE-MIT) |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | [COPYRIGHT](https://docs.rs/crate/unicode-width/0.2.2/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/unicode-width/0.2.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-width/0.2.2/source/LICENSE-MIT) |
| utf8_iter | 1.0.4 | Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/utf8_iter/1.0.4/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/utf8_iter/1.0.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/utf8_iter/1.0.4/source/LICENSE-MIT) |
| uuid | 1.27.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/uuid/1.27.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/uuid/1.27.0/source/LICENSE-MIT) |
| version_check | 0.9.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/version_check/0.9.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/version_check/0.9.5/source/LICENSE-MIT) |
| walkdir | 2.5.0 | Unlicense OR MIT | [COPYING](https://docs.rs/crate/walkdir/2.5.0/source/COPYING), [LICENSE-MIT](https://docs.rs/crate/walkdir/2.5.0/source/LICENSE-MIT) |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/wasip2/1.0.4%2Bwasi-0.2.12/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/wasip2/1.0.4%2Bwasi-0.2.12/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/wasip2/1.0.4%2Bwasi-0.2.12/source/LICENSE-MIT) |
| wasm-bindgen | 0.2.129 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wasm-bindgen/0.2.129/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wasm-bindgen/0.2.129/source/LICENSE-MIT) |
| wasm-bindgen-futures | 0.4.79 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wasm-bindgen-futures/0.4.79/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wasm-bindgen-futures/0.4.79/source/LICENSE-MIT) |
| wasm-bindgen-macro | 0.2.129 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wasm-bindgen-macro/0.2.129/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wasm-bindgen-macro/0.2.129/source/LICENSE-MIT) |
| wasm-bindgen-macro-support | 0.2.129 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wasm-bindgen-macro-support/0.2.129/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wasm-bindgen-macro-support/0.2.129/source/LICENSE-MIT) |
| wasm-bindgen-shared | 0.2.129 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wasm-bindgen-shared/0.2.129/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wasm-bindgen-shared/0.2.129/source/LICENSE-MIT) |
| wayland-backend | 0.3.17 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-backend/0.3.17/source/LICENSE.txt) |
| wayland-client | 0.31.15 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-client/0.31.15/source/LICENSE.txt) |
| wayland-csd-frame | 0.3.0 | MIT | [LICENSE](https://docs.rs/crate/wayland-csd-frame/0.3.0/source/LICENSE) |
| wayland-cursor | 0.31.14 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-cursor/0.31.14/source/LICENSE.txt) |
| wayland-protocols | 0.32.13 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-protocols/0.32.13/source/LICENSE.txt), [protocols/COPYING](https://docs.rs/crate/wayland-protocols/0.32.13/source/protocols/COPYING) |
| wayland-protocols-plasma | 0.3.12 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-protocols-plasma/0.3.12/source/LICENSE.txt), [plasma-wayland-protocols/COPYING.LIB](https://docs.rs/crate/wayland-protocols-plasma/0.3.12/source/plasma-wayland-protocols/COPYING.LIB) |
| wayland-protocols-wlr | 0.3.12 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-protocols-wlr/0.3.12/source/LICENSE.txt) |
| wayland-scanner | 0.31.11 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-scanner/0.31.11/source/LICENSE.txt) |
| wayland-sys | 0.31.11 | MIT | [LICENSE.txt](https://docs.rs/crate/wayland-sys/0.31.11/source/LICENSE.txt) |
| web-sys | 0.3.106 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/web-sys/0.3.106/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/web-sys/0.3.106/source/LICENSE-MIT) |
| web-time | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/web-time/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/web-time/1.1.0/source/LICENSE-MIT) |
| wgpu | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu/30.0.1/source/LICENSE.MIT) |
| wgpu-core | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-core/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-core/30.0.1/source/LICENSE.MIT) |
| wgpu-core-deps-apple | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-core-deps-apple/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-core-deps-apple/30.0.1/source/LICENSE.MIT) |
| wgpu-core-deps-windows-linux-android | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-core-deps-windows-linux-android/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-core-deps-windows-linux-android/30.0.1/source/LICENSE.MIT) |
| wgpu-hal | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-hal/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-hal/30.0.1/source/LICENSE.MIT) |
| wgpu-naga-bridge | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-naga-bridge/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-naga-bridge/30.0.1/source/LICENSE.MIT) |
| wgpu-types | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/wgpu-types/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/wgpu-types/30.0.1/source/LICENSE.MIT) |
| winapi-util | 0.1.11 | Unlicense OR MIT | [COPYING](https://docs.rs/crate/winapi-util/0.1.11/source/COPYING), [LICENSE-MIT](https://docs.rs/crate/winapi-util/0.1.11/source/LICENSE-MIT) |
| windows | 0.62.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows/0.62.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows/0.62.2/source/license-mit) |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-collections/0.3.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-collections/0.3.2/source/license-mit) |
| windows-core | 0.62.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-core/0.62.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-core/0.62.2/source/license-mit) |
| windows-future | 0.3.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-future/0.3.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-future/0.3.2/source/license-mit) |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-implement/0.60.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-implement/0.60.2/source/license-mit) |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-interface/0.59.3/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-interface/0.59.3/source/license-mit) |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-link/0.2.1/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-link/0.2.1/source/license-mit) |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-numerics/0.3.1/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-numerics/0.3.1/source/license-mit) |
| windows-result | 0.4.1 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-result/0.4.1/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-result/0.4.1/source/license-mit) |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-strings/0.5.1/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-strings/0.5.1/source/license-mit) |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-sys/0.52.0/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-sys/0.52.0/source/license-mit) |
| windows-sys | 0.59.0 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-sys/0.59.0/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-sys/0.59.0/source/license-mit) |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-sys/0.61.2/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-sys/0.61.2/source/license-mit) |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-targets/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-targets/0.52.6/source/license-mit) |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows-threading/0.2.1/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows-threading/0.2.1/source/license-mit) |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_aarch64_gnullvm/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_aarch64_gnullvm/0.52.6/source/license-mit) |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_aarch64_msvc/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_aarch64_msvc/0.52.6/source/license-mit) |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_i686_gnu/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_i686_gnu/0.52.6/source/license-mit) |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_i686_gnullvm/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_i686_gnullvm/0.52.6/source/license-mit) |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_i686_msvc/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_i686_msvc/0.52.6/source/license-mit) |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_x86_64_gnu/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_x86_64_gnu/0.52.6/source/license-mit) |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_x86_64_gnullvm/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_x86_64_gnullvm/0.52.6/source/license-mit) |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | [license-apache-2.0](https://docs.rs/crate/windows_x86_64_msvc/0.52.6/source/license-apache-2.0), [license-mit](https://docs.rs/crate/windows_x86_64_msvc/0.52.6/source/license-mit) |
| winit | 0.30.13 | Apache-2.0 | [LICENSE](https://docs.rs/crate/winit/0.30.13/source/LICENSE) |
| winnow | 1.0.4 | MIT | [LICENSE-MIT](https://docs.rs/crate/winnow/1.0.4/source/LICENSE-MIT) |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/wit-bindgen/0.57.1/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/wit-bindgen/0.57.1/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/wit-bindgen/0.57.1/source/LICENSE-MIT) |
| wl-clipboard-rs | 0.9.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/wl-clipboard-rs/0.9.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/wl-clipboard-rs/0.9.4/source/LICENSE-MIT) |
| writeable | 0.6.4 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/writeable/0.6.4/source/LICENSE) |
| x11-dl | 2.21.0 | MIT | [LICENSE-MIT](https://docs.rs/crate/x11-dl/2.21.0/source/LICENSE-MIT) |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/x11rb/0.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/x11rb/0.13.2/source/LICENSE-MIT) |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/x11rb-protocol/0.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/x11rb-protocol/0.13.2/source/LICENSE-MIT) |
| xcursor | 0.3.11 | MIT | [LICENSE](https://docs.rs/crate/xcursor/0.3.11/source/LICENSE) |
| xkbcommon-dl | 0.4.2 | MIT | [LICENSE](https://docs.rs/crate/xkbcommon-dl/0.4.2/source/LICENSE) |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-ZLIB) |
| yazi | 0.2.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/yazi/0.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/yazi/0.2.1/source/LICENSE-MIT) |
| yeslogic-fontconfig-sys | 6.0.1 | MIT | [LICENSE](https://docs.rs/crate/yeslogic-fontconfig-sys/6.0.1/source/LICENSE) |
| yoke | 0.8.3 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/yoke/0.8.3/source/LICENSE) |
| yoke-derive | 0.8.4 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/yoke-derive/0.8.4/source/LICENSE) |
| zbus | 5.19.0 | MIT | [LICENSE](https://docs.rs/crate/zbus/5.19.0/source/LICENSE) |
| zbus-lockstep | 0.5.2 | MIT | [LICENSE-MIT](https://docs.rs/crate/zbus-lockstep/0.5.2/source/LICENSE-MIT) |
| zbus-lockstep-macros | 0.5.2 | MIT | [LICENSE-MIT](https://docs.rs/crate/zbus-lockstep-macros/0.5.2/source/LICENSE-MIT) |
| zbus_macros | 5.19.0 | MIT | [LICENSE](https://docs.rs/crate/zbus_macros/5.19.0/source/LICENSE) |
| zbus_names | 4.3.4 | MIT | [LICENSE](https://docs.rs/crate/zbus_names/4.3.4/source/LICENSE) |
| zbus_xml | 5.2.1 | MIT | [LICENSE](https://docs.rs/crate/zbus_xml/5.2.1/source/LICENSE) |
| zcheapstr | 1.1.0 | MIT | [LICENSE](https://docs.rs/crate/zcheapstr/1.1.0/source/LICENSE) |
| zeno | 0.3.3 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/zeno/0.3.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/zeno/0.3.3/source/LICENSE-MIT) |
| zerocopy | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-MIT) |
| zerocopy-derive | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-MIT) |
| zerofrom | 0.1.8 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/zerofrom/0.1.8/source/LICENSE) |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/zerofrom-derive/0.1.8/source/LICENSE) |
| zerotrie | 0.2.5 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/zerotrie/0.2.5/source/LICENSE) |
| zerovec | 0.11.8 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/zerovec/0.11.8/source/LICENSE) |
| zerovec-derive | 0.11.6 | Unicode-3.0 | [LICENSE](https://docs.rs/crate/zerovec-derive/0.11.6/source/LICENSE) |
| zvariant | 5.15.0 | MIT | [LICENSE](https://docs.rs/crate/zvariant/5.15.0/source/LICENSE) |
| zvariant_derive | 5.15.0 | MIT | [LICENSE](https://docs.rs/crate/zvariant_derive/5.15.0/source/LICENSE) |
| zvariant_utils | 4.2.0 | MIT | [LICENSE](https://docs.rs/crate/zvariant_utils/4.2.0/source/LICENSE) |
