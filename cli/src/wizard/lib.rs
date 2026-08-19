use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct LibWizard;

impl LibWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let language = results.first().cloned().unwrap_or_default();

        ScaffoldConfig {
            core: "lib".to_string(),
            sub_type: String::new(),
            frameworks: vec![language],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select library language:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("TypeScript (npm)", "ts"),
                    Answer::new("Rust (cargo)", "rust"),
                    Answer::new("Python (pip)", "python"),
                ],
            },
        }
    }
}
