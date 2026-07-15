use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Result of a completed wizard flow
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScaffoldConfig {
    pub core: String,
    pub sub_type: String,
    pub frameworks: Vec<String>,
    pub project_name: String,
    pub features: Vec<String>,
    pub template_dir: PathBuf,
}

/// A wizard question with possible answers
pub struct Question {
    pub prompt: String,
    pub kind: QuestionKind,
}

pub enum QuestionKind {
    Select {
        options: Vec<Answer>,
    },
    MultiSelect {
        options: Vec<Answer>,
    },
    #[allow(dead_code)]
    Input {
        default: Option<String>,
    },
}

pub struct Answer {
    pub label: String,
    pub value: String,
    pub next_questions: Vec<Question>,
}

impl Answer {
    pub fn new(label: &str, value: &str) -> Self {
        Self {
            label: label.to_string(),
            value: value.to_string(),
            next_questions: vec![],
        }
    }

    pub fn with_questions(mut self, questions: Vec<Question>) -> Self {
        self.next_questions = questions;
        self
    }
}

/// Shared TUI engine for all wizard flows
pub struct WizardEngine;

impl WizardEngine {
    pub fn run_question(question: &Question) -> Vec<String> {
        match &question.kind {
            QuestionKind::Select { options } => {
                let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
                let idx = mg_ui::prompt::select(&question.prompt, &labels).unwrap_or(0);
                let chosen = &options[idx];
                Self::run_next(chosen)
            }
            QuestionKind::MultiSelect { options } => {
                let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
                let indices =
                    mg_ui::prompt::multi_select(&question.prompt, &labels).unwrap_or(vec![]);
                let mut results = vec![];
                for &i in &indices {
                    let chosen = &options[i];
                    results.push(chosen.value.clone());
                    results.extend(Self::run_next_recursive(&chosen.next_questions));
                }
                results
            }
            QuestionKind::Input { default } => {
                let answer = mg_ui::prompt::input(&question.prompt)
                    .unwrap_or(default.clone().unwrap_or_default());
                vec![answer]
            }
        }
    }

    fn run_next(answer: &Answer) -> Vec<String> {
        let mut results = vec![answer.value.clone()];
        results.extend(Self::run_next_recursive(&answer.next_questions));
        results
    }

    fn run_next_recursive(questions: &[Question]) -> Vec<String> {
        let mut results = vec![];
        for q in questions {
            results.extend(Self::run_question(q));
        }
        results
    }
}
