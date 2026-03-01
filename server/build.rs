fn main() {
    // On Windows, we use ort-load-dynamic (fastembed feature) to load
    // ONNX Runtime as a shared library at runtime. This avoids the
    // CRT mismatch between ort-sys (/MT) and rocksdb-sys (/MD).
    //
    // On macOS/Linux, ort-download-binaries statically links ONNX Runtime.
}
