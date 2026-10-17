// Package runtime is Ferry's Go-side host: a wazero-backed runtime that
// compiles a guest wasm module once and instantiates it fresh for every
// call (see Module.compile and Call), plus the generic Call[In, Out] that
// does the alloc/write/call/read/dealloc dance against ferry-go's wire
// convention.
package runtime

import (
	"context"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

// Runtime owns a wazero runtime with WASI instantiated. One Runtime can
// host any number of Modules.
type Runtime struct {
	wazero wazero.Runtime
}

// NewRuntime constructs a Runtime with WASI preview1 available to guests.
func NewRuntime(ctx context.Context) (*Runtime, error) {
	wr := wazero.NewRuntime(ctx)
	if _, err := wasi_snapshot_preview1.Instantiate(ctx, wr); err != nil {
		wr.Close(ctx)
		return nil, err
	}
	return &Runtime{wazero: wr}, nil
}

// Close releases the underlying wazero runtime and every module compiled
// from it.
func (r *Runtime) Close(ctx context.Context) error {
	return r.wazero.Close(ctx)
}
