# Slint Windows 7 compatibility patch

`i-slint-backend-winit` is the source of the published 1.18.1 crate, retained with its upstream LICENSES directory. Cargo uses this copy through `[patch.crates-io]`.

The patch in `lib.rs` replaces the Windows 10-only `SystemParametersInfoForDpi` static import with the Win7 `SystemParametersInfoW` API, and converts the returned system-DPI font height to logical pixels. All other upstream platform implementations are retained for source completeness. The application selects the software renderer and uses its own optional DPI API layer.

Keep this patch and license files in source distributions. Do not edit the registry cache instead. Upstream: https://github.com/slint-ui/slint/tree/v1.18.1/internal/backends/winit

`windows-core` 0.62.2 retains its MIT/Apache notices. Its COM allocator and free-threaded marshaler imports use `ole32.dll`, which exports these APIs on Win7, instead of the Windows 8+ `combase.dll`. Import audits reject remaining `combase.dll` references.

The generated `windows` 0.62.2 COM bindings need the same three ole32 imports. `compat-build.ps1` verifies the upstream file hash and applies this patch to an ignored `.tools` copy via Cargo configuration. It never modifies the registry cache. This keeps the 109 MB generated source out of Git while preserving a reproducible patch.
