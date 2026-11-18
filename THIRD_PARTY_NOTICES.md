# Third-Party Notices — MultiTunnel

MultiTunnel is MIT-licensed (see LICENSE). This file lists every
third-party component the project's release binaries reference or
expect at runtime, per the LICENSE's notice obligation. The Rust
dependency manifest is MACHINE-GENERATED from Cargo.lock (see
repo `scripts/gen_notices.py`);
versions are exactly those pinned by the v0.3.0 Cargo.lock
(391 external crates).

## Rust dependencies (compiled into the release binaries)

Every dependency below is permissively licensed; grouped by license
expression. "OR" licenses let the distributor choose either side
(this project elects the permissive side listed in the elections note
below where a copyleft option exists). No GPL/AGPL/MPL component is
linked into any binary.

### MIT OR Apache-2.0 (165 crates)

- ahash 0.8.12, android-activity 0.6.1, anyhow 1.0.104, arbitrary 1.5.0
- arrayvec 0.7.8, as-raw-xcb-connection 1.0.1, async-trait 0.1.92, base64 0.22.1
- base64 0.23.1, bitflags 2.13.2, block-buffer 0.10.4, bumpalo 3.20.4
- by_address 1.2.1, cc 1.11.2, cfg-if 1.0.5, chacha20 0.10.2
- core-foundation 0.9.4, core-foundation-sys 0.8.7, core-graphics 0.23.2
- core-graphics-types 0.1.3, cosmic-text 0.12.1, cpufeatures 0.2.17
- cpufeatures 0.3.1, crc32fast 1.5.5, crossbeam-deque 0.8.8
- crossbeam-epoch 0.9.21, crossbeam-utils 0.8.23, crypto-common 0.1.7
- derive_arbitrary 1.5.0, digest 0.10.7, displaydoc 0.2.7, either 1.19.0
- errno 0.3.15, fdeflate 0.3.7, find-msvc-tools 0.1.15, flate2 1.1.11
- font-types 0.7.3, form_urlencoded 1.2.2, futures 0.3.34
- futures-channel 0.3.34, futures-core 0.3.34, futures-executor 0.3.34
- futures-io 0.3.34, futures-macro 0.3.34, futures-sink 0.3.34
- futures-task 0.3.34, futures-util 0.3.34, getrandom 0.2.17, getrandom 0.3.4
- getrandom 0.4.3, glam 0.25.0, half 2.7.1, hashbrown 0.17.1, hermit-abi 0.5.3
- hex 0.4.3, http 1.5.0, httparse 1.10.1, idna 1.1.0, ipnet 2.12.2, itoa 1.0.19
- jni 0.22.4, jni-macros 0.22.4, jni-sys 0.3.1, jni-sys 0.4.1
- jni-sys-macros 0.4.1, jobserver 0.1.35, js-sys 0.3.107, kurbo 0.10.4
- libc 0.2.191, lock_api 0.4.14, log 0.4.35, material-colors 0.5.0
- memmap2 0.9.12, ndk 0.9.0, ndk-context 0.1.1, ndk-sys 0.6.0+11769913
- num-traits 0.2.19, once_cell 1.21.4, palette 0.7.7, palette_derive 0.7.7
- palette_math 0.7.7, percent-encoding 2.3.2, pin-utils 0.1.1, pkg-config 0.3.34
- png 0.17.16, proc-macro-crate 3.5.0, proc-macro2 1.0.107, quinn 0.11.12
- quinn-proto 0.11.19, quinn-udp 0.5.16, quote 1.0.47, rand 0.10.4
- rand_core 0.10.1, rand_pcg 0.10.2, rayon 1.12.0, rayon-core 1.13.0
- read-fonts 0.22.7, regex 1.13.1, regex-automata 0.4.18, regex-syntax 0.8.11
- reqwest 0.12.28, roxmltree 0.20.0, rustc_version 0.4.1
- rustls-pki-types 1.16.0, rustversion 1.0.23, scopeguard 1.2.0, semver 1.0.28
- serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.154
- sha2 0.10.9, shlex 2.0.1, signal-hook-registry 1.4.8, simdutf8 0.1.5
- siphasher 1.0.4, skrifa 0.22.3, smallvec 1.16.5, smol_str 0.2.2, socket2 0.6.5
- softbuffer 0.4.8, stable_deref_trait 1.2.1, syn 2.0.119, syn 3.0.10
- sys-locale 0.3.2, thiserror 1.0.69, thiserror 2.0.21, thiserror-impl 1.0.69
- thiserror-impl 2.0.21, tokio-rustls 0.26.6, toml_datetime 1.1.3+spec-1.1.0
- toml_edit 0.25.19+spec-1.1.0, toml_parser 1.1.5+spec-1.1.0, ttf-parser 0.20.0
- ttf-parser 0.21.1, ttf-parser 0.25.1, typenum 1.20.1, unicode-bidi 0.3.18
- unicode-script 0.5.9, unicode-segmentation 1.13.3, url 2.5.8
- wasm-bindgen 0.2.130, wasm-bindgen-futures 0.4.80, wasm-bindgen-macro 0.2.130
- wasm-bindgen-macro-support 0.2.130, wasm-bindgen-shared 0.2.130
- web-sys 0.3.107, web-time 1.1.0, windows-link 0.2.1, windows-sys 0.52.0
- windows-sys 0.59.0, windows-sys 0.61.2, windows-targets 0.52.6
- windows_aarch64_gnullvm 0.52.6, windows_aarch64_msvc 0.52.6
- windows_i686_gnu 0.52.6, windows_i686_gnullvm 0.52.6, windows_i686_msvc 0.52.6
- windows_x86_64_gnu 0.52.6, windows_x86_64_gnullvm 0.52.6
- windows_x86_64_msvc 0.52.6, x11rb 0.13.2, x11rb-protocol 0.13.2, yazi 0.1.6
- zeno 0.2.3

