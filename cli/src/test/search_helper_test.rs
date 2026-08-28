#![cfg(test)]
#![allow(clippy::unwrap_used)]
//! Tests for search helper functions

use super::*;

#[test]
fn test_should_trigger_search() {
    // Should trigger
    assert!(should_trigger_search("express"));
    assert!(should_trigger_search("gin"));
    assert!(should_trigger_search("requests"));
    
    // Should not trigger
    assert!(!should_trigger_search("express@4.18.0"));
    assert!(!should_trigger_search("express@^4.0.0"));
    assert!(!should_trigger_search("@types/node"));
    assert!(!should_trigger_search("./local"));
    assert!(!should_trigger_search("../sibling"));
    assert!(!should_trigger_search("file:../local"));
    assert!(!should_trigger_search("https://github.com/user/repo"));
    assert!(!should_trigger_search("github.com/user/repo"));
}
