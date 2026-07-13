use crate::wizard::engine::{Answer, Question, QuestionKind, ScaffoldConfig, WizardEngine};
use std::path::PathBuf;

pub struct WebWizard;

impl WebWizard {
    pub fn run() -> ScaffoldConfig {
        let root = Self::build_tree();
        let results = WizardEngine::run_question(&root);
        let frameworks = if results.len() > 1 {
            results[1..].to_vec()
        } else {
            vec![]
        };

        ScaffoldConfig {
            core: "web".to_string(),
            sub_type: results.first().cloned().unwrap_or_default(),
            frameworks,
            project_name: String::new(),
            features: vec![],
            template_dir: PathBuf::new(),
        }
    }

    fn build_tree() -> Question {
        let frontend = Answer::new("Frontend", "frontend").with_questions(vec![Question {
            prompt: "\n  Select a framework:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("Next.js", "nextjs"),
                    Answer::new("React + Vite", "react-vite"),
                    Answer::new("Vue + Vite", "vue-vite"),
                    Answer::new("Nuxt", "nuxt"),
                    Answer::new("SvelteKit", "sveltekit"),
                    Answer::new("Angular", "angular"),
                    Answer::new("Solid.js", "solidjs"),
                    Answer::new("Qwik", "qwik"),
                    Answer::new("Vanilla (HTML + TS)", "vanilla"),
                    Answer::new("Astro", "astro"),
                ],
            },
        }]);

        let backend = Answer::new("Backend", "backend").with_questions(vec![Question {
            prompt: "\n  Select language:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("Node.js / TypeScript", "node").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("Express", "express"),
                                Answer::new("Fastify", "fastify"),
                                Answer::new("NestJS", "nestjs"),
                                Answer::new("Hono", "hono"),
                                Answer::new("tRPC (server)", "trpc"),
                            ],
                        },
                    }]),
                    Answer::new("PHP", "php").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("Laravel", "laravel"),
                                Answer::new("Symfony", "symfony"),
                            ],
                        },
                    }]),
                    Answer::new("Java", "java").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("Spring Boot", "spring-boot"),
                                Answer::new("Quarkus", "quarkus"),
                            ],
                        },
                    }]),
                    Answer::new("Go", "go").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("Gin", "gin"),
                                Answer::new("Echo", "echo"),
                                Answer::new("Fiber", "fiber"),
                            ],
                        },
                    }]),
                    Answer::new("Python", "python").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("FastAPI", "fastapi"),
                                Answer::new("Django", "django"),
                                Answer::new("Flask", "flask"),
                            ],
                        },
                    }]),
                    Answer::new("Rust", "rust").with_questions(vec![Question {
                        prompt: "\n  Select framework:".to_string(),
                        kind: QuestionKind::Select {
                            options: vec![
                                Answer::new("Axum", "axum"),
                                Answer::new("Actix-Web", "actix-web"),
                            ],
                        },
                    }]),
                ],
            },
        }]);

        let fullstack = Answer::new("Fullstack", "fullstack").with_questions(vec![Question {
            prompt: "\n  Select stack:".to_string(),
            kind: QuestionKind::Select {
                options: vec![
                    Answer::new("Next.js (FE + BE all-in-one)", "nextjs"),
                    Answer::new("Nuxt (FE + BE all-in-one)", "nuxt"),
                    Answer::new("SvelteKit (FE + BE all-in-one)", "sveltekit"),
                    Answer::new("Remix (FE + BE all-in-one)", "remix"),
                    Answer::new("React + Fastify (separate)", "react-fastify"),
                    Answer::new("Vue + Laravel (separate)", "vue-laravel"),
                    Answer::new("React + Spring Boot (separate)", "react-spring"),
                    Answer::new("Custom (pick your own)", "custom"),
                ],
            },
        }]);

        let monorepo = Answer::new("Monorepo", "monorepo").with_questions(vec![
            Question {
                prompt: "\n  Select FE framework:".to_string(),
                kind: QuestionKind::Select {
                    options: vec![
                        Answer::new("Next.js", "nextjs"),
                        Answer::new("React + Vite", "react-vite"),
                        Answer::new("Vue + Vite", "vue-vite"),
                        Answer::new("Nuxt", "nuxt"),
                        Answer::new("SvelteKit", "sveltekit"),
                        Answer::new("Angular", "angular"),
                        Answer::new("Solid.js", "solidjs"),
                        Answer::new("Qwik", "qwik"),
                        Answer::new("Astro", "astro"),
                        Answer::new("Vanilla", "vanilla"),
                    ],
                },
            },
            Question {
                prompt: "\n  Select BE framework:".to_string(),
                kind: QuestionKind::Select {
                    options: vec![
                        Answer::new("NestJS", "nestjs"),
                        Answer::new("Express", "express"),
                        Answer::new("Fastify", "fastify"),
                        Answer::new("Hono", "hono"),
                        Answer::new("tRPC (server)", "trpc"),
                        Answer::new("Laravel", "laravel"),
                        Answer::new("Symfony", "symfony"),
                        Answer::new("Spring Boot", "spring-boot"),
                        Answer::new("Quarkus", "quarkus"),
                        Answer::new("Go + Gin", "gin"),
                        Answer::new("Go + Echo", "echo"),
                        Answer::new("Go + Fiber", "fiber"),
                        Answer::new("FastAPI", "fastapi"),
                        Answer::new("Django", "django"),
                        Answer::new("Flask", "flask"),
                        Answer::new("Rust + Axum", "axum"),
                        Answer::new("Rust + Actix-Web", "actix-web"),
                    ],
                },
            },
        ]);

        Question {
            prompt: "\n  What type of web project?".to_string(),
            kind: QuestionKind::Select {
                options: vec![frontend, backend, fullstack, monorepo],
            },
        }
    }
}