### MIT (103 crates)

- android-properties 0.2.2, block2 0.5.1, bytes 1.12.1, calloop 0.13.0
- calloop 0.14.5, calloop-wayland-source 0.3.0, calloop-wayland-source 0.4.1
- cfg_aliases 0.2.2, clipboard_x11 0.4.3, combine 4.6.9, crunchy 0.2.4
- dispatch 0.2.0, dlib 0.5.3, drm 0.14.1, drm-ffi 0.9.1, drm-fourcc 2.2.0
- drm-sys 0.8.1, fontconfig-parser 0.5.8, fontdb 0.16.2, generic-array 0.14.7
- http-body 1.1.0, http-body-util 0.1.5, hyper 1.12.0, hyper-util 0.1.21
- iced 0.13.1, iced_core 0.13.2, iced_futures 0.13.2, iced_graphics 0.13.0
- iced_renderer 0.13.0, iced_runtime 0.13.2, iced_tiny_skia 0.13.0
- iced_widget 0.13.4, iced_winit 0.13.0, libm 0.2.16, libredox 0.1.25, mio 1.2.5
- objc-sys 0.3.5, objc2 0.5.3, objc2 0.6.5, objc2-app-kit 0.2.2
- objc2-cloud-kit 0.2.2, objc2-contacts 0.2.2, objc2-core-data 0.2.2
- objc2-core-image 0.2.2, objc2-core-location 0.2.2, objc2-encode 4.1.0
- objc2-foundation 0.2.2, objc2-foundation 0.3.2, objc2-link-presentation 0.2.2
- objc2-metal 0.2.2, objc2-quartz-core 0.2.2, objc2-symbols 0.2.2
- objc2-ui-kit 0.2.2, objc2-uniform-type-identifiers 0.2.2
- objc2-user-notifications 0.2.2, orbclient 0.3.55, phf 0.13.1
- phf_shared 0.13.1, quick-xml 0.41.0, redox_syscall 0.2.16, redox_syscall 0.4.1
- redox_syscall 0.5.18, redox_syscall 0.9.5, rustybuzz 0.14.1
- sctk-adwaita 0.10.1, simd-adler32 0.3.10, slab 0.4.12
- smithay-client-toolkit 0.19.2, smithay-client-toolkit 0.20.0
- smithay-clipboard 0.7.3, strict-num 0.1.1, synstructure 0.14.0, tokio 1.54.1
- tokio-macros 2.7.2, tower 0.5.3, tower-http 0.6.11, tower-layer 0.3.3
- tower-service 0.3.3, tracing 0.1.44, tracing-attributes 0.1.31
- tracing-core 0.1.36, try-lock 0.2.5, want 0.3.2, wasm-timer 0.2.5
- wayland-backend 0.3.17, wayland-client 0.31.15, wayland-csd-frame 0.3.0
- wayland-cursor 0.31.14, wayland-protocols 0.32.13
- wayland-protocols-experimental 20250721.0.1, wayland-protocols-misc 0.3.12
- wayland-protocols-plasma 0.3.12, wayland-protocols-wlr 0.3.12
- wayland-scanner 0.31.11, wayland-sys 0.31.11, window_clipboard 0.4.1
- winnow 1.0.5, winreg 0.56.0, x11-dl 2.21.0, xcursor 0.3.11, xkbcommon-dl 0.4.2
- zip 2.6.1, zmij 1.0.23

