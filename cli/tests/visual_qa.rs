//! Visual QA tests — scaffold frontend framework, start dev server, screenshot via Playwright.
//! Run: cargo test --test visual_qa -- --ignored
//! Requires: node + npx playwright installed.
#![allow(dead_code)]

use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn project_dir(base: &str, framework: &str) -> String {
    format!("{}/visual-qa-{}", base, framework)
}

fn scaffold_project(framework: &str, dir: &str) {
    assert!(
        Command::new("cargo")
            .args([
                "run",
                "--bin",
                "mg",
                "--",
                "create-web",
                framework,
                &format!("visual-qa-{}", framework),
                "--ts",
            ])
            .current_dir(dir)
            .status()
            .expect("failed to run mg create-web")
            .success(),
        "scaffold {} failed",
        framework
    );
}

fn install_deps(framework: &str, dir: &str) {
    let pdir = project_dir(dir, framework);
    assert!(
        Command::new("cargo")
            .args(["run", "--bin", "mg", "--", "install-web"])
            .current_dir(&pdir)
            .status()
            .expect("failed to run mg install-web")
            .success(),
        "mg install-web failed for {}",
        framework
    );
}

fn start_dev_server(framework: &str, dir: &str, port: u16) -> Child {
    let pdir = project_dir(dir, framework);
    let dev_cmd = format!("npx vite --port {} --host 127.0.0.1", port);
    Command::new("sh")
        .args(["-c", &dev_cmd])
        .current_dir(&pdir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to start dev server")
}

fn wait_for_server(port: u16, timeout: Duration) {
    let start = Instant::now();
    let url = format!("http://127.0.0.1:{}", port);
    while start.elapsed() < timeout {
        if let Ok(out) = Command::new("curl")
            .args(["-s", "-o", "/dev/null", "-w", "%{http_code}", &url])
            .output()
        {
            let code = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if code == "200" || code == "304" {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!("server on port {} did not start within {:?}", port, timeout);
}

fn run_playwright_tests(framework: &str, port: u16) {
    let spec_dir = format!("target/.vqa-{}", framework);
    std::fs::create_dir_all(&spec_dir).unwrap();

    // Install @playwright/test locally
    std::fs::write(
        format!("{}/package.json", spec_dir),
        r#"{"private":true,"devDependencies":{"@playwright/test":"latest"}}"#,
    )
    .unwrap();
    assert!(
        Command::new("npm")
            .args(["install", "--silent"])
            .current_dir(&spec_dir)
            .status()
            .expect("npm install failed")
            .success(),
        "npm install @playwright/test failed"
    );

    std::fs::write(
        format!("{}/spec.spec.ts", spec_dir),
        format!(
            r#"
import {{ test, expect }} from '@playwright/test';
test('{fw} loads', async ({{ page }}) => {{
    await page.goto('http://127.0.0.1:{port}', {{ waitUntil: 'networkidle' }});
    await expect(page.locator('body')).not.toBeEmpty();
    await page.screenshot({{ path: 'screenshots/{fw}.png', fullPage: true }});
}});
test('{fw} no console errors', async ({{ page }}) => {{
    const errors: string[] = [];
    page.on('console', msg => {{ if (msg.type() === 'error') errors.push(msg.text()); }});
    await page.goto('http://127.0.0.1:{port}', {{ waitUntil: 'networkidle' }});
    await page.waitForTimeout(500);
    expect(errors).toEqual([]);
}});
"#,
            fw = framework,
            port = port
        ),
    )
    .unwrap();
    std::fs::write(
        format!("{}/playwright.config.ts", spec_dir),
        r#"import { defineConfig } from '@playwright/test';
export default defineConfig({
    testDir: '.',
    timeout: 30000,
    use: { headless: true, viewport: { width: 1280, height: 720 } },
});
"#,
    )
    .unwrap();
    std::fs::create_dir_all(format!("{}/screenshots", spec_dir)).unwrap();

    let output = Command::new("npx")
        .args(["playwright", "test", "--config", "playwright.config.ts"])
        .current_dir(&spec_dir)
        .output()
        .expect("failed to run playwright");
    if !output.status.success() {
        eprintln!("stdout:\n{}", String::from_utf8_lossy(&output.stdout));
        eprintln!("stderr:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    assert!(
        output.status.success(),
        "Playwright tests failed for {}",
        framework
    );
}

fn run_framework_test(framework: &str, port: u16) {
    let dir = format!("target/.vqa-base");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    scaffold_project(framework, &dir);
    install_deps(framework, &dir);
    let mut child = start_dev_server(framework, &dir, port);
    wait_for_server(port, Duration::from_secs(30));
    run_playwright_tests(framework, port);
    child.kill().ok();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore]
fn visual_qa_react_vite() {
    run_framework_test("react-vite", 4315);
}

#[test]
#[ignore]
fn visual_qa_vue_vite() {
    run_framework_test("vue-vite", 4316);
}
