use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct GameWizard;

impl GameWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let framework = results.first().cloned().unwrap_or_default();

        ScaffoldConfig {
            core: "game".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select game engine:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("Bevy (Rust / cargo)", "bevy"),
                    Answer::new("Godot", "godot"),
                    Answer::new("Unity (UPM)", "unity"),
                    Answer::new("Unreal Engine", "unreal"),
                ],
            },
        }
    }
}
