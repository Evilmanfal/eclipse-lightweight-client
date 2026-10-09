Bundled from crates.io libopus_sys 0.3.3, including its libopus sources and licenses.

Eclipse changes build.rs to set OPUS_STATIC_RUNTIME from Rust's crt-static target
feature on Windows. Upstream libopus defaults to a DLL CRT even when the codec
itself is linked statically. This keeps the codec and Rust allocation runtime
consistent in the portable executable. Codec sources are unchanged.

The patch also selects Release for CMake try-compiles and explicitly selects
MultiThreaded/MultiThreadedDLL from that same Rust CRT feature. This prevents
Debug probe/PDB-tool mismatches when building with the installed Windows toolset.