### Apache-2.0 OR MIT (20 crates)

(same terms as MIT OR Apache-2.0 (order swapped by the crate).)

- atomic-waker 1.1.2, autocfg 1.5.1, concurrent-queue 2.5.0, ctor 0.10.1
- dtor 0.8.1, equivalent 1.0.3, fastrand 2.5.0, idna_adapter 1.2.2
- indexmap 2.14.2, pin-project 1.1.13, pin-project-internal 1.1.13
- pin-project-lite 0.2.17, polling 3.11.0, rustc-hash 2.1.3, simd_cesu8 1.2.0
- swash 0.1.19, utf8_iter 1.1.0, utf_types 1.0.0, uuid 1.29.3, zeroize 1.9.1

### MIT/Apache-2.0 (15 crates)

(same terms as MIT OR Apache-2.0 (slash form).)

- bitflags 1.3.2, downcast-rs 1.2.1, foreign-types 0.5.0
- foreign-types-macros 0.2.4, foreign-types-shared 0.3.1, plain 0.2.3
- rangemap 1.8.0, scoped-tls 1.0.1, serde_urlencoded 0.7.1
- unicode-bidi-mirroring 0.2.0, unicode-ccc 0.2.0, version_check 0.9.5
- winapi 0.3.9, winapi-i686-pc-windows-gnu 0.4.0
- winapi-x86_64-pc-windows-gnu 0.4.0

### Apache-2.0/MIT (3 crates)

(same terms as MIT OR Apache-2.0 (slash form).)

- parking_lot 0.11.2, parking_lot_core 0.8.6, rustc-hash 1.1.0

### Apache-2.0 (11 crates)

- ab_glyph 0.2.32, ab_glyph_rasterizer 0.1.10, approx 0.5.1
- clipboard_macos 0.1.1, clipboard_wayland 0.2.2, gethostname 1.1.0
- owned_ttf_parser 0.25.1, sync_wrapper 1.0.2, unicode-linebreak 0.1.5
- winit 0.30.13, zopfli 0.8.4

### Unicode-3.0 (18 crates)

- icu_collections 2.4.0, icu_locale_core 2.4.0, icu_normalizer 2.4.0
- icu_normalizer_data 2.4.0, icu_properties 2.4.0, icu_properties_data 2.4.0
- icu_provider 2.4.0, litemap 0.8.3, potential_utf 0.1.6, tinystr 0.8.5
- writeable 0.6.4, yoke 0.8.4, yoke-derive 0.8.4, zerofrom 0.1.8
- zerofrom-derive 0.1.8, zerotrie 0.2.5, zerovec 0.11.9, zerovec-derive 0.11.7

### (MIT OR Apache-2.0) AND Unicode-3.0 (2 crates)

- unicode-ident 1.0.27, unicode-properties 0.1.5

### 0BSD OR MIT OR Apache-2.0 (1 crates)

- adler2 2.0.1

### Apache-2.0 AND ISC (1 crates)

- ring 0.17.14

### Apache-2.0 AND MIT (1 crates)

- dpi 0.1.2

### Apache-2.0 OR BSL-1.0 (1 crates)

- ryu 1.0.23

### Apache-2.0 OR GPL-2.0-only (1 crates)

- self_cell 1.3.0

### Apache-2.0 OR ISC OR MIT (2 crates)

- hyper-rustls 0.27.10, rustls 0.23.46

### Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT (8 crates)

- linux-raw-sys 0.12.1, linux-raw-sys 0.4.15, linux-raw-sys 0.9.4
- rustix 0.38.44, rustix 1.1.6, wasi 0.11.1+wasi-snapshot-preview1
- wasip2 1.0.4+wasi-0.2.12, wit-bindgen 0.57.1

### BSD-2-Clause (1 crates)

- arrayref 0.3.9

### BSD-2-Clause OR Apache-2.0 OR MIT (2 crates)

- zerocopy 0.8.63, zerocopy-derive 0.8.63

### BSD-3-Clause (4 crates)

- instant 0.1.13, subtle 2.6.1, tiny-skia 0.11.4, tiny-skia-path 0.11.4

### BSD-3-Clause OR MIT OR Apache-2.0 (2 crates)

