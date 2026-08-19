use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct AppWizard;

impl AppWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let mut it = results.into_iter();
        let framework = it.next().unwrap_or_default();

        ScaffoldConfig {
            core: "app".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select app language:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new(
                        "Multi-platform (KMP shared + android/ios/react-native/flutter)",
                        "multi",
                    ),
                    Answer::new("Flutter (Dart)", "flutter"),
                    Answer::new("React Native (TypeScript)", "react-native"),
                    Answer::new("Tauri (Rust + Web Desktop)", "tauri"),
                    Answer::new("Kotlin (Android / Gradle)", "kotlin"),
                    Answer::new("Swift (iOS / SPM)", "swift"),
                    Answer::new(".NET MAUI (C# Cross-platform)", "maui"),
                ],
            },
        }

    }
}
