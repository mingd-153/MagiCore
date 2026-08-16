use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct CloudWizard;

impl CloudWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let mut it = results.into_iter();
        let framework = it.next().unwrap_or_default();

        ScaffoldConfig {
            core: "clo".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select cloud framework:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("CDK (AWS TypeScript)", "cdk"),
                    Answer::new("Pulumi (nodejs)", "pulumi"),
                    Answer::new("Terraform (HCL)", "terraform"),
                    Answer::new("Cloudflare Workers (wrangler)", "cloudflare"),
                ],
            },
        }
    }
}
