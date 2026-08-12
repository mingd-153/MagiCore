use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

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
                    Answer::new("esp32-rust (no_std / cargo)", "esp32-rust").with_questions(vec![
                        Self::board_question(vec![
                            Answer::new("ESP32-C3 (riscv32imac)", "esp32c3"),
                            Answer::new("ESP32-S3 (xtensa)", "esp32s3"),
                            Answer::new("ESP32 (xtensa)", "esp32"),
                        ]),
                    ]),
                    Answer::new("platformio (pio)", "platformio").with_questions(vec![
                        Self::board_question(vec![
                            Answer::new("ESP32 DevKit (esp32dev)", "esp32dev"),
                            Answer::new("NodeMCU-32S", "nodemcu-32s"),
                        ]),
                    ]),
                    Answer::new("zephyr (west / ARM)", "zephyr-arm").with_questions(vec![
                        Self::board_question(vec![
                            Answer::new("nRF52 DK (nrf52dk_nrf52832)", "nrf52dk_nrf52832"),
                            Answer::new("STM32F4 Discovery", "stm32f4_disc"),
                        ]),
                    ]),
                ],
            },
        }
    }

    fn board_question(options: Vec<Answer>) -> Question {
        Question {
            prompt: "\n  Select target board:".to_string(),
            kind: QuestionKind::Select { options },
        }
    }
}