- num_enum 0.7.6, num_enum_derive 0.7.6

### BSL-1.0 (2 crates)

- clipboard-win 5.4.3, error-code 3.4.0

### CDLA-Permissive-2.0 (1 crates)

- webpki-roots 1.0.12

### ISC (3 crates)

- libloading 0.8.9, rustls-webpki 0.103.17, untrusted 0.9.0

### MIT OR Apache-2.0 OR LGPL-2.1-or-later (2 crates)

- r-efi 5.3.0, r-efi 6.0.0

### MIT OR Apache-2.0 OR Zlib (5 crates)

- cursor-icon 1.2.0, lru-slab 0.1.3, raw-window-handle 0.6.2, tiny-xlib 0.2.5
- xkeysym 0.2.1

### MIT OR Zlib OR Apache-2.0 (2 crates)

- miniz_oxide 0.8.9, miniz_oxide 0.9.1

### Unlicense OR MIT (3 crates)

- aho-corasick 1.1.5, memchr 2.8.3, winapi-util 0.1.11

### Unlicense/MIT (2 crates)

- same-file 1.0.6, walkdir 2.5.0

### Zlib (2 crates)

- slotmap 1.1.1, zlib-rs 0.6.8

### Zlib OR Apache-2.0 OR MIT (8 crates)

- bytemuck 1.25.2, bytemuck_derive 1.12.1, dispatch2 0.3.1
- objc2-core-foundation 0.3.2, objc2-core-graphics 0.3.2, objc2-io-surface 0.3.2
- objc2-quartz-core 0.3.2, tinyvec 1.13.3

### License elections (OR-licensed crates with a copyleft option)

- `self_cell 1.3.0` — "Apache-2.0 OR GPL-2.0-only": this project
  elects **Apache-2.0**.
- `r-efi 5.3.0` / `r-efi 6.0.0` — "MIT OR Apache-2.0 OR
  LGPL-2.1-or-later" (UEFI-target-only dependency): this project
  elects **MIT**.

### Embedded fonts (inside every GUI binary)

`iced_graphics 0.13` (MIT OR Apache-2.0) compiles two font files into
the GUI binaries. Their licenses require this notice on redistribution:

- `fonts/FiraSans-Regular.ttf` — SIL Open Font License 1.1
  (https://openfontlicense.org/open-font-license-official-text/).
  Copyright 2012-2015 The Fira Project authors
  (https://github.com/mozilla/Fira), with Reserved Font Name "Fira".
- `fonts/Iced-Icons.ttf` — MIT OR Apache-2.0,
  https://github.com/iced-rs/iced (Copyright 2021 iced contributors).

### License texts

- MIT: https://opensource.org/license/mit (text embedded below)
- Apache-2.0: https://www.apache.org/licenses/LICENSE-2.0
- Zlib: https://opensource.org/license/zlib
- BSD-2/3-Clause: https://opensource.org/license/bsd-3-clause
- ISC: https://opensource.org/license/isc
- BSL-1.0: https://www.boost.org/users/license.html
- Unicode-3.0: https://www.unicode.org/license.txt
- 0BSD: https://unlicense.org/#unlicense-only
- Unlicense: https://unlicense.org
- CDLA-Permissive-2.0: https://cdla.dev/permissive-2-0
- OFL-1.1 (fonts only): https://openfontlicense.org
- LLVM-exception: https://spdx.org/licenses/LLVM-exception.html

```text
MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Components fetched at runtime (NOT redistributed in the zips)

| Component | License | Upstream |
|---|---|---|
| Xray-core (the proxy engine) | MPL-2.0 | https://github.com/XTLS/Xray-core |
| net-merge (link bonding; legacy v0.2 `connect` path only) | MIT | https://github.com/net-merge/net-merge |

Since v0.3 MultiTunnel can download the Xray-core engine itself on first
connect (user-triggered, from the official XTLS GitHub releases, verified
against the published SHA256SUMS when present — see DECISIONS D14).
Downloading at user request is not redistribution: no Xray bits ship in
the release zips, and MPL-2.0 imposes no obligations on this project's
own source distribution. Users who prefer manual placement can still
drop `xray.exe` into the engine directory themselves. The v0.3 GUI bonds
links in-process (mt-net) and does not require net-merge at all; the
legacy `multitunnel connect` command still launches a user-installed
net-merge if the v0.2 config asks for it.

---

This file is part of every release zip (see .github/workflows/release.yml).
Update it whenever Cargo.lock gains a new dependency
(regenerate with the notices script; see harness docs).
