# bytecodegen

`bytecodegen` is a Rust DSL for declaring a bytecode format once and generating the repetitive parts around it: instruction types, encoding and decoding, disassembly, linking, metadata layout, and interpreter-facing helpers.

The design is based on WebKit's JavaScriptCore bytecode rewrite described in [A New Bytecode Format for JavaScriptCore](https://webkit.org/blog/9329/a-new-bytecode-format-for-javascriptcore/), adapted here as a reusable Rust library rather than an JS engine-specific implementation.

See [bytecodegen-core](./bytecodegen-core) for the runtime pieces and [bytecodegen-macros](./bytecodegen-macros) for the DSL and code generation. For a complete example, start with [examples/interpreters.rs](./examples/interpreters.rs).

## Why use this?

Defining a non-trivial bytecode format usually means writing the same logic several times: instruction structs, encoders, decoders, relocations, disassembly, metadata plumbing, and interpreter dispatch glue. That code is repetitive, easy to get wrong, and awkward to evolve.

`bytecodegen` moves that work into the DSL. You describe opcodes, operands, relocations, and per-opcode metadata once, and the crate generates the typed surface area around it. The result is less boilerplate and a format that is easier to inspect, test, and change.

## Encoding model

The encoding follows the same broad idea as the JavaScriptCore format from the WebKit post: keep the instruction stream compact, and move writable runtime state into side tables instead of embedding it in the instruction bytes.

Each instruction is emitted at the smallest width that fits all of its operands and its opcode:

- `Narrow`: 1-byte header and 1-byte operands
- `Wide16`: 3-byte header and 2-byte operands
- `Wide32`: 5-byte header and 4-byte operands

Widening is instruction-wide. If one field does not fit, the whole instruction is promoted. That keeps operand offsets regular and decoding simple.

## Linking and metadata

Linking resolves relocatable operands and builds a packed metadata table for instructions that declare runtime payloads. The instruction stream stores only a `metadata_id`; the actual typed metadata lives in a separate allocation indexed by opcode and slot.

This mirrors the main tradeoff described in the WebKit article:

- encoded bytecode stays compact and easier to store or map
- writable metadata stays outside the instruction stream
- interpreters read metadata through typed side-table access instead of patching instruction bytes in place

In this crate, that model is exposed directly through generated metadata structs, typed table views, and linking helpers.

## What the macro generates

For each declared section, `bytecode_list!` generates:

- typed instruction structs and a shared instruction enum
- encode and decode helpers
- disassembly helpers
- linkable forms for relocations and metadata
- metadata structs for opcodes that declare payloads
- verifier and debug-oriented utilities

The examples directory shows the intended workflow:

- [examples/interpreters.rs](./examples/interpreters.rs): encoding, linking, disassembly, and interpreter execution
- [examples/debug_dsl.rs](./examples/debug_dsl.rs): debug metadata and source mapping
- [examples/verify_dsl.rs](./examples/verify_dsl.rs): validation of bytecode definitions. NOTE: this is a work in progress, only jump target and stack depth verification are implemented so far.
