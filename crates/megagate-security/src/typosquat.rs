use megagate_types::error::Result;
use strsim::levenshtein;
use unicode_normalization::UnicodeNormalization;

pub struct TyposquatDetector {
    known_packages: Vec<String>,
    threshold: usize,
}

impl TyposquatDetector {
    pub fn new(known_packages: Vec<String>) -> Self {
        Self {
            known_packages,
            threshold: 2,
        }
    }

    pub fn check(&self, name: &str) -> Result<Vec<TyposquatMatch>> {
        let normalized = self.normalize(name);
        let mut matches = Vec::new();

        for known in &self.known_packages {
            let known_normalized = self.normalize(known);
            if normalized == known_normalized {
                continue;
            }
            let distance = levenshtein(&normalized, &known_normalized);
            if distance <= self.threshold {
                matches.push(TyposquatMatch {
                    input: name.to_string(),
                    matched: known.clone(),
                    distance,
                    confidence: self.calculate_confidence(distance, name.len()),
                });
            }
        }

        matches.sort_by(|a, b| a.distance.cmp(&b.distance));
        Ok(matches)
    }

    fn normalize(&self, name: &str) -> String {
        name.nfkc().collect::<String>()
            .to_lowercase()
            .replace('-', "")
            .replace('_', "")
            .replace('.', "")
    }

    fn calculate_confidence(&self, distance: usize, len: usize) -> f64 {
        if len == 0 {
            return 0.0;
        }
        1.0 - (distance as f64 / len as f64).min(1.0)
    }
}

#[derive(Debug, Clone)]
pub struct TyposquatMatch {
    pub input: String,
    pub matched: String,
    pub distance: usize,
    pub confidence: f64,
}

pub fn is_suspicious(name: &str, known: &[String]) -> bool {
    let detector = TyposquatDetector::new(known.to_vec());
    detector.check(name).unwrap().iter().any(|m| m.confidence > 0.8)
}