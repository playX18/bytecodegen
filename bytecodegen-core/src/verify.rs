/// Error returned when verification fails.
#[derive(Debug, Clone)]
pub struct VerificationError {
    ordinal: usize,
    message: String,
}

impl VerificationError {
    pub fn new(ordinal: usize, message: impl Into<String>) -> Self {
        Self {
            ordinal,
            message: message.into(),
        }
    }

    pub fn ordinal(&self) -> usize {
        self.ordinal
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "verification error at instruction {}: {}",
            self.ordinal, self.message
        )
    }
}

impl std::error::Error for VerificationError {}

/// A verifier that walks a linked bytecode and checks invariants.
///
/// The `State` type tracks analysis state (e.g., stack height, type state).
/// Each `check` call receives the opcode value and mutable state,
/// returning `Ok(())` if the check passes or `Err(VerificationError)` if it fails.
pub trait Verifier {
    type State: Default;

    fn check(opcode: u32, state: &mut Self::State) -> Result<(), VerificationError>;
}

/// Result of running verification on a linked bytecode.
pub type VerificationResult = Result<(), Vec<VerificationError>>;
