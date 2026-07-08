/// PubGrub resolver for npm dependencies
use anyhow::Result;

pub struct Resolver;

impl Resolver {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve(&self, _deps: &[String]) -> Result<Vec<String>> {
        Ok(vec![])
    }
}
