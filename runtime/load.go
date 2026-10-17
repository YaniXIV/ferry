package runtime

import (
	"fmt"

	"github.com/klauspost/compress/zstd"
)

// maxDecompressedSize caps how large a zstd-decompressed module can be, as
// a sanity check against a corrupt or hostile artifact -- not a realistic
// limit for any guest module Ferry expects to run.
const maxDecompressedSize = 200 << 20 // 200MB

// LoadZstd decompresses a zstd-compressed wasm binary, typically one
// embedded into the caller's package via //go:embed. Decompression (and the
// CompileModule call after it) only happens once per Module, memoized by
// NewModule -- see Module.compile.
func LoadZstd(compressed []byte) ([]byte, error) {
	decoder, err := zstd.NewReader(nil, zstd.WithDecoderConcurrency(0))
	if err != nil {
		return nil, fmt.Errorf("new zstd reader: %w", err)
	}
	defer decoder.Close()

	out, err := decoder.DecodeAll(compressed, nil)
	if err != nil {
		return nil, fmt.Errorf("zstd decode: %w", err)
	}
	if len(out) > maxDecompressedSize {
		return nil, fmt.Errorf("decompressed module too large: %d bytes", len(out))
	}
	return out, nil
}
