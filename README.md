# bytecodegen

DSL for generating bytecode definitions, encoding, decoding, interpreter loops and related boilerplate. See the [bytecodegen-core](./bytecodegen-core) crate for the core library, and the [bytecodegen-macro](./bytecodegen-macro) crate for the procedural macro that generates code from a DSL definition.

Check out the [interpreters.rs](./examples/interpreters.rs) example for a demonstration of how to use the generated code to implement an interpreter loop and tail dispatch interpreter.
