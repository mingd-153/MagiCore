use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct CicdWizard;

impl CicdWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let mut it = results.into_iter();
        let framework = it.next().unwrap_or_default();

        ScaffoldConfig {
            core: "cicd".to_string(),
            sub_type: String::new(),
            frameworks: vec![framework],
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        Question {
            prompt: "\n  Select CI/CD provider:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("Cloudflare Workers (wrangler)", "cloudflare"),
                    Answer::new("AWS (s3 sync / deployment)", "aws"),
                    Answer::new("GCP (gcloud app deploy)", "gcp"),
                    Answer::new("GitHub Actions (CI only)", "github-actions"),
                    Answer::new("ArgoCD (Kubernetes GitOps)", "argocd"),
                ],
            },
        }
    }
}
