package runtime

import (
	"context"
	"sync"

	"github.com/tetratelabs/wazero"
)

// Loader fetches a module's raw wasm bytes, e.g. embedding plus LoadZstd.
// It's called at most once per Module, the first time the module is used.
type Loader func() ([]byte, error)

// Module is a compiled guest wasm binary, loaded and compiled lazily and
// memoized for the life of the Runtime. Compiling is the expensive, cacheable
// step; Call instantiates a fresh module instance for every call on top of
// this compiled module and discards it afterwards -- see Call for why.
type Module struct {
	rt       *Runtime
	name     string
	load     Loader
	once     sync.Once
	compiled wazero.CompiledModule
	err      error
}

// NewModule registers a module with the runtime. Nothing is loaded or
// compiled until the first Call against it.
func (r *Runtime) NewModule(name string, load Loader) *Module {
	return &Module{rt: r, name: name, load: load}
}

func (m *Module) compile(ctx context.Context) (wazero.CompiledModule, error) {
	m.once.Do(func() {
		raw, err := m.load()
		if err != nil {
			m.err = err
			return
		}
		m.compiled, m.err = m.rt.wazero.CompileModule(ctx, raw)
	})
	return m.compiled, m.err
}
