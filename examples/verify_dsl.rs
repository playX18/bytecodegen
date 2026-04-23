use bytecodegen::bytecode_list;

bytecode_list! {
    section Vm {
        #[verify(stack_balance = 1)]
        Push { args { value: i32 } }

        #[verify(stack_balance = -1)]
        Pop { args { dst: usize } }

        #[verify(stack_balance = -1)]
        Return { args { src: usize } }
    }
}

fn main() {
    // Build a valid program: Push(42), Return(0)
    // Push adds 1 to stack, Return removes 1, so net balance = 0
    let push = Push::new(42);
    let ret = Return::new(0);
    let result = generated::link(vec![push.into(), ret.into()]).unwrap();

    // Verify the bytecode should succeed
    let verify_result = generated::verify(&result);
    assert!(
        verify_result.is_ok(),
        "Expected verification to succeed, but got errors: {:?}",
        verify_result
    );

    println!("Verification passed for Push/Return program!");

    // Build a program that should fail verification: Pop(0) without Push
    let pop_only = Pop::new(0);
    let result2 = generated::link(vec![pop_only.into()]).unwrap();
    let verify_result2 = generated::verify(&result2);
    match verify_result2 {
        Err(errors) => {
            assert!(
                errors
                    .iter()
                    .any(|e| e.message().contains("stack underflow")),
                "Expected stack underflow error, got: {:?}",
                errors
            );
            println!("Correctly detected stack underflow: {}", errors[0]);
        }
        Ok(()) => {
            panic!("Expected verification to fail with stack underflow, but it succeeded");
        }
    }

    println!("All verify_dsl tests passed!");
}
