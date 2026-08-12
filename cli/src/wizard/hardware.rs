use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct HardwareWizard;

impl HardwareWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let framework = results.first().cloned().unwrap_or_default();

        ScaffoldConfig {
            core: "hardware".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select hardware package:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new(
                        "optimizer (GPU/CPU texture+mesh optimize, FFI C ABI)",
                        "optimizer",
                    ),
                    Answer::new("bench (GPU vs CPU benchmark harness)", "bench"),
                ],
            },
        }
    }
}
