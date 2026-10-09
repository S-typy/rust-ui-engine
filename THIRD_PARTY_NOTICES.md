# Third-party notices

Snapshot: 2026-10-09. The project code is licensed under Apache-2.0. Dependencies
retain the licenses declared below. The table covers all 239 registry packages in
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

## Resolved dependencies

| Crate | Version | Declared SPDX expression | License / source evidence |
|---|---|---|---|
| ahash | 0.8.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/ahash/0.8.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/ahash/0.8.12/source/LICENSE-MIT) |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/allocator-api2/0.2.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/allocator-api2/0.2.21/source/LICENSE-MIT) |
| android-activity | 0.6.1 | MIT OR Apache-2.0 | [LICENSE](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE), [LICENSE-APACHE](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/android-activity/0.6.1/source/LICENSE-MIT) |
| android-properties | 0.2.2 | MIT | [LICENSE](https://docs.rs/crate/android-properties/0.2.2/source/LICENSE) |
| android_system_properties | 0.1.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/android_system_properties/0.1.6/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/android_system_properties/0.1.6/source/LICENSE-MIT) |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/arrayvec/0.7.8/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/arrayvec/0.7.8/source/LICENSE-MIT) |
| as-raw-xcb-connection | 1.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/as-raw-xcb-connection/1.0.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/as-raw-xcb-connection/1.0.1/source/LICENSE-MIT) |
| ash | 0.38.0+1.3.281 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/ash/0.38.0%2B1.3.281/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/ash/0.38.0%2B1.3.281/source/LICENSE-MIT) |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-MIT), [LICENSE-THIRD-PARTY](https://docs.rs/crate/atomic-waker/1.1.2/source/LICENSE-THIRD-PARTY) |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/autocfg/1.5.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/autocfg/1.5.1/source/LICENSE-MIT) |
| bit-set | 0.10.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bit-set/0.10.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bit-set/0.10.0/source/LICENSE-MIT) |
| bit-vec | 0.9.1 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bit-vec/0.9.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bit-vec/0.9.1/source/LICENSE-MIT) |
| bitflags | 1.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bitflags/1.3.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bitflags/1.3.2/source/LICENSE-MIT) |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bitflags/2.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bitflags/2.13.2/source/LICENSE-MIT) |
| block2 | 0.5.1 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/4fc083f1c6d6784577e38b0ee8dbd344481e2fd2/LICENSE.txt) |
| block2 | 0.6.2 | MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/b4167b582b2f75f9a1be75495c41b765344fd03c/LICENSE.md) |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/bumpalo/3.20.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bumpalo/3.20.3/source/LICENSE-MIT) |
| bytemuck | 1.25.2 | Zlib OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/bytemuck/1.25.2/source/LICENSE-ZLIB) |
| bytemuck_derive | 1.12.1 | Zlib OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/bytemuck_derive/1.12.1/source/LICENSE-ZLIB) |
| bytes | 1.12.1 | MIT | [LICENSE](https://docs.rs/crate/bytes/1.12.1/source/LICENSE) |
| calloop | 0.13.0 | MIT | [LICENSE.txt](https://docs.rs/crate/calloop/0.13.0/source/LICENSE.txt) |
| calloop-wayland-source | 0.3.0 | MIT | [LICENSE.txt](https://docs.rs/crate/calloop-wayland-source/0.3.0/source/LICENSE.txt) |
| cc | 1.6.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/cc/1.6.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/cc/1.6.0/source/LICENSE-MIT) |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/cfg-if/1.0.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/cfg-if/1.0.5/source/LICENSE-MIT) |
| cfg_aliases | 0.2.2 | MIT | [LICENSE](https://docs.rs/crate/cfg_aliases/0.2.2/source/LICENSE), [NOTICES.md](https://docs.rs/crate/cfg_aliases/0.2.2/source/NOTICES.md) |
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
| dispatch | 0.2.0 | MIT | [manifest only](https://docs.rs/crate/dispatch/0.2.0/source/Cargo.toml) |
| dispatch2 | 0.3.1 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/8852b424193ca41602281b3d7540d7c8ed51e49a/LICENSE.md) |
| dlib | 0.5.3 | MIT | [LICENSE.txt](https://docs.rs/crate/dlib/0.5.3/source/LICENSE.txt) |
| document-features | 0.2.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/document-features/0.2.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/document-features/0.2.12/source/LICENSE-MIT) |
| downcast-rs | 1.2.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/downcast-rs/1.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/downcast-rs/1.2.1/source/LICENSE-MIT) |
| dpi | 0.1.2 | Apache-2.0 AND MIT | [LICENSE](https://docs.rs/crate/dpi/0.1.2/source/LICENSE), [LICENSE-LIBM-MIT](https://docs.rs/crate/dpi/0.1.2/source/LICENSE-LIBM-MIT) |
| equivalent | 1.0.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/equivalent/1.0.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/equivalent/1.0.2/source/LICENSE-MIT) |
| errno | 0.3.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/errno/0.3.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/errno/0.3.14/source/LICENSE-MIT) |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/find-msvc-tools/0.1.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/find-msvc-tools/0.1.14/source/LICENSE-MIT) |
| foldhash | 0.2.0 | Zlib | [LICENSE](https://docs.rs/crate/foldhash/0.2.0/source/LICENSE) |
| foreign-types | 0.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types/0.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types/0.5.0/source/LICENSE-MIT) |
| foreign-types-macros | 0.2.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types-macros/0.2.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types-macros/0.2.4/source/LICENSE-MIT) |
| foreign-types-shared | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/foreign-types-shared/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/foreign-types-shared/0.3.1/source/LICENSE-MIT) |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-core/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-core/0.3.34/source/LICENSE-MIT) |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-task/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-task/0.3.34/source/LICENSE-MIT) |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/futures-util/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/futures-util/0.3.34/source/LICENSE-MIT) |
| gethostname | 1.1.0 | Apache-2.0 | [LICENSE](https://docs.rs/crate/gethostname/1.1.0/source/LICENSE) |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/getrandom/0.3.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/getrandom/0.3.4/source/LICENSE-MIT) |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/getrandom/0.4.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/getrandom/0.4.3/source/LICENSE-MIT) |
| gpu-allocator | 0.28.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/gpu-allocator/0.28.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/gpu-allocator/0.28.0/source/LICENSE-MIT) |
| half | 2.7.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/half/2.7.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/half/2.7.1/source/LICENSE-MIT) |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hashbrown/0.16.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hashbrown/0.16.1/source/LICENSE-MIT) |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hashbrown/0.17.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hashbrown/0.17.1/source/LICENSE-MIT) |
| hermit-abi | 0.5.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/hermit-abi/0.5.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/hermit-abi/0.5.3/source/LICENSE-MIT) |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/indexmap/2.14.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/indexmap/2.14.2/source/LICENSE-MIT) |
| jni | 0.22.4 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/jni-rs/jni-rs/5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/jni-rs/jni-rs/5ae9458a4ec44c5318f37ddc7569c1d4ae8a69e7/LICENSE-MIT) |
| jni-macros | 0.22.4 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/jni-rs/jni-rs/33045a124105c939d1e2cbdcb5a39e5d868ffa03/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/jni-rs/jni-rs/33045a124105c939d1e2cbdcb5a39e5d868ffa03/LICENSE-MIT) |
| jni-sys | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jni-sys/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jni-sys/0.3.1/source/LICENSE-MIT) |
| jni-sys | 0.4.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jni-sys/0.4.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jni-sys/0.4.1/source/LICENSE-MIT) |
| jni-sys-macros | 0.4.1 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/jni-rs/jni-sys/64d77b7a5f119d7b55b4e2c169a4668067ff59e6/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/jni-rs/jni-sys/64d77b7a5f119d7b55b4e2c169a4668067ff59e6/LICENSE-MIT) |
| jobserver | 0.1.35 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/jobserver/0.1.35/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/jobserver/0.1.35/source/LICENSE-MIT) |
| js-sys | 0.3.106 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/js-sys/0.3.106/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/js-sys/0.3.106/source/LICENSE-MIT) |
| libc | 0.2.190 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/libc/0.2.190/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/libc/0.2.190/source/LICENSE-MIT) |
| libloading | 0.8.9 | ISC | [LICENSE](https://docs.rs/crate/libloading/0.8.9/source/LICENSE) |
| libm | 0.2.16 | MIT | [LICENSE.txt](https://docs.rs/crate/libm/0.2.16/source/LICENSE.txt) |
| libredox | 0.1.25 | MIT | [LICENSE](https://docs.rs/crate/libredox/0.1.25/source/LICENSE) |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/linux-raw-sys/0.12.1/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/linux-raw-sys/0.12.1/source/LICENSE-MIT) |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | [COPYRIGHT](https://docs.rs/crate/linux-raw-sys/0.4.15/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-APACHE), [LICENSE-Apache-2.0_WITH_LLVM-exception](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-Apache-2.0_WITH_LLVM-exception), [LICENSE-MIT](https://docs.rs/crate/linux-raw-sys/0.4.15/source/LICENSE-MIT) |
| litrs | 1.0.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/litrs/1.0.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/litrs/1.0.0/source/LICENSE-MIT) |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/lock_api/0.4.14/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/lock_api/0.4.14/source/LICENSE-MIT) |
| log | 0.4.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/log/0.4.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/log/0.4.34/source/LICENSE-MIT) |
| memchr | 2.8.3 | Unlicense OR MIT | [COPYING](https://docs.rs/crate/memchr/2.8.3/source/COPYING), [LICENSE-MIT](https://docs.rs/crate/memchr/2.8.3/source/LICENSE-MIT) |
| memmap2 | 0.9.11 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/memmap2/0.9.11/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/memmap2/0.9.11/source/LICENSE-MIT) |
| naga | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/naga/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/naga/30.0.1/source/LICENSE.MIT) |
| naga-types | 30.0.1 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/naga-types/30.0.1/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/naga-types/30.0.1/source/LICENSE.MIT) |
| ndk | 0.9.0 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-MIT) |
| ndk-context | 0.1.1 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/rust-windowing/android-ndk-rs/10f2ba388fca20f7349996ebae26ccda7a6fda5c/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/rust-windowing/android-ndk-rs/10f2ba388fca20f7349996ebae26ccda7a6fda5c/LICENSE-MIT) |
| ndk-sys | 0.6.0+11769913 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/rust-mobile/ndk/49bbbba16c58ff63cb8a0ad0eca5a9fb7ecaec25/LICENSE-MIT) |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num-traits/0.2.19/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/num-traits/0.2.19/source/LICENSE-MIT) |
| num_enum | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/num_enum/0.7.6/source/LICENSE-MIT) |
| num_enum_derive | 0.7.6 | BSD-3-Clause OR MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/num_enum_derive/0.7.6/source/LICENSE-MIT) |
| objc-sys | 0.3.5 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/4fc083f1c6d6784577e38b0ee8dbd344481e2fd2/LICENSE.txt) |
| objc2 | 0.5.3 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/5237cbf081d04ef75a1f6a207b4018e7c1ae7438/LICENSE.txt) |
| objc2 | 0.6.5 | MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/d7d2fa23ceaa5e6096c923b081040e5d81b3b9df/LICENSE.md) |
| objc2-app-kit | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-cloud-kit | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-contacts | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-data | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-core-graphics | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-core-image | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-core-location | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-encode | 4.1.0 | MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/8d214f5477365ffcbcbb7de058c86ed9a518efb7/LICENSE.md) |
| objc2-foundation | 0.2.2 | MIT | [src/copying.rs](https://docs.rs/crate/objc2-foundation/0.2.2/source/src/copying.rs), [src/tests/copying.rs](https://docs.rs/crate/objc2-foundation/0.2.2/source/src/tests/copying.rs) |
| objc2-foundation | 0.3.2 | MIT | [src/copying.rs](https://docs.rs/crate/objc2-foundation/0.3.2/source/src/copying.rs), [src/tests/copying.rs](https://docs.rs/crate/objc2-foundation/0.3.2/source/src/tests/copying.rs) |
| objc2-io-surface | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-link-presentation | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-metal | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-metal | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-quartz-core | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-quartz-core | 0.3.2 | Zlib OR Apache-2.0 OR MIT | [upstream LICENSE.md](https://raw.githubusercontent.com/madsmtm/objc2/7b1abfd750a2cacaea71d6a56ecfb83cb7de560b/LICENSE.md) |
| objc2-symbols | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-ui-kit | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-uniform-type-identifiers | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| objc2-user-notifications | 0.2.2 | MIT | [upstream LICENSE.txt](https://raw.githubusercontent.com/madsmtm/objc2/e282618be4c3a3b9542957e0c8540e9588472ce8/LICENSE.txt) |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/once_cell/1.21.4/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/once_cell/1.21.4/source/LICENSE-MIT) |
| orbclient | 0.3.55 | MIT | [LICENSE](https://docs.rs/crate/orbclient/0.3.55/source/LICENSE) |
| ordered-float | 5.5.0 | MIT | [LICENSE-MIT](https://docs.rs/crate/ordered-float/5.5.0/source/LICENSE-MIT) |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/parking_lot/0.12.5/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parking_lot/0.12.5/source/LICENSE-MIT) |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/parking_lot_core/0.9.12/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/parking_lot_core/0.9.12/source/LICENSE-MIT) |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/percent-encoding/2.3.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/percent-encoding/2.3.2/source/LICENSE-MIT) |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project/1.1.13/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project/1.1.13/source/LICENSE-MIT) |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project-internal/1.1.13/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project-internal/1.1.13/source/LICENSE-MIT) |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pin-project-lite/0.2.17/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pin-project-lite/0.2.17/source/LICENSE-MIT) |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/pkg-config/0.3.34/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pkg-config/0.3.34/source/LICENSE-MIT) |
| plain | 0.2.3 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/plain/0.2.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/plain/0.2.3/source/LICENSE-MIT) |
| polling | 3.11.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/polling/3.11.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/polling/3.11.0/source/LICENSE-MIT) |
| pollster | 0.4.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/pollster/0.4.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/pollster/0.4.0/source/LICENSE-MIT) |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/portable-atomic/1.15.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/portable-atomic/1.15.0/source/LICENSE-MIT) |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/portable-atomic-util/0.2.8/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/portable-atomic-util/0.2.8/source/LICENSE-MIT) |
| presser | 0.3.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/presser/0.3.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/presser/0.3.1/source/LICENSE-MIT) |
| proc-macro-crate | 3.5.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/proc-macro-crate/3.5.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/proc-macro-crate/3.5.0/source/LICENSE-MIT) |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/proc-macro2/1.0.107/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/proc-macro2/1.0.107/source/LICENSE-MIT) |
| profiling | 1.0.18 | MIT OR Apache-2.0 | [upstream LICENSE-APACHE](https://raw.githubusercontent.com/aclysma/profiling/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-APACHE), [upstream LICENSE-MIT](https://raw.githubusercontent.com/aclysma/profiling/8271551172eb6fa4cba47369aedd93790c623df9/LICENSE-MIT) |
| quick-xml | 0.41.0 | MIT | [LICENSE-MIT.md](https://docs.rs/crate/quick-xml/0.41.0/source/LICENSE-MIT.md) |
| quote | 1.0.47 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/quote/1.0.47/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/quote/1.0.47/source/LICENSE-MIT) |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [AUTHORS](https://docs.rs/crate/r-efi/5.3.0/source/AUTHORS) |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | [AUTHORS](https://docs.rs/crate/r-efi/6.0.0/source/AUTHORS) |
| range-alloc | 0.1.5 | MIT OR Apache-2.0 | [LICENSE.APACHE](https://docs.rs/crate/range-alloc/0.1.5/source/LICENSE.APACHE), [LICENSE.MIT](https://docs.rs/crate/range-alloc/0.1.5/source/LICENSE.MIT) |
| raw-window-handle | 0.6.2 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-APACHE.md), [LICENSE-MIT.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-MIT.md), [LICENSE-ZLIB.md](https://docs.rs/crate/raw-window-handle/0.6.2/source/LICENSE-ZLIB.md) |
| raw-window-metal | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/raw-window-metal/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/raw-window-metal/1.1.0/source/LICENSE-MIT) |
| redox_syscall | 0.4.1 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.4.1/source/LICENSE) |
| redox_syscall | 0.5.18 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.5.18/source/LICENSE) |
| redox_syscall | 0.9.4 | MIT | [LICENSE](https://docs.rs/crate/redox_syscall/0.9.4/source/LICENSE) |
| renderdoc-sys | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/renderdoc-sys/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/renderdoc-sys/1.1.0/source/LICENSE-MIT) |
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
| shlex | 2.0.1 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/shlex/2.0.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/shlex/2.0.1/source/LICENSE-MIT) |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/simd_cesu8/1.2.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/simd_cesu8/1.2.0/source/LICENSE-MIT) |
| simdutf8 | 0.1.5 | MIT OR Apache-2.0 | [LICENSE-Apache](https://docs.rs/crate/simdutf8/0.1.5/source/LICENSE-Apache), [LICENSE-MIT](https://docs.rs/crate/simdutf8/0.1.5/source/LICENSE-MIT) |
| slab | 0.4.12 | MIT | [LICENSE](https://docs.rs/crate/slab/0.4.12/source/LICENSE) |
| slotmap | 1.1.1 | Zlib | [LICENSE](https://docs.rs/crate/slotmap/1.1.1/source/LICENSE) |
| smallvec | 1.16.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/smallvec/1.16.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/smallvec/1.16.2/source/LICENSE-MIT) |
| smithay-client-toolkit | 0.19.2 | MIT | [LICENSE.txt](https://docs.rs/crate/smithay-client-toolkit/0.19.2/source/LICENSE.txt) |
| smol_str | 0.2.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/smol_str/0.2.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/smol_str/0.2.2/source/LICENSE-MIT) |
| spirv | 0.4.0+sdk-1.4.341.0 | Apache-2.0 | [upstream LICENSE](https://raw.githubusercontent.com/gfx-rs/rspirv/8afc3d0ac8e158128cd1410bb2e4b4c26ab11bb4/LICENSE) |
| static_assertions | 1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/static_assertions/1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/static_assertions/1.1.0/source/LICENSE-MIT) |
| syn | 2.0.119 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/syn/2.0.119/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/syn/2.0.119/source/LICENSE-MIT) |
| syn | 3.0.6 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/syn/3.0.6/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/syn/3.0.6/source/LICENSE-MIT) |
| taffy | 0.14.0 | MIT | [upstream LICENSE](https://raw.githubusercontent.com/DioxusLabs/taffy/77f385683c1d698c91a23a259f87fdddf26925fb/LICENSE) |
| thiserror | 1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror/1.0.69/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror/1.0.69/source/LICENSE-MIT) |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror/2.0.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror/2.0.21/source/LICENSE-MIT) |
| thiserror-impl | 1.0.69 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror-impl/1.0.69/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror-impl/1.0.69/source/LICENSE-MIT) |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/thiserror-impl/2.0.21/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/thiserror-impl/2.0.21/source/LICENSE-MIT) |
| tokio | 1.53.2 | MIT | [LICENSE](https://docs.rs/crate/tokio/1.53.2/source/LICENSE) |
| toml_datetime | 1.1.2+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_datetime/1.1.2%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_datetime/1.1.2%2Bspec-1.1.0/source/LICENSE-MIT) |
| toml_edit | 0.25.17+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_edit/0.25.17%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_edit/0.25.17%2Bspec-1.1.0/source/LICENSE-MIT) |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/toml_parser/1.1.5%2Bspec-1.1.0/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/toml_parser/1.1.5%2Bspec-1.1.0/source/LICENSE-MIT) |
| tracing | 0.1.44 | MIT | [LICENSE](https://docs.rs/crate/tracing/0.1.44/source/LICENSE) |
| tracing-core | 0.1.36 | MIT | [LICENSE](https://docs.rs/crate/tracing-core/0.1.36/source/LICENSE), [src/spin/LICENSE](https://docs.rs/crate/tracing-core/0.1.36/source/src/spin/LICENSE) |
| unicode-ident | 1.0.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | [LICENSE-APACHE](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-MIT), [LICENSE-UNICODE](https://docs.rs/crate/unicode-ident/1.0.26/source/LICENSE-UNICODE) |
| unicode-segmentation | 1.13.3 | MIT OR Apache-2.0 | [COPYRIGHT](https://docs.rs/crate/unicode-segmentation/1.13.3/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/unicode-segmentation/1.13.3/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-segmentation/1.13.3/source/LICENSE-MIT) |
| unicode-width | 0.2.2 | MIT OR Apache-2.0 | [COPYRIGHT](https://docs.rs/crate/unicode-width/0.2.2/source/COPYRIGHT), [LICENSE-APACHE](https://docs.rs/crate/unicode-width/0.2.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/unicode-width/0.2.2/source/LICENSE-MIT) |
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
| x11-dl | 2.21.0 | MIT | [LICENSE-MIT](https://docs.rs/crate/x11-dl/2.21.0/source/LICENSE-MIT) |
| x11rb | 0.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/x11rb/0.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/x11rb/0.13.2/source/LICENSE-MIT) |
| x11rb-protocol | 0.13.2 | MIT OR Apache-2.0 | [LICENSE-APACHE](https://docs.rs/crate/x11rb-protocol/0.13.2/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/x11rb-protocol/0.13.2/source/LICENSE-MIT) |
| xcursor | 0.3.11 | MIT | [LICENSE](https://docs.rs/crate/xcursor/0.3.11/source/LICENSE) |
| xkbcommon-dl | 0.4.2 | MIT | [LICENSE](https://docs.rs/crate/xkbcommon-dl/0.4.2/source/LICENSE) |
| xkeysym | 0.2.1 | MIT OR Apache-2.0 OR Zlib | [LICENSE-APACHE](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-APACHE), [LICENSE-MIT](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-MIT), [LICENSE-ZLIB](https://docs.rs/crate/xkeysym/0.2.1/source/LICENSE-ZLIB) |
| zerocopy | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/zerocopy/0.8.62/source/LICENSE-MIT) |
| zerocopy-derive | 0.8.62 | BSD-2-Clause OR Apache-2.0 OR MIT | [LICENSE-APACHE](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-APACHE), [LICENSE-BSD](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-BSD), [LICENSE-MIT](https://docs.rs/crate/zerocopy-derive/0.8.62/source/LICENSE-MIT) |
