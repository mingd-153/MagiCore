use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

#[cfg(test)]
#[path = "../test/wizard_iot_test.rs"]
mod tests;

pub struct IotWizard;

impl IotWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let mut it = results.into_iter();
        let framework = it.next().unwrap_or_default();
        let board = it.next().unwrap_or_else(|| Self::default_board(&framework));

        ScaffoldConfig {
            core: "iot".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![board],
            template_dir: PathBuf::new(),
        }
    }

    fn default_board(framework: &str) -> String {
        match framework {
            "esp32-rust" => "esp32c3".to_string(),
            "platformio" => "esp32dev".to_string(),
            _ => "nrf52dk_nrf52832".to_string(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select IoT framework:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("esp32-rust (no_std / cargo)", "esp32-rust")
                        .with_questions(vec![Self::board_question("esp32-rust")]),
                    Answer::new("platformio (pio)", "platformio")
                        .with_questions(vec![Self::board_question("platformio")]),
                    Answer::new("zephyr (west / ARM)", "zephyr-arm")
                        .with_questions(vec![Self::board_question("zephyr")]),
                ],
            },
        }
    }

    fn board_question(framework: &str) -> Question {
        let options = mgc_iot_adapter::boards_for_framework(framework)
            .into_iter()
            .map(|board| Answer::new(&board.label, &board.id))
            .collect();
        Question {
            prompt: "\n  Select target board:".to_string(),
            kind: QuestionKind::Select { options },
        }
    }
}
