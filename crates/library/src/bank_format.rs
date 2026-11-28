//! The bank format of the main repository, which shared content follows.
//!
//! Its JSON Schema check comes with issue #5 (blocked on the main
//! repository publishing the schema). Until then [`check`] holds only what
//! any bank or learning path is: a JSON object. Every share goes through it
//! before the review, so the schema slots in here alone.

use serde_json::Value;

use crate::ItemType;

/// Why content is not in the bank format.
#[derive(Debug)]
pub struct BadContent;

/// Checks that `content` is a bank or a learning path, as `kind` says, in
/// the bank format.
pub fn check(_kind: ItemType, content: &Value) -> Result<(), BadContent> {
    if content.is_object() {
        Ok(())
    } else {
        Err(BadContent)
    }
}
