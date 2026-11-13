# Third-Party Notices — MultiTunnel

MultiTunnel is MIT-licensed (see LICENSE). This file lists the third-party
components the project references or expects the user to run alongside it,
per the LICENSE's notice obligation. Versions are those pinned in
Cargo.lock at the time of writing.

## Rust dependencies (compiled into the binaries)

All of the following are permissive dual-licensed or MIT crates; the
authors grant redistribution under the terms below. "MIT OR Apache-2.0"
means you may choose either license.

| Crate | Version | License | Upstream |
|---|---|---|---|
| serde / serde_derive | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_core | (serde runtime dep) | MIT OR Apache-2.0 | https://crates.io/crates/serde_core |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| regex | 1.13.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-automata, regex-syntax | (regex subcrates) | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| aho-corasick | (regex dep) | MIT OR Unlicense | https://github.com/BurntSushi/aho-corasick |
| memchr | (regex dep) | MIT OR Unlicense | https://github.com/BurntSushi/rust-memchr |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-link | (windows-sys dep) | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| winreg | 0.56.0 | MIT | https://github.com/gentoo90/winreg |
| cfg-if | (winreg dep) | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| proc-macro2, quote, syn, unicode-ident | (serde_derive deps) | MIT OR Apache-2.0 | https://crates.io/crates/proc-macro2 |
| itoa, zmij | (serde_json deps) | MIT OR Apache-2.0 | https://crates.io/crates/itoa |

License texts:
- MIT: https://opensource.org/license/mit (text also embedded below)
- Apache-2.0: https://www.apache.org/licenses/LICENSE-2.0
- Unlicense: https://unlicense.org

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

## Components the user runs separately (NOT redistributed in releases)

| Component | License | Upstream |
|---|---|---|
| Xray-core (the proxy core; downloaded by the user) | MPL-2.0 | https://github.com/XTLS/Xray-core |
| net-merge (link bonding; installed by the user) | MIT | https://github.com/net-merge/net-merge |

MultiTunnel neither bundles nor redistributes these binaries; it only
launches processes the user installed. MPL-2.0 and MIT impose no
obligations on this project's own source distribution.

---

This file is part of every release zip (see .github/workflows/release.yml).
Update it whenever Cargo.lock gains a new dependency.
