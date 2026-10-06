//! CICD scaffold: argocd/cloudflare/github-actions templates.

use std::path::Path;

use anyhow::Result;

use super::write_file;

pub struct CicdProcessor;

impl CicdProcessor {
    /// Frameworks generated directly (the `_` arm is the generic
    /// github-actions workflow). `aws`/`gcp` have no template — they keep
    /// the layer-required error instead of a mislabeled workflow.
    /// (aws/gcp chưa có template — fail rõ thay vì workflow sai nhãn.)
    pub fn supports(framework: &str) -> bool {
        matches!(framework, "argocd" | "cloudflare" | "github-actions")
    }

    pub fn files(target: &Path, _name: &str, framework: &str) -> Result<()> {
        match framework {
            "argocd" => write_file(
                &target.join("argocd").join("application.yaml"),
                "apiVersion: argoproj.io/v1alpha1\nkind: Application\nmetadata:\n  name: magicore-app\nspec: {}\n",
            )?,
            "cloudflare" => {
                write_file(
                    &target.join("wrangler.toml"),
                    "name = \"worker\"\nmain = \"src/index.js\"\ncompatibility_date = \"2026-01-01\"\n",
                )?;
                write_file(
                    &target.join("src").join("index.js"),
                    "export default {\n  async fetch(request) {\n    return new Response(\"Hello from MagiCore Worker\", { status: 200 });\n  },\n};\n",
                )?;
            }
            _ => write_file(
                &target.join(".github").join("workflows").join("ci.yml"),
                "name: CI\n\non:\n  push:\n  pull_request:\n\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      # actions/checkout v4.4.0 pinned to its verified commit. / Ghim actions/checkout v4.4.0 vào commit đã xác minh.\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n      - run: echo \"MagiCore CI scaffold\"\n\n  publish:\n    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/') && vars.MAGICORE_TRUSTED_PUBLISH == 'true'\n    permissions:\n      contents: read\n      id-token: write\n    runs-on: ubuntu-latest\n    steps:\n      # actions/checkout v4.4.0 pinned to its verified commit. / Ghim actions/checkout v4.4.0 vào commit đã xác minh.\n      - uses: actions/checkout@11d5960a326750d5838078e36cf38b85af677262\n        with:\n          persist-credentials: false\n      - name: Install pinned MagiCore CLI\n        shell: bash\n        env:\n          MAGICORE_CLI_VERSION: ${{ vars.MAGICORE_CLI_VERSION }}\n          MAGICORE_CLI_SHA256: ${{ vars.MAGICORE_CLI_SHA256 }}\n        run: |\n          if [[ ! \"$MAGICORE_CLI_VERSION\" =~ ^v?[0-9]+\\.[0-9]+\\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then echo \"::error::Set MAGICORE_CLI_VERSION to a valid release version\"; exit 1; fi\n          if [[ ! \"$MAGICORE_CLI_SHA256\" =~ ^[a-fA-F0-9]{64}$ ]]; then echo \"::error::Set MAGICORE_CLI_SHA256 to the release archive SHA-256\"; exit 1; fi\n          VERSION_NUMBER=\"${MAGICORE_CLI_VERSION#v}\"\n          ARCHIVE=\"magicore-${VERSION_NUMBER}-linux-x64.tar.gz\"\n          curl -fsSL --proto '=https' --tlsv1.2 \"https://github.com/mingd-153/MagiCore/releases/download/v${VERSION_NUMBER}/${ARCHIVE}\" -o \"$RUNNER_TEMP/$ARCHIVE\"\n          printf '%s  %s\\n' \"$MAGICORE_CLI_SHA256\" \"$RUNNER_TEMP/$ARCHIVE\" | sha256sum --check --strict -\n          mkdir -p \"$RUNNER_TEMP/bin\" \"$RUNNER_TEMP/mgc-extract\"\n          tar -xzf \"$RUNNER_TEMP/$ARCHIVE\" -C \"$RUNNER_TEMP/mgc-extract\"\n          if [[ -f \"$RUNNER_TEMP/mgc-extract/mgc\" ]]; then BIN=\"$RUNNER_TEMP/mgc-extract/mgc\"; elif [[ -f \"$RUNNER_TEMP/mgc-extract/magicore/mgc\" ]]; then BIN=\"$RUNNER_TEMP/mgc-extract/magicore/mgc\"; else echo \"::error::mgc binary missing from verified archive\"; exit 1; fi\n          install \"$BIN\" \"$RUNNER_TEMP/bin/mgc\"\n          echo \"$RUNNER_TEMP/bin\" >> \"$GITHUB_PATH\"\n      - name: Publish OCI artifact with trusted OIDC\n        env:\n          MGC_REGISTRY_URL: ${{ vars.MAGICORE_REGISTRY_URL }}\n          MGC_OIDC_AUDIENCE: ${{ vars.MAGICORE_OIDC_AUDIENCE }}\n          MGC_PUBLISH_CORE: ${{ vars.MAGICORE_PUBLISH_CORE }}\n          MGC_PUBLISH_PACKAGE: ${{ vars.MAGICORE_PUBLISH_PACKAGE }}\n          MGC_PUBLISH_ARTIFACT: ${{ vars.MAGICORE_PUBLISH_ARTIFACT }}\n          MGC_PUBLISH_VERSION: ${{ github.ref_name }}\n        run: |\n          if [[ -z \"$MGC_REGISTRY_URL\" || -z \"$MGC_OIDC_AUDIENCE\" || -z \"$MGC_PUBLISH_CORE\" || -z \"$MGC_PUBLISH_PACKAGE\" || -z \"$MGC_PUBLISH_VERSION\" ]]; then echo \"::error::Set the registry, audience, core, package, and version workflow variables\"; exit 1; fi\n          case \"$MGC_PUBLISH_CORE\" in web|ai|app|lib|game|iot|cloud|cicd|hardware) ;; *) echo \"::error::MGC_PUBLISH_CORE must name a supported MagiCore core\"; exit 1 ;; esac\n          if [[ -z \"$MGC_PUBLISH_ARTIFACT\" ]]; then MGC_PUBLISH_ARTIFACT=\"$RUNNER_TEMP/mgc-source.tar.gz\"; git archive --format=tar.gz --output=\"$MGC_PUBLISH_ARTIFACT\" HEAD; fi\n          if [[ ! -f \"$MGC_PUBLISH_ARTIFACT\" ]]; then echo \"::error::MGC_PUBLISH_ARTIFACT must point to a file or be empty to publish the tagged source archive\"; exit 1; fi\n          mgc publish --trusted --protocol oci --core \"$MGC_PUBLISH_CORE\" --package \"$MGC_PUBLISH_PACKAGE\" --version \"$MGC_PUBLISH_VERSION\" --artifact \"$MGC_PUBLISH_ARTIFACT\" --registry \"$MGC_REGISTRY_URL\" --trusted-audience \"$MGC_OIDC_AUDIENCE\"\n",
            )?,
        }

        Ok(())
    }
}
