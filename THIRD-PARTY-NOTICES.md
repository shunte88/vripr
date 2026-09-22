# Third-Party Notices

VRipr's own source code is licensed under the MIT License — see [LICENSE](LICENSE).

VRipr binaries are **statically linked** Rust executables: the compiled artefacts
published on the [Releases](../../releases) page embed the object code of VRipr's
dependencies. Some of those dependencies carry licences whose notice and source
availability terms apply to anyone redistributing those binaries. This file records
them.

A full machine-readable inventory of every dependency and its version is
[`Cargo.lock`](Cargo.lock) in this repository; each release tag pins the exact
versions used to build that release's binaries.

---

## LGPL-2.1-or-later — `chromaprint-next`

VRipr uses [`chromaprint-next`](https://github.com/attilagyorffy/chromaprint-next)
to compute AcoustID audio fingerprints in-process. That crate is licensed
`MIT AND LGPL-2.1-or-later`:

- Most of the crate is MIT (Copyright © 2010–2016 Lukas Lalinsky for the original
  C/C++ Chromaprint; Copyright © 2026 Attila Györffy for the Rust port).
- Its resampler module (`src/audio/resample.rs`) is a port of FFmpeg's
  `av_resample` (Copyright © 2004 Michael Niedermayer) and is licensed
  **LGPL-2.1-or-later**.

The full text of the GNU Lesser General Public License, version 2.1, is included
in this repository as [LICENSE-LGPL-2.1](LICENSE-LGPL-2.1) and is shipped
alongside every released binary.

### Notice to users of VRipr binaries

VRipr binaries contain LGPL-2.1-or-later licensed code. Under section 6 of that
licence you have the right to modify the LGPL portion and relink it into VRipr.
VRipr supports this as follows:

1. **Complete corresponding source.** The complete source of VRipr is this
   repository, under the MIT licence. The complete source of the LGPL component
   is published on [crates.io](https://crates.io/crates/chromaprint-next) and at
   the upstream repository linked above. The exact version used by any release is
   recorded in `Cargo.lock` at that release's tag.
2. **Relinking.** Because the complete source of the "work that uses the Library"
   is available under the MIT licence, you can modify `chromaprint-next` (for
   example via a Cargo `[patch]` entry or a path dependency) and rebuild VRipr
   yourself to produce a binary incorporating your modified version:

   ```toml
   # Cargo.toml
   [patch.crates-io]
   chromaprint-next = { path = "../my-modified-chromaprint-next" }
   ```

   ```sh
   cargo build --release
   ```

3. **No further restrictions.** VRipr does not impose terms on the LGPL portion
   beyond those in LGPL-2.1, and the binaries are not obfuscated or
   licence-restricted in a way that would prevent reverse engineering for
   debugging your modifications.

`chromaprint-next` is used **unmodified**, as an ordinary Cargo dependency.

---

## MPL-2.0 — Symphonia

VRipr decodes audio with [Symphonia](https://github.com/pdeljanov/Symphonia),
which is licensed under the Mozilla Public License 2.0. The following crates from
that project are linked into VRipr binaries:

`symphonia`, `symphonia-core`, `symphonia-bundle-flac`, `symphonia-bundle-mp3`,
`symphonia-codec-adpcm`, `symphonia-codec-pcm`, `symphonia-codec-vorbis`,
`symphonia-format-mkv`, `symphonia-format-ogg`, `symphonia-format-riff`,
`symphonia-metadata`, `symphonia-utils-xiph`.

MPL-2.0 is a per-file copyleft licence. VRipr uses these crates **unmodified**;
their source is available at the repository above and on crates.io. A copy of the
MPL-2.0 licence text is available at <https://mozilla.org/MPL/2.0/>. Should any
Symphonia file ever be modified within VRipr, that file's source must be made
available under MPL-2.0 — this project's own source is public, which satisfies
that requirement.

`option-ext` (a transitive dependency of `dirs`) is also MPL-2.0 and is likewise
used unmodified.

---

## Permissive licences

The remaining dependencies are under permissive licences that require attribution
only — predominantly `MIT OR Apache-2.0`, with a smaller number under
Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0,
CDLA-Permissive-2.0, CC0-1.0, and Unlicense. Their copyright notices are carried
in their respective crate sources, referenced by `Cargo.lock`.

To regenerate a complete per-crate listing:

```sh
cargo install cargo-about
cargo about generate --format json
```

---

## Optional ONNX feature

Builds with `--features onnx` link [`ort`](https://github.com/pykeio/ort)
(MIT OR Apache-2.0) and download prebuilt ONNX Runtime binaries, which are
licensed under the Apache License 2.0 by Microsoft. Those binaries are a runtime
artefact fetched at build time and are not covered by the notices above.
