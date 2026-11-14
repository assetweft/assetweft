//! Responsibility: compatibility path for Production generation specifications.
//! Dependencies: Production specification; callers: existing CLI and library consumers.
//! Invariant: these are the same items, with no second validator or DTO definitions.
pub use crate::production::specification::{
    Budget, CompletionClaim, ComponentRole, GenerationSpecRequest, InferenceState, IssueCode,
    LockClaim, RECIPE_STATUS, RecipeExample, ReferencePurpose, ReferenceUse,
    STATE_PENDING_CONFIRMATION, STATUS_DECLARATION_CONSISTENT, STATUS_REJECTED, SpecIssue,
    TrustedContext, VALIDATOR_VERSION, ValidationOutcome, recipe_examples, validate,
};
