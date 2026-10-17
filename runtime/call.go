package runtime

import (
	"bytes"
	"context"
	"encoding/binary"
	"fmt"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/vmihailenco/msgpack/v5"
)

const maxStderrSnippet = 4 << 10 // 4KB

// Call invokes a guest export by name, msgpack-encoding in and decoding the
// envelope's success payload into Out.
//
// Every call gets a fresh module instance, compiled once and instantiated
// here, closed when Call returns. Two things depend on this: the guest's
// bump allocator (see ferry-go/src/alloc.rs) never reclaims memory back to
// the instance, so a reused instance would leak without bound across calls;
// and a guest panic currently traps the whole instance outright (wasm32-
// wasip1's prebuilt std forces panic=abort, so ferry-go's catch_unwind
// can't actually catch anything today -- see docs/abi.md), so reusing an
// instance after a trap would mean calling into a module that's already
// dead.
func Call[In, Out any](ctx context.Context, m *Module, export string, in In) (Out, error) {
	var zero Out

	compiled, err := m.compile(ctx)
	if err != nil {
		return zero, fmt.Errorf("compile module %q: %w", m.name, err)
	}

	inputBytes, err := msgpack.Marshal(in)
	if err != nil {
		return zero, fmt.Errorf("marshal input: %w", err)
	}

	var stderr bytes.Buffer
	mod, err := m.rt.wazero.InstantiateModule(ctx, compiled, wazero.NewModuleConfig().WithStderr(&stderr))
	if err != nil {
		return zero, fmt.Errorf("instantiate module %q: %w", m.name, err)
	}
	defer mod.Close(context.Background())

	resultBytes, err := callExport(ctx, mod, export, inputBytes)
	if err != nil {
		// The export call itself failed -- on today's toolchain, this is
		// how a guest panic actually surfaces (a trap, not a decoded
		// envelope). Stderr is the only place the panic message survives;
		// see ferry-go's panic hook.
		return zero, &Error{Kind: KindPanic, Message: err.Error(), Stderr: stderrSnippet(&stderr)}
	}

	envelope, err := decodeEnvelope(resultBytes)
	if err != nil {
		return zero, &Error{Kind: KindDecode, Message: err.Error(), Stderr: stderrSnippet(&stderr)}
	}

	if !envelope.Ok {
		var wireErr wireError
		if err := msgpack.Unmarshal(envelope.Payload, &wireErr); err != nil {
			return zero, &Error{
				Kind:    KindDecode,
				Message: fmt.Sprintf("malformed error payload: %v", err),
				Stderr:  stderrSnippet(&stderr),
			}
		}
		return zero, &Error{Kind: wireErr.Kind, Message: wireErr.Message, Stderr: stderrSnippet(&stderr)}
	}

	var out Out
	if err := msgpack.Unmarshal(envelope.Payload, &out); err != nil {
		return zero, &Error{
			Kind:    KindDecode,
			Message: fmt.Sprintf("decode result: %v", err),
			Stderr:  stderrSnippet(&stderr),
		}
	}
	return out, nil
}

// decodeEnvelope unwraps the {v, k, p} envelope and checks its version.
func decodeEnvelope(resultBytes []byte) (wireEnvelope, error) {
	if len(resultBytes) == 0 {
		return wireEnvelope{}, fmt.Errorf("guest returned an empty result (allocation failure?)")
	}
	var envelope wireEnvelope
	if err := msgpack.Unmarshal(resultBytes, &envelope); err != nil {
		return wireEnvelope{}, fmt.Errorf("malformed envelope: %w", err)
	}
	if envelope.Version != EnvelopeVersion {
		return wireEnvelope{}, fmt.Errorf(
			"unsupported envelope version %d (expected %d) -- the guest module may be out of sync with this build",
			envelope.Version, EnvelopeVersion,
		)
	}
	return envelope, nil
}

func stderrSnippet(stderr *bytes.Buffer) string {
	if stderr == nil || stderr.Len() == 0 {
		return ""
	}
	s := stderr.String()
	if len(s) > maxStderrSnippet {
		s = s[:maxStderrSnippet] + "... (truncated)"
	}
	return s
}

// callExport does the low-level memory dance against a single module
// instance: allocate and write the input, allocate space for the output
// header, call the export, read the header, copy the result out of guest
// memory, and free every buffer the guest allocated.
func callExport(ctx context.Context, mod api.Module, export string, input []byte) ([]byte, error) {
	alloc := mod.ExportedFunction("alloc")
	dealloc := mod.ExportedFunction("dealloc")
	fn := mod.ExportedFunction(export)
	if alloc == nil {
		return nil, fmt.Errorf("wasm export %q not found", "alloc")
	}
	if dealloc == nil {
		return nil, fmt.Errorf("wasm export %q not found", "dealloc")
	}
	if fn == nil {
		return nil, fmt.Errorf("wasm export %q not found", export)
	}

	inputSize := uint64(len(input))
	res, err := alloc.Call(ctx, inputSize)
	if err != nil {
		return nil, fmt.Errorf("alloc input: %w", err)
	}
	inPtr := uint32(res[0])
	defer dealloc.Call(ctx, uint64(inPtr), inputSize)

	if len(input) > 0 && !mod.Memory().Write(inPtr, input) {
		return nil, fmt.Errorf("failed to write input to guest memory")
	}

	// 8 bytes for the WasmBuf {ptr: u32, len: u32} header the guest writes
	// its result into.
	headerAlloc, err := alloc.Call(ctx, 8)
	if err != nil {
		return nil, fmt.Errorf("alloc output header: %w", err)
	}
	outStructPtr := uint32(headerAlloc[0])
	defer dealloc.Call(ctx, uint64(outStructPtr), 8)

	if _, err := fn.Call(ctx, uint64(outStructPtr), uint64(inPtr), inputSize); err != nil {
		return nil, fmt.Errorf("%s: %w", export, err)
	}

	header, ok := mod.Memory().Read(outStructPtr, 8)
	if !ok {
		return nil, fmt.Errorf("failed to read output header")
	}
	retPtr := binary.LittleEndian.Uint32(header[0:4])
	retLen := binary.LittleEndian.Uint32(header[4:8])
	if retLen == 0 {
		return nil, nil
	}
	defer dealloc.Call(ctx, uint64(retPtr), uint64(retLen))

	resultBytes, ok := mod.Memory().Read(retPtr, retLen)
	if !ok {
		return nil, fmt.Errorf("failed to read result data")
	}

	// Copy out of guest memory so it survives the instance being closed.
	out := make([]byte, len(resultBytes))
	copy(out, resultBytes)
	return out, nil
}
