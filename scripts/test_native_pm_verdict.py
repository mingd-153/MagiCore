#!/usr/bin/env python3
"""Regression tests for the native package-manager evidence verdict."""

import io
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path, PureWindowsPath
from contextlib import redirect_stdout
from unittest.mock import patch
import lifecycle_capability_matrix as matrix_module

from lifecycle_capability_matrix import (
    NATIVE_PM_REQUIRED_DIMENSIONS,
    NATIVE_PM_USER_OPERATIONS,
    ALL_DEPENDENCY_OPERATIONS,
    PLATFORM_EVIDENCE_RELEASE_SCOPE,
    ALL_DIMENSIONS,
    LANES,
    validate_lane_registry,
    SCHEMA_VERSION,
    STATUS_NATIVE,
    STATUS_PLAIN,
    STATUS_FAILED,
    STATUS_UNVERIFIED,
    STATUS_UNSUPPORTED,
    native_pm_supported,
    native_pm_unavailable_verdict,
    native_pm_claim_scope,
    native_pm_claim_errors,
    native_pm_lane_errors,
    lifecycle_lane_owner_contract_errors,
    lifecycle_status_owner_matches,
    lifecycle_owner_for,
    lifecycle_pass_status,
    lifecycle_status_for_owner,
    framework_catalog_errors,
    release_scope_out_reason,
    evidence_commit_error,
    scaffold_language_matches,
    platform_evidence_errors,
    record_platform_green,
    print_dependency_owner_summary,
    lifecycle_platform_name,
    lifecycle_environment,
    pulumi_local_environment,
    recovery_environment,
    python_venv_environment,
    provision_python_build_tools,
    validate_adapter_consistency,
    _materialize_marker,
    lifecycle_working_tree_clean,
    lifecycle_source_matches,
    run_lane,
)
from provenance_chain import _matrix_sha_for_head


def bash_compatible_path(path, windows=None):
    value = os.fspath(path)
    if windows is None:
        windows = os.name == "nt"
    if not windows:
        return value

    windows_path = PureWindowsPath(value)
    drive = windows_path.drive.rstrip(":").lower()
    if not drive:
        raise ValueError(f"Windows path must include a drive letter: {value}")

    # Git Bash exposes local Windows drives below /<drive>/.
    # Git Bash hiển thị các ổ đĩa Windows cục bộ dưới /<drive>/.
    return f"/{drive}/" + "/".join(windows_path.parts[1:])


def bootstrap_bash_command(platform=None, git_executable=None):
    if platform is None:
        platform = os.name
    if platform != "nt":
        return "bash"
    if git_executable is None:
        git_executable = shutil.which("git")
    if not git_executable:
        return None

    # Bind platform choice and Windows shell resolution in one auditable function.
    # Gộp chọn shell theo hệ điều hành và tìm Bash trên Windows để audit chung.
    git_path = Path(git_executable).resolve()
    for install_directory in (git_path.parent, *git_path.parents):
        for relative_path in (Path("bin/bash.exe"), Path("usr/bin/bash.exe")):
            candidate = install_directory / relative_path
            if candidate.is_file():
                return str(candidate.resolve())
    return None


class GitHubWorkflowAnnotationEscaping(unittest.TestCase):
    def test_annotation_data_cannot_inject_another_workflow_command(self):
        self.assertEqual(
            matrix_module.github_actions_annotation_escape(
                "gate%failure\r\n::warning::injected"
            ),
            "gate%25failure%0D%0A::warning::injected",
        )

    def test_gate_annotations_stay_within_limit_and_count_omitted_failures(self):
        failures = [f"gate failure {index}" for index in range(12)]

        annotations = matrix_module.github_actions_error_annotations(failures)

        self.assertEqual(len(annotations), 10)
        self.assertEqual(annotations[:9], [f"::error::{failure}" for failure in failures[:9]])
        self.assertEqual(
            annotations[-1],
            "::error::3 additional gate failures; full details are in this step log",
        )


class AdapterConsistencyGate(unittest.TestCase):
    def test_every_lane_matches_its_real_adapter_or_embedded_native_route(self):
        output = io.StringIO()
        with redirect_stdout(output):
            result = validate_adapter_consistency()

        self.assertEqual(result, 0, output.getvalue())
        self.assertIn("22 lanes", output.getvalue())

    def test_cloud_native_exceptions_are_framework_scoped(self):
        from lifecycle_capability_matrix import CLI_NATIVE_DEPENDENCY_ROUTES

        self.assertIn(("clo", "cdk"), CLI_NATIVE_DEPENDENCY_ROUTES)
        self.assertIn(("clo", "pulumi"), CLI_NATIVE_DEPENDENCY_ROUTES)
        self.assertNotIn(("clo", "terraform"), CLI_NATIVE_DEPENDENCY_ROUTES)


class LifecycleEnvironmentIsolation(unittest.TestCase):
    def test_bootstrap_bash_command_uses_git_for_windows_and_posix_bash(self):
        with tempfile.TemporaryDirectory(prefix="git bash lookup ") as tmp:
            git_root = Path(tmp) / "Program Files" / "Git"
            git_executable = git_root / "cmd" / "git.exe"
            git_executable.parent.mkdir(parents=True)
            git_executable.touch()
            expected_bash = git_root / "bin" / "bash.exe"
            expected_bash.parent.mkdir(parents=True)
            expected_bash.touch()

            self.assertEqual(
                bootstrap_bash_command("nt", git_executable),
                str(expected_bash.resolve()),
            )
            self.assertEqual(bootstrap_bash_command("posix"), "bash")

    def test_bash_compatible_path_maps_windows_drive_to_msys_mount(self):
        self.assertEqual(
            bash_compatible_path(r"D:\a\_temp\fake Flutter SDK", windows=True),
            "/d/a/_temp/fake Flutter SDK",
        )

    def test_bash_compatible_path_preserves_posix_paths(self):
        self.assertEqual(
            bash_compatible_path("/tmp/fake Flutter SDK", windows=False),
            "/tmp/fake Flutter SDK",
        )

    def test_flutter_runtime_steps_reuse_the_warmed_sdk_pub_cache(self):
        lane = {"core": "app", "language": "flutter"}
        lane_env = {"HOME": "/tmp/lane/.home", "PUB_CACHE": "/tmp/lane/.cache/pub"}
        original = dict(lane_env)

        test_env = matrix_module.lifecycle_step_environment(
            lane, "test", lane_env, "/runner/.pub-cache", "/runner/flutter-sdk-home"
        )
        build_env = matrix_module.lifecycle_step_environment(
            lane, "build", lane_env, "/runner/.pub-cache", "/runner/flutter-sdk-home"
        )
        install_env = matrix_module.lifecycle_step_environment(
            lane, "install", lane_env, "/runner/.pub-cache", "/runner/flutter-sdk-home"
        )

        self.assertEqual(test_env["PUB_CACHE"], "/runner/.pub-cache")
        self.assertEqual(build_env["PUB_CACHE"], "/runner/.pub-cache")
        self.assertEqual(test_env["HOME"], "/runner/flutter-sdk-home")
        self.assertEqual(test_env["USERPROFILE"], "/runner/flutter-sdk-home")
        self.assertEqual(build_env["HOME"], "/runner/flutter-sdk-home")
        self.assertEqual(install_env["HOME"], lane_env["HOME"])
        self.assertEqual(install_env["PUB_CACHE"], "/tmp/lane/.cache/pub")
        self.assertEqual(lane_env, original)
        self.assertIsNot(test_env, lane_env)

    def test_non_flutter_runtime_steps_keep_lane_local_pub_cache(self):
        lane = {"core": "web", "language": "ts"}
        lane_env = {"HOME": "/tmp/lane/.home", "PUB_CACHE": "/tmp/lane/.cache/pub"}

        step_env = matrix_module.lifecycle_step_environment(
            lane, "test", lane_env, "/runner/.pub-cache"
        )

        self.assertIs(step_env, lane_env)
        self.assertEqual(step_env["PUB_CACHE"], "/tmp/lane/.cache/pub")

    def test_local_flutter_runtime_keeps_lane_home_and_pub_cache_without_job_override(self):
        lane = {"core": "app", "language": "flutter"}
        lane_env = {"HOME": "/tmp/lane/.home", "PUB_CACHE": "/tmp/lane/.cache/pub"}

        step_env = matrix_module.lifecycle_step_environment(
            lane, "test", lane_env, "/home/runner/.pub-cache"
        )

        self.assertIs(step_env, lane_env)
        self.assertEqual(step_env["HOME"], "/tmp/lane/.home")
        self.assertEqual(step_env["PUB_CACHE"], "/tmp/lane/.cache/pub")

    def test_flutter_sdk_dependencies_are_resolved_before_the_lifecycle_matrix_runs(self):
        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "lifecycle-matrix.yml"
        ).read_text(encoding="utf-8")

        setup = workflow.index("Setup Flutter (app lane)")
        warm = workflow.index(
            "Resolve Flutter SDK dependencies before MGC process guard", setup
        )
        matrix = workflow.index("Generate lifecycle matrix (real binary, real steps)")
        self.assertLess(setup, warm)
        self.assertLess(warm, matrix)
        self.assertIn("run: bash scripts/bootstrap_flutter_sdk.sh", workflow[warm:matrix])
        job_cache = "PUB_CACHE: ${{ runner.temp }}/flutter-pub-cache"
        self.assertIn(job_cache, workflow[warm:matrix])
        self.assertIn(job_cache, workflow[matrix:])
        job_home = "FLUTTER_SDK_HOME: ${{ runner.temp }}/flutter-sdk-home"
        self.assertIn(job_home, workflow[warm:matrix])
        self.assertIn(job_home, workflow[matrix:])
        self.assertIn("HOME: ${{ runner.temp }}/flutter-sdk-home", workflow[warm:matrix])
        self.assertIn(
            "USERPROFILE: ${{ runner.temp }}/flutter-sdk-home", workflow[warm:matrix]
        )

    def test_windows_spawn_job_resolves_flutter_sdk_dependencies_before_guard_tests(self):
        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "ci.yml"
        ).read_text(encoding="utf-8")

        setup = workflow.index("Setup Flutter (for .bat spawn tests)")
        warm = workflow.index(
            "Resolve Flutter SDK dependencies before MGC process guard", setup
        )
        tests = workflow.index("Run Windows spawn tests")
        self.assertLess(setup, warm)
        self.assertLess(warm, tests)
        self.assertIn("run: bash scripts/bootstrap_flutter_sdk.sh", workflow[warm:tests])

    def test_guarded_flutter_workflows_resolve_sdk_tool_dependencies(self):
        workflow_dir = Path(__file__).resolve().parent.parent / ".github" / "workflows"
        workflows = [
            (
                "release-binary-e2e.yml",
                "Setup Flutter (app lane)",
                "Resolve Flutter SDK dependencies before MGC process guard",
                "Lifecycle app flutter (create → install → test → build)",
            ),
            (
                "delegated-compatibility-matrix.yml",
                "Setup Flutter",
                "Resolve Flutter SDK dependencies before MGC process guard",
                "Create app project",
            ),
            (
                "security.yml",
                "Setup Flutter",
                "Resolve Flutter SDK dependencies before MGC process guard",
                "Install cargo-audit (rust parity lane on this SHA)",
            ),
            (
                "ci.yml",
                "Setup Flutter SDK",
                "Resolve Flutter SDK dependencies before MGC process guard",
                "Run lifecycle tests (MUST PASS - no || true)",
            ),
        ]

        for filename, setup_name, warm_name, guarded_step in workflows:
            with self.subTest(workflow=filename):
                workflow = (workflow_dir / filename).read_text(encoding="utf-8")
                setup = workflow.index(setup_name)
                warm = workflow.index(warm_name, setup)
                guarded = workflow.index(guarded_step, warm)
                self.assertLess(setup, warm)
                self.assertLess(warm, guarded)
                self.assertIn(
                    "run: bash scripts/bootstrap_flutter_sdk.sh", workflow[warm:guarded]
                )

    def test_flutter_sdk_bootstrap_resolves_dependencies_and_warms_cli(self):
        script = (
            Path(__file__).resolve().parent.parent / "scripts" / "bootstrap_flutter_sdk.sh"
        ).read_text(encoding="utf-8")

        self.assertIn('executable="dart.exe"', script)
        self.assertIn('executable="dart"', script)
        self.assertIn(
            'pub --suppress-analytics --directory "$sdk_root/packages/flutter_tools" get --example',
            script,
        )
        self.assertIn('local flutter_bin="$sdk_root/bin/flutter"', script)
        self.assertLess(
            script.index('if [[ ! -x "$flutter_bin" ]]'),
            script.index('"$dart_bin" pub --suppress-analytics'),
        )
        self.assertIn('"$flutter_bin" --version', script)
        self.assertNotIn("flutter test --help", script)

    def test_flutter_sdk_bootstrap_invokes_dart_and_cli_for_posix_and_windows_paths(self):
        script = (
            Path(__file__).resolve().parent.parent / "scripts" / "bootstrap_flutter_sdk.sh"
        )

        with tempfile.TemporaryDirectory(prefix="flutter sdk bootstrap ") as tmp:
            temp_root = Path(tmp)
            sdk_root = temp_root / "fake Flutter SDK"
            bash_sdk_root = bash_compatible_path(sdk_root)
            flutter_tools_path = (
                f"{sdk_root.as_posix()}/packages/flutter_tools"
                if os.name == "nt"
                else f"{bash_sdk_root}/packages/flutter_tools"
            )
            expected_args = [
                "pub",
                "--suppress-analytics",
                "--directory",
                flutter_tools_path,
                "get",
                "--example",
            ]

            dart = sdk_root / "bin" / "cache" / "dart-sdk" / "bin" / (
                "dart.exe" if os.name == "nt" else "dart"
            )
            dart.parent.mkdir(parents=True, exist_ok=True)
            if os.name == "nt":
                # Compile a native fake dart.exe so Windows exercises the complete bootstrap invocation.
                # Biên dịch dart.exe giả để Windows chạy trọn luồng bootstrap.
                fake_dart_source = temp_root / "fake_dart.rs"
                fake_dart_source.write_text(
                    r"""use std::{env, fs::{File, OpenOptions}, io::Write};
fn main() {
    let mut capture = File::create(env::var("CAPTURED_ARGS").unwrap()).unwrap();
    for argument in env::args().skip(1) { writeln!(capture, "{argument}").unwrap(); }
    let mut steps = OpenOptions::new().create(true).append(true).open(env::var("BOOTSTRAP_STEPS").unwrap()).unwrap();
    writeln!(steps, "dart").unwrap();
}
""",
                    encoding="utf-8",
                )
                compiled = subprocess.run(
                    ["rustc", str(fake_dart_source), "--edition=2021", "-o", str(dart)],
                    cwd=temp_root,
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(
                    compiled.returncode,
                    0,
                    f"stdout={compiled.stdout!r}; stderr={compiled.stderr!r}",
                )
            else:
                dart.write_text(
                    "#!/bin/sh\n"
                    "printf '%s\\n' \"$@\" > \"$CAPTURED_ARGS\"\n"
                    "printf '%s\\n' dart >> \"$BOOTSTRAP_STEPS\"\n",
                    encoding="utf-8",
                )
                dart.chmod(0o755)

            flutter = sdk_root / "bin" / "flutter"
            flutter_capture = temp_root / "flutter-args.txt"
            flutter.write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$@\" > \"$FLUTTER_CAPTURED_ARGS\"\n"
                "printf '%s\\n' flutter >> \"$BOOTSTRAP_STEPS\"\n",
                encoding="utf-8",
            )
            flutter.chmod(0o755)

            capture = temp_root / "args-Linux.txt"
            bootstrap_steps = temp_root / "bootstrap-steps.txt"
            env = os.environ.copy()
            env.update(
                {
                    "FLUTTER_ROOT": bash_sdk_root,
                    "RUNNER_OS": "Windows" if os.name == "nt" else "Linux",
                    "CAPTURED_ARGS": bash_compatible_path(capture),
                    "FLUTTER_CAPTURED_ARGS": bash_compatible_path(flutter_capture),
                    "BOOTSTRAP_STEPS": bash_compatible_path(bootstrap_steps),
                }
            )
            if os.name == "nt":
                self.assertIsNotNone(
                    bootstrap_bash_command(),
                    "Git Bash was not found beside the Git for Windows installation",
                )
                self.assertNotEqual(
                    bootstrap_bash_command(),
                    "bash",
                    "Windows must invoke the selected Git Bash executable",
                )
            result = subprocess.run(
                [bootstrap_bash_command(), bash_compatible_path(script)],
                cwd=temp_root,
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )

            self.assertEqual(
                result.returncode,
                0,
                f"stdout={result.stdout!r}; stderr={result.stderr!r}",
            )
            captured_args = capture.read_text(encoding="utf-8").splitlines()
            if os.name == "nt":
                captured_args[3] = PureWindowsPath(captured_args[3]).as_posix()
            self.assertEqual(captured_args, expected_args)
            self.assertEqual(
                flutter_capture.read_text(encoding="utf-8").splitlines(), ["--version"]
            )
            self.assertEqual(
                bootstrap_steps.read_text(encoding="utf-8").splitlines(),
                ["dart", "flutter"],
            )

            windows_sdk_root = "C:/ci/fake Flutter SDK"
            resolve_windows_dart = (
                'cygpath() { [ "$1" = -u ] && [ "$2" = "C:/ci/fake Flutter SDK" ] || return 2; '
                'printf "%s\\n" "/c/ci/fake Flutter SDK"; }; '
                'source scripts/bootstrap_flutter_sdk.sh; '
                'sdk_root="$(normalize_flutter_sdk_root "$1" "$2")"; '
                'resolve_flutter_sdk_dart_bin "$sdk_root" "$2"'
            )
            resolved = subprocess.run(
                [
                    bootstrap_bash_command(),
                    "-c",
                    resolve_windows_dart,
                    "bootstrap-test",
                    windows_sdk_root,
                    "Windows",
                ],
                cwd=script.parents[1],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            expected_windows_bin = (
                "/c/ci/fake Flutter SDK/bin/cache/dart-sdk/bin/dart.exe"
            )
            self.assertEqual(resolved.returncode, 0, resolved.stderr)
            self.assertEqual(resolved.stdout.strip(), expected_windows_bin)

    def test_flutter_sdk_cache_uses_the_explicit_job_local_path(self):
        cache = matrix_module.flutter_sdk_pub_cache_directory(
            {"PUB_CACHE": "/runner/_temp/flutter-pub-cache", "HOME": "/home/runner"}
        )

        self.assertEqual(cache, os.path.abspath("/runner/_temp/flutter-pub-cache"))

    def test_flutter_sdk_cache_resolves_relative_override_from_the_host(self):
        cache = matrix_module.flutter_sdk_pub_cache_directory(
            {"PUB_CACHE": "relative-flutter-pub-cache"}
        )

        self.assertTrue(os.path.isabs(cache))
        self.assertEqual(os.path.basename(cache), "relative-flutter-pub-cache")

    def test_flutter_sdk_cache_uses_the_windows_default_when_unconfigured(self):
        cache = matrix_module.flutter_sdk_pub_cache_directory(
            {"LOCALAPPDATA": "C:/Users/runner/AppData/Local"}
        )

        self.assertEqual(cache, "")

    def test_flutter_sdk_cache_uses_home_default_on_posix_when_unconfigured(self):
        cache = matrix_module.flutter_sdk_pub_cache_directory(
            {"HOME": "/home/runner"}
        )

        self.assertEqual(cache, "")

    def test_flutter_sdk_pub_cache_without_override_stays_below_job_home(self):
        cache = matrix_module.flutter_sdk_pub_cache_directory(
            {"FLUTTER_SDK_HOME": "/runner/flutter-sdk-home"}
        )

        self.assertEqual(
            cache,
            os.path.join(os.path.abspath("/runner/flutter-sdk-home"), ".pub-cache"),
        )

    def test_flutter_sdk_home_uses_only_explicit_job_local_override(self):
        home = matrix_module.flutter_sdk_home_directory(
            {"FLUTTER_SDK_HOME": "relative/flutter-sdk-home", "HOME": "/home/runner"}
        )

        self.assertTrue(os.path.isabs(home))
        self.assertEqual(os.path.basename(home), "flutter-sdk-home")

    def test_flutter_sdk_home_does_not_fall_back_to_the_host_home(self):
        home = matrix_module.flutter_sdk_home_directory({"HOME": "/home/runner"})

        self.assertEqual(home, "")

    def test_lane_environment_overrides_host_store_and_project_cache(self):
        with patch.dict(
            "os.environ",
            {
                "MAGICORE_STORE_ROOT": "/host/private/mgc-store",
                "MGC_CACHE_DIR": "/host/private/mgc-cache",
            },
        ):
            original_environment = os.environ.copy()
            env = lifecycle_environment(
                "/tmp/mgc-lane-sandbox", os.path.join("/tmp/mgc-lane-sandbox", "project")
            )
            self.assertEqual(os.environ, original_environment)

        self.assertEqual(
            env["MGC_CACHE_DIR"],
            os.path.join("/tmp/mgc-lane-sandbox", "project", ".magicore"),
        )
        self.assertEqual(env["HOME"], os.path.join("/tmp/mgc-lane-sandbox", ".home"))
        self.assertEqual(
            env["MAGICORE_STORE_ROOT"],
            os.path.join("/tmp/mgc-lane-sandbox", ".home", ".magicore", "store"),
        )
        self.assertEqual(
            env["CARGO_HOME"], os.path.join("/tmp/mgc-lane-sandbox", ".cargo-home")
        )
        self.assertEqual(
            env["PIP_CACHE_DIR"], os.path.join("/tmp/mgc-lane-sandbox", ".cache", "pip")
        )
        self.assertEqual(
            env["npm_config_cache"], os.path.join("/tmp/mgc-lane-sandbox", ".cache", "npm")
        )
        self.assertEqual(env["GOMODCACHE"], os.path.join("/tmp/mgc-lane-sandbox", ".go", "pkg", "mod"))
        self.assertEqual(env["GOCACHE"], os.path.join("/tmp/mgc-lane-sandbox", ".cache", "go-build"))
        self.assertEqual(
            env["DOTNET_CLI_HOME"], os.path.join("/tmp/mgc-lane-sandbox", ".home", ".dotnet")
        )
        self.assertEqual(
            env["TMPDIR"], os.path.join("/tmp/mgc-lane-sandbox", ".tmp")
        )
        self.assertEqual(
            env["DENO_DIR"], os.path.join("/tmp/mgc-lane-sandbox", ".cache", "deno")
        )

    def test_pulumi_lane_uses_ephemeral_local_backend_and_drops_cloud_credentials(self):
        with tempfile.TemporaryDirectory() as root:
            env = pulumi_local_environment(
                root,
                {
                    "HOME": os.path.join(root, ".home"),
                    "PULUMI_ACCESS_TOKEN": "host-token",
                    "AWS_ACCESS_KEY_ID": "host-access-key",
                    "AWS_SECRET_ACCESS_KEY": "host-secret-key",
                    "AWS_PROFILE": "host-profile",
                    "PATH": "/usr/bin",
                },
            )

        self.assertEqual(env["PULUMI_BACKEND_URL"], Path(root, ".pulumi-backend").as_uri())
        self.assertEqual(env["PULUMI_HOME"], os.path.join(root, ".pulumi-home"))
        self.assertTrue(env["PULUMI_CONFIG_PASSPHRASE"])
        self.assertEqual(env["AWS_EC2_METADATA_DISABLED"], "true")
        for name in ("PULUMI_ACCESS_TOKEN", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_PROFILE"):
            self.assertNotIn(name, env)

    def test_pulumi_lane_declares_a_local_stack_setup_and_toolchain_probe(self):
        lane = next(
            lane for lane in LANES
            if lane["core"] == "clo" and lane["language"] == "pulumi"
        )
        self.assertEqual(lane["toolchain_probes"], ["pulumi"])
        self.assertIn("initialize_local_pulumi_stack", lane["pre_steps"])

        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "lifecycle-matrix.yml"
        ).read_text(encoding="utf-8")
        self.assertIn(
            "pulumi/actions@8e5e406f4007fca908480587cb9893c07090f58d",
            workflow,
        )
        self.assertIn('pulumi-version: "3.267.0"', workflow)

    def test_lane_environment_preserves_read_only_rustup_toolchain_home(self):
        with tempfile.TemporaryDirectory() as root:
            rustup_home = os.path.join(root, "rustup")
            os.makedirs(rustup_home)
            with patch.dict("os.environ", {"RUSTUP_HOME": rustup_home}):
                env = lifecycle_environment(
                    os.path.join(root, "lane"), os.path.join(root, "project")
                )

            self.assertEqual(env["RUSTUP_HOME"], rustup_home)
            self.assertEqual(env["RUSTUP_AUTO_INSTALL"], "0")

    def test_lane_environment_infers_rustup_home_from_host_home(self):
        with tempfile.TemporaryDirectory() as root:
            host_home = os.path.join(root, "host-home")
            rustup_home = os.path.join(host_home, ".rustup")
            os.makedirs(rustup_home)
            with patch.dict("os.environ", {"HOME": host_home}, clear=False):
                os.environ.pop("RUSTUP_HOME", None)
                env = lifecycle_environment(
                    os.path.join(root, "lane"), os.path.join(root, "project")
                )

            self.assertEqual(env["RUSTUP_HOME"], rustup_home)

    def test_lane_environment_rejects_missing_rustup_home_without_installing(self):
        with tempfile.TemporaryDirectory() as root:
            host_home = os.path.join(root, "host-home")
            os.makedirs(host_home)
            with patch.dict(
                "os.environ",
                {"HOME": host_home, "RUSTUP_HOME": os.path.join(root, "missing")},
                clear=False,
            ):
                env = lifecycle_environment(
                    os.path.join(root, "lane"), os.path.join(root, "project")
                )

            self.assertNotIn("RUSTUP_HOME", env)
            self.assertEqual(env["RUSTUP_AUTO_INSTALL"], "0")

    def test_lane_environment_creates_isolated_tool_temp_directory(self):
        with tempfile.TemporaryDirectory() as root:
            sandbox = os.path.join(root, "lane")
            os.makedirs(sandbox)
            env = lifecycle_environment(sandbox, os.path.join(sandbox, "project"))

            self.assertTrue(os.path.isdir(env["TMPDIR"]))
            self.assertEqual(env["TMP"], env["TMPDIR"])
            self.assertEqual(env["TEMP"], env["TMPDIR"])


    def test_recovery_environment_uses_project_private_store(self):
        with patch.dict(
            "os.environ",
            {
                "MAGICORE_STORE_ROOT": "/host/private/store",
                "MGC_CACHE_DIR": "/host/private/cache",
            },
        ):
            original_environment = os.environ.copy()
            env = recovery_environment("/tmp/lane", "/tmp/lane/project")
            self.assertEqual(os.environ, original_environment)

        self.assertEqual(
            env["HOME"], os.path.join('/tmp/lane/project', '.magicore-recovery', 'home')
        )
        self.assertEqual(
            env["MGC_CACHE_DIR"], os.path.join('/tmp/lane/project', '.magicore-recovery')
        )

    def test_materialization_markers_resolve_inside_lane_environment(self):
        rust_env = {"HOME": "/tmp/lane/.home"}
        self.assertEqual(
            _materialize_marker("/tmp/lane/project", "rust", rust_env),
            (
                "mgc-cargo-store",
                os.path.join('/tmp/lane/.home', '.magicore', 'store', 'cargo'),
            ),
        )

        go_env = {"PATH": "/lane/bin", "GOMODCACHE": "/tmp/lane/.go/pkg/mod"}
        completed = subprocess.CompletedProcess(
            args=["go", "env", "GOMODCACHE"], returncode=0, stdout=go_env["GOMODCACHE"]
        )
        with patch("lifecycle_capability_matrix.shutil.which", return_value="/lane/bin/go"), \
             patch("lifecycle_capability_matrix.subprocess.run", return_value=completed) as run:
            marker = _materialize_marker("/tmp/lane/project", "go", go_env)
        self.assertEqual(marker, ("gomodcache", go_env["GOMODCACHE"]))
        self.assertEqual(run.call_args.kwargs["env"], go_env)

    def test_python_provision_only_installs_into_lane_venv(self):
        sandbox = "/tmp/lane"
        project = "/tmp/lane/project"
        environment = {"PATH": "/usr/bin:/bin"}
        success = subprocess.CompletedProcess(args=[], returncode=0)
        calls = []

        def fake_run(argv, **kwargs):
            calls.append((argv, kwargs))
            return success

        with patch("lifecycle_capability_matrix.shutil.which", return_value="/usr/bin/python3"), \
             patch("lifecycle_capability_matrix.subprocess.run", side_effect=fake_run):
            result, isolated = provision_python_build_tools(
                sandbox, project, environment, timeout_s=30
            )

        self.assertIs(result, success)
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0][0][:3], ["/usr/bin/python3", "-m", "venv"])
        venv_python = os.path.join(
            sandbox,
            ".mgc-lifecycle-python",
            "Scripts" if os.name == "nt" else "bin",
            "python.exe" if os.name == "nt" else "python",
        )
        self.assertEqual(calls[1][0][0], venv_python)
        self.assertEqual(calls[1][1]["env"]["PIP_REQUIRE_VIRTUALENV"], "true")
        self.assertEqual(isolated["VIRTUAL_ENV"], os.path.dirname(os.path.dirname(venv_python)))
        self.assertEqual(environment, {"PATH": "/usr/bin:/bin"})

    def test_python_tools_use_isolated_venv_path_and_require_venv(self):
        original = {
            "PATH": "/usr/bin:/bin",
            "MAGICORE_STORE_ROOT": "/tmp/lane-store",
            "MGC_CACHE_DIR": "/tmp/project/.magicore",
        }
        active = python_venv_environment(original, "/tmp/lane/.venv")

        scripts_dir = "Scripts" if os.name == "nt" else "bin"
        self.assertTrue(
            active["PATH"].startswith(
                os.path.join("/tmp/lane/.venv", scripts_dir) + os.pathsep
            )
        )
        self.assertEqual(active["VIRTUAL_ENV"], "/tmp/lane/.venv")
        self.assertEqual(active["PIP_REQUIRE_VIRTUALENV"], "true")
        self.assertNotIn("VIRTUAL_ENV", original)
        self.assertEqual(original["PATH"], "/usr/bin:/bin")


class LanePreparationFailure(unittest.TestCase):
    def test_failed_real_dependency_fixture_cannot_be_laundered_by_empty_install(self):
        calls = []

        def fake_run(argv, **kwargs):
            args = argv[1:]
            calls.append(args)
            if args[0] == "create-test":
                os.mkdir(os.path.join(kwargs["cwd"], args[-1]))
                return subprocess.CompletedProcess(argv, 0, "created", "")
            if args[0] == "add-ai":
                return subprocess.CompletedProcess(argv, 1, "", "registry unavailable")
            if args[0] == "optimizer":
                return subprocess.CompletedProcess(argv, 0, "Optimizer skipped", "")
            raise AssertionError(f"blocked lifecycle step was executed: {args}")

        lane = {
            "core": "ai",
            "language": "matrix-test-language",
            "scaffold": ["create-test", "fixture"],
            "pre_steps": ["mgc_add_real_dependency"],
            "steps": [
                ("install", ["install"]),
                ("test", ["test"]),
                ("build", ["build"]),
            ],
            "delegated": [],
            "install_owner": "native-engine",
        }

        with patch("lifecycle_capability_matrix.subprocess.run", side_effect=fake_run):
            result = run_lane("/tmp/fake-mgc", lane)

        self.assertEqual(result["dims"]["add"], STATUS_FAILED)
        self.assertEqual(result["dims"]["install"], STATUS_UNVERIFIED)
        self.assertEqual(result["dims"]["test"], STATUS_UNVERIFIED)
        self.assertEqual(result["dims"]["build"], STATUS_UNVERIFIED)
        self.assertNotIn(["install"], calls)

    def test_failed_python_tool_provisioning_blocks_test_and_build(self):
        calls = []

        def fake_run(argv, **kwargs):
            args = argv[1:]
            calls.append(args)
            if args[0] == "create-test":
                os.mkdir(os.path.join(kwargs["cwd"], args[-1]))
                return subprocess.CompletedProcess(argv, 0, "created", "")
            if args[0] == "install":
                return subprocess.CompletedProcess(argv, 0, "installed", "")
            if args[0] == "optimizer":
                return subprocess.CompletedProcess(argv, 0, "Optimizer skipped", "")
            raise AssertionError(f"step without provisioned tools was run: {args}")

        provision_failure = subprocess.CompletedProcess(
            args=[], returncode=1, stdout="", stderr="build backend unavailable"
        )
        lane = {
            "core": "lib",
            "language": "matrix-test-language",
            "scaffold": ["create-test", "fixture"],
            "pre_steps": ["provision_py_build_tools"],
            "steps": [
                ("install", ["install"]),
                ("test", ["test"]),
                ("build", ["build"]),
            ],
            "delegated": [],
            "install_owner": "native-engine",
        }

        with patch("lifecycle_capability_matrix.subprocess.run", side_effect=fake_run), \
             patch(
                 "lifecycle_capability_matrix.provision_python_build_tools",
                 return_value=(provision_failure, {"PATH": "/lane/bin"}),
             ):
            result = run_lane("/tmp/fake-mgc", lane)

        self.assertEqual(result["dims"]["install"], STATUS_NATIVE)
        self.assertEqual(result["dims"]["test"], STATUS_UNVERIFIED)
        self.assertEqual(result["dims"]["build"], STATUS_UNVERIFIED)
        self.assertIn(["install"], calls)
        self.assertNotIn(["test"], calls)
        self.assertNotIn(["build"], calls)

    def test_failed_local_pulumi_stack_setup_blocks_preview_without_cloud_auth(self):
        calls = []

        def fake_run(argv, **kwargs):
            args = argv[1:] if argv[0] == "/tmp/fake-mgc" else argv
            calls.append((args, kwargs.get("env", {})))
            if args[0] == "create-test":
                project = os.path.join(kwargs["cwd"], args[-1])
                os.mkdir(project)
                Path(project, "package.json").write_text("{}", encoding="utf-8")
                Path(project, "Pulumi.yaml").write_text(
                    "name: fixture\nruntime: nodejs\n", encoding="utf-8"
                )
                return subprocess.CompletedProcess(argv, 0, "created", "")
            if args[:2] == ["pulumi", "login"]:
                return subprocess.CompletedProcess(argv, 0, "local login", "")
            if args[:3] == ["pulumi", "stack", "init"]:
                return subprocess.CompletedProcess(argv, 1, "", "stack setup failed")
            if args[0] == "install":
                return subprocess.CompletedProcess(argv, 0, "installed", "")
            if args[0] == "test":
                return subprocess.CompletedProcess(argv, 0, "tested", "")
            if args[0] == "optimizer":
                return subprocess.CompletedProcess(argv, 0, "Optimizer skipped", "")
            raise AssertionError(f"Pulumi preview ran without a local stack: {args}")

        lane = {
            "core": "clo",
            "language": "pulumi",
            "scaffold": ["create-test", "fixture"],
            "pre_steps": ["initialize_local_pulumi_stack"],
            "steps": [("install", ["install"]), ("test", ["test"]), ("build", ["build"])],
            "delegated": [],
            "install_owner": "native-engine",
        }

        with patch("lifecycle_capability_matrix.subprocess.run", side_effect=fake_run):
            result = run_lane("/tmp/fake-mgc", lane)

        self.assertEqual(result["dims"]["install"], STATUS_NATIVE, result)
        self.assertEqual(result["dims"]["test"], STATUS_PLAIN, result)
        self.assertEqual(result["dims"]["build"], STATUS_UNVERIFIED)
        pulumi_calls = [call for call in calls if call[0][0] == "pulumi"]
        self.assertEqual([call[0][1] for call in pulumi_calls], ["login", "stack"])
        pulumi_env = pulumi_calls[0][1]
        self.assertTrue(pulumi_env["PULUMI_BACKEND_URL"].startswith("file://"))
        self.assertNotIn("PULUMI_ACCESS_TOKEN", pulumi_env)
        self.assertFalse(any(name.startswith("AWS_") and name != "AWS_EC2_METADATA_DISABLED" for name in pulumi_env))
        self.assertNotIn((['build'], pulumi_env), calls)


def evidence(dimensions=None, owners=None):
    statuses = {name: STATUS_NATIVE for name in NATIVE_PM_REQUIRED_DIMENSIONS}
    statuses.update(dimensions or {})
    operation_owners = {
        "resolve": "mgc",
        "lock": "mgc",
        "fetch": "mgc",
        "verify": "mgc",
        "store": "magicore-shared-cas",
        "materialize": "mgc",
    }
    operation_owners.update({operation: "mgc" for operation in NATIVE_PM_USER_OPERATIONS})
    operation_owners.update(owners or {})
    return statuses, operation_owners


class NativePackageManagerVerdict(unittest.TestCase):
    def test_release_gate_scope_tracks_every_declared_native_lane(self):
        expected = {
            (lane["core"], lane["language"], lane.get("framework_id", ""))
            for lane in LANES
            if lane.get("dependency_owner") == "mgc-native"
        }
        self.assertEqual(native_pm_claim_scope(LANES), expected)

    def test_missing_native_evidence_blocks_each_claimed_lane(self):
        lanes = [
            {
                "core": "app",
                "language": "flutter",
                "dependency_owner": "mgc-native",
                "native_pm_verdict": "not-native-pm",
            },
            {
                "core": "app",
                "language": "objc",
                "dependency_owner": "unsupported",
                "native_pm_verdict": "unsupported",
            },
        ]
        failures = native_pm_claim_errors(lanes)
        self.assertEqual(len(failures), 1)
        self.assertIn("app/flutter", failures[0])

    def test_native_pm_claim_with_missing_evidence_shape_fails_closed(self):
        failures = native_pm_lane_errors({
            "core": "web",
            "language": "javascript",
            "dependency_owner": "mgc-native",
            "install_owner": "native-engine",
            "native_pm_verdict": "not-native-pm",
        })
        self.assertTrue(any("malformed native-PM dimensions" in failure for failure in failures))

    def test_matrix_cannot_downgrade_source_native_owner(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "app" and lane["language"] == "swift"
        )
        matrix_lane = dict(source_lane)
        matrix_lane["dependency_owner"] = "delegated"
        matrix_lane["install_owner"] = "plain-delegation"
        errors = lifecycle_lane_owner_contract_errors(matrix_lane)
        self.assertTrue(any("dependency owner differs from source contract" in error for error in errors))
        self.assertTrue(any("install owner differs from source contract" in error for error in errors))

    def test_matrix_cannot_downgrade_a_source_operation_owner(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "lib" and lane["language"] == "rust"
        )
        matrix_lane = dict(source_lane)
        matrix_lane["owner_by_operation"] = dict(source_lane["owner_by_operation"])
        matrix_lane["owner_by_operation"]["install"] = "cargo"
        errors = lifecycle_lane_owner_contract_errors(matrix_lane)
        self.assertTrue(any("operation owner differs from source contract" in error for error in errors))

    def test_partial_native_ownership_is_not_mislabeled_as_full_native_pm(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "lib" and lane["language"] == "rust"
        )
        dimensions, _ = evidence()
        dimensions["list"] = STATUS_UNVERIFIED
        self.assertEqual(
            native_pm_lane_errors({
                "core": "lib",
                "language": "rust",
                "verdict": "orchestration-lifecycle-passed",
                "dependency_owner": "mgc-native",
                "install_owner": "native-engine",
                "native_pm_verdict": "not-native-pm",
                "native_pm_delegated": [],
                "dimensions": dimensions,
                "owner_by_operation": dict(source_lane["owner_by_operation"]),
            }),
            [],
        )

    def test_native_claim_errors_recheck_runtime_evidence(self):
        dimensions, operation_owners = evidence()
        operation_owners["fetch"] = "cargo"
        failures = native_pm_claim_errors([{
            "core": "lib",
            "language": "rust",
            "verdict": "orchestration-lifecycle-passed",
            "dependency_owner": "mgc-native",
            "install_owner": "native-engine",
            "native_pm_verdict": "native-pm-supported",
            "native_pm_delegated": [],
            "dimensions": dimensions,
            "owner_by_operation": operation_owners,
        }])
        self.assertTrue(
            any(
                "without complete package-manager evidence" in failure
                for failure in failures
            )
        )

    def test_native_pm_lane_gate_rechecks_dimensions_and_operation_owners(self):
        dimensions, operation_owners = evidence()
        lane = {
            "core": "lib",
            "language": "rust",
            "verdict": "orchestration-lifecycle-passed",
            "dependency_owner": "mgc-native",
            "install_owner": "native-engine",
            "native_pm_verdict": "native-pm-supported",
            "native_pm_delegated": [],
            "dimensions": dimensions,
            "owner_by_operation": operation_owners,
            "release_scope_out_reason": "Lifecycle qualification is evidence-only.",
        }
        self.assertEqual(native_pm_lane_errors(lane), [])
        operation_owners["fetch"] = "cargo"
        failures = native_pm_lane_errors(lane)
        self.assertTrue(
            any(
                "without complete package-manager evidence" in failure
                for failure in failures
            )
        )

    def test_explicit_non_native_owners_do_not_get_misclassified_as_native_claims(self):
        for dependency_owner, install_owner, verdict in (
            ("delegated", "plain-delegation", "compatibility-passed"),
            ("unsupported", "unsupported", "unsupported"),
            ("scaffold-only", "unsupported", "unsupported"),
        ):
            with self.subTest(dependency_owner=dependency_owner):
                self.assertEqual(
                    native_pm_lane_errors({
                        "core": "app",
                        "language": "swift",
                        "verdict": "orchestration-lifecycle-passed",
                        "dependency_owner": dependency_owner,
                        "install_owner": install_owner,
                        "native_pm_verdict": verdict,
                        "release_scope_out_reason": (
                            "Current release contract excludes this unsupported lane."
                            if dependency_owner in {"unsupported", "scaffold-only"}
                            else None
                        ),
                    }),
                    [],
                )

    def test_unsupported_owner_without_scopeout_reason_fails_closed(self):
        failures = native_pm_lane_errors({
            "core": "app",
            "language": "objc",
            "dependency_owner": "unsupported",
            "install_owner": "unsupported",
            "native_pm_verdict": "unsupported",
        })
        self.assertTrue(any("missing an explicit release scope-out reason" in f for f in failures))

    def test_scopeout_with_unknown_lifecycle_verdict_fails_closed(self):
        failures = native_pm_lane_errors({
            "core": "app",
            "language": "objc",
            "verdict": "made-up-verdict",
            "dependency_owner": "unsupported",
            "install_owner": "unsupported",
            "native_pm_verdict": "unsupported",
            "release_scope_out_reason": "Explicitly outside this release scope.",
        })
        self.assertTrue(any("invalid lifecycle verdict" in failure for failure in failures))

    def test_unknown_dependency_owner_fails_closed(self):
        failures = native_pm_lane_errors({
            "core": "app",
            "language": "swift",
            "dependency_owner": "maybe-native",
            "install_owner": "native-engine",
            "native_pm_verdict": "native-pm-supported",
        })
        self.assertTrue(any("invalid dependency_owner" in failure for failure in failures))

    def test_release_scopeouts_are_named_and_do_not_remove_lanes_from_inventory(self):
        expected = {
            ("lib", "java", "java"),
            ("lib", "dotnet", "dotnet"),
            ("app", "swift", ""),
            ("app", "objc", ""),
            ("app", "react-native", ""),
            ("clo", "terraform", ""),
            ("game", "rust", ""),
            ("hardware", "benchmark", ""),
            ("iot", "rust", ""),
            ("cicd", "github-actions", ""),
        }
        self.assertTrue(expected.issubset(PLATFORM_EVIDENCE_RELEASE_SCOPE))
        self.assertTrue(all(release_scope_out_reason(*key) for key in expected))
        evidence_only = {
            (lane["core"], lane["language"], lane.get("framework_id", ""))
            for lane in LANES
            if lane.get("evidence_only")
        }
        self.assertEqual(expected, evidence_only)
        self.assertEqual(
            expected & native_pm_claim_scope(LANES),
            {
                ("lib", "java", "java"),
                ("lib", "dotnet", "dotnet"),
                ("app", "swift", ""),
                ("game", "rust", ""),
                ("iot", "rust", ""),
            },
        )
        self.assertIsNone(release_scope_out_reason("web", "javascript"))

    def test_dotnet_lifecycle_is_scoped_out_until_native_build_state_exists(self):
        lane = next(
            lane for lane in LANES
            if (lane["core"], lane["language"], lane["framework_id"])
            == ("lib", "dotnet", "dotnet")
        )
        reason = release_scope_out_reason("lib", "dotnet", "dotnet")

        self.assertTrue(lane["evidence_only"])
        self.assertEqual([step for step, _ in lane["steps"]], ["install"])
        self.assertEqual(
            lane["lifecycle_owner_overrides"],
            {"test": "unsupported", "build": "unsupported"},
        )
        self.assertIn("MSBuild restore state", reason)
        self.assertIn("00-index §5.1", reason)

    def test_lifecycle_workflow_uses_shared_native_claim_gate(self):
        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "lifecycle-matrix.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("NATIVE_PM_SCOPE = native_pm_claim_scope(data[\"lanes\"])", workflow)
        self.assertIn("failures.extend(native_pm_claim_errors(data[\"lanes\"]))", workflow)
        self.assertIn("failures.extend(native_pm_lane_errors(lane))", workflow)
        self.assertIn("lifecycle_lane_owner_contract_errors(lane)", workflow)
        self.assertIn("release_scope_out_reason(*key)", workflow)
        self.assertIn("PLATFORM_EVIDENCE_RELEASE_SCOPE", workflow)
        self.assertIn('if lane.get("dependency_owner") != "mgc-native":', workflow)
        self.assertIn('lane.get("native_pm_verdict") != "native-pm-supported"', workflow)
        self.assertIn("unexpected lane outside global v1.2 scope", workflow)
        self.assertIn("OK: release-blocking lifecycle lanes passed:", workflow)
        self.assertIn(
            "Lifecycle-scoped-out lanes (native-PM claims remain gated):", workflow
        )
        self.assertNotIn(
            "OK: global core/language lanes orchestration-lifecycle-passed", workflow
        )
        self.assertIn("from lifecycle_capability_matrix import (\n              ALL_STATUSES,\n              ALL_DIMENSIONS,\n              SCHEMA_VERSION,", workflow)
        self.assertIn('data.get("schema_version") != SCHEMA_VERSION', workflow)
        self.assertIn("LIFECYCLE_OWNER_VALUES,", workflow)
        self.assertIn('lifecycle_owners = lane.get("lifecycle_owners")', workflow)
        self.assertNotIn('owner != "mgc-native"', workflow)
        self.assertIn("lifecycle_status_owner_matches(", workflow)
        self.assertIn("Fully native-PM qualified lanes:", workflow)
        self.assertIn("Native-PM claim withheld pending complete runtime evidence:", workflow)
        self.assertIn("framework_catalog_errors(data.get(\"framework_catalog\"), data[\"lanes\"])", workflow)
        self.assertIn("required_dimensions_for_lane(*key)", workflow)
        self.assertIn("if status not in ALL_STATUSES", workflow)
        self.assertIn("for name in (source_required or ())", workflow)
        self.assertIn("if dimensions.get(name) not in PASS_STATUSES", workflow)
        self.assertIn("unknown_dimensions = sorted(set(dimensions) - set(ALL_DIMENSIONS))", workflow)
        self.assertGreaterEqual(workflow.count('- "scripts/**"'), 2)
        self.assertNotIn('RELEASE_SCOPE = {\n              ("web", "javascript")', workflow)

    def test_lifecycle_workflow_loads_dimensions_before_owner_consistency_check(self):
        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "lifecycle-matrix.yml"
        ).read_text(encoding="utf-8")
        dimensions_assignment = workflow.index(
            'dimensions = lane.get("dimensions", {})'
        )
        owner_status_check = workflow.index("lifecycle_status_owner_matches(")
        self.assertLess(dimensions_assignment, owner_status_check)
        self.assertIn('if not isinstance(dimensions, dict):', workflow)

    def test_lifecycle_workflow_enforces_matrix_on_every_operating_system(self):
        workflow = (
            Path(__file__).resolve().parent.parent
            / ".github"
            / "workflows"
            / "lifecycle-matrix.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("os: [ubuntu-latest, macos-latest, windows-latest]", workflow)
        self.assertIn("shell: python {0}", workflow)
        self.assertNotIn("shell: ${{ matrix.", workflow)
        self.assertNotIn("if: matrix.os != 'windows-latest'", workflow)
        self.assertNotIn("windows-evidence-promotion:", workflow)

    def test_native_verdict_requires_every_user_dependency_operation(self):
        expected = {
            "install", "add", "remove", "update", "list",
            "frozen-install", "offline-reinstall", "gc",
        }
        self.assertEqual(set(NATIVE_PM_USER_OPERATIONS), expected)
        self.assertTrue(expected.issubset(set(NATIVE_PM_REQUIRED_DIMENSIONS)))
        for operation in expected:
            with self.subTest(operation=operation):
                dimensions, owners = evidence({operation: STATUS_UNSUPPORTED})
                self.assertFalse(
                    native_pm_supported("mgc-native", "native-engine", [], dimensions, owners)
                )

    def test_missing_owner_for_a_native_user_operation_blocks_claim(self):
        dimensions, owners = evidence()
        del owners["list"]
        self.assertFalse(
            native_pm_supported("mgc-native", "native-engine", [], dimensions, owners)
        )

    def test_owner_summary_never_presents_declared_native_owner_as_a_verdict(self):
        output = io.StringIO()
        with redirect_stdout(output):
            print_dependency_owner_summary()
        self.assertNotIn("native-pm-supported (ceiling)", output.getvalue())
        self.assertIn("native evidence required; not a verdict", output.getvalue())
        self.assertIn(
            '"native_pm_verdict": "not-evaluated-without-lifecycle-evidence"',
            output.getvalue(),
        )

    def test_integrity_verification_is_a_first_class_matrix_dimension(self):
        self.assertIn("verify", ALL_DIMENSIONS)
        self.assertTrue(set(NATIVE_PM_USER_OPERATIONS).issubset(ALL_DIMENSIONS))
        self.assertEqual(len(ALL_DIMENSIONS), 23)
        for lane in LANES:
            with self.subTest(core=lane["core"], language=lane["language"]):
                self.assertTrue(lane.get("owner_by_operation", {}).get("verify"))

    def test_lifecycle_only_pass_does_not_claim_native_package_manager(self):
        dimensions, owners = evidence(
            {"store": STATUS_UNSUPPORTED, "offline-reinstall": STATUS_UNSUPPORTED}
        )
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_missing_dependency_dimension_does_not_claim_native(self):
        dimensions, owners = evidence()
        del dimensions["fetch"]
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_missing_integrity_verification_evidence_does_not_claim_native(self):
        dimensions, owners = evidence({"verify": STATUS_UNSUPPORTED})
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_delegated_integrity_verification_does_not_claim_native(self):
        dimensions, owners = evidence(owners={"verify": "cargo"})
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_delegated_operation_does_not_claim_native(self):
        dimensions, owners = evidence(owners={"fetch": "cargo"})
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_any_delegated_package_manager_blocks_native_claim(self):
        dimensions, owners = evidence()
        self.assertFalse(native_pm_supported("mgc-native", "native-engine", ["cargo"], dimensions, owners))

    def test_complete_native_dependency_evidence_passes(self):
        dimensions, owners = evidence()
        self.assertTrue(native_pm_supported("mgc-native", "native-engine", [], dimensions, owners))

    def test_missing_toolchain_preserves_explicit_unsupported_native_pm_verdict(self):
        self.assertEqual(native_pm_unavailable_verdict("unsupported"), "unsupported")
        self.assertEqual(native_pm_unavailable_verdict("scaffold-only"), "unsupported")
        self.assertEqual(native_pm_unavailable_verdict("mgc-native"), "not-native-pm")
        self.assertEqual(native_pm_unavailable_verdict("delegated"), "not-native-pm")

    def test_failed_probe_is_not_rewritten_as_an_owner_status_mismatch(self):
        self.assertTrue(lifecycle_status_owner_matches(STATUS_FAILED, "plain-delegation"))


class NativeFrameworkLifecycleCoverage(unittest.TestCase):
    def test_framework_qualified_release_scope_uses_new_schema(self):
        self.assertEqual(SCHEMA_VERSION, 7)

    def catalog_record(self, framework, *, core="lib", status="mgc-engine-path", operations=None):
        operation_owners = operations or {
            operation: {"owner": "mgc-native"}
            for operation in ALL_DEPENDENCY_OPERATIONS
        }
        return {
            "core": core,
            "framework": framework,
            "status": status,
            "dependency_ownership": operation_owners,
            "evidence": "test fixture",
        }

    def test_catalog_native_framework_with_complete_lane_and_owners_passes(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "lib" and lane["language"] == "rust"
        )
        lanes = [{
            "core": "lib",
            "framework_id": "rust",
            "owner_by_operation": source_lane["owner_by_operation"],
        }]
        self.assertEqual(
            framework_catalog_errors(
                [self.catalog_record("rust", operations=self.catalog_owners(source_lane))], lanes
            ),
            [],
        )

    def catalog_owners(self, lane):
        owners = {}
        for operation, matrix_owner in lane["owner_by_operation"].items():
            owner = (
                "mgc-native"
                if matrix_owner in {"mgc", "magicore-shared-cas"}
                else "scaffold-only"
                if matrix_owner == "scaffold-only"
                else "unsupported"
            )
            owners[operation] = {"owner": owner}
        return owners

    def test_native_framework_can_explicitly_leave_an_operation_unsupported(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "lib" and lane["language"] == "rust"
        )
        owners = self.catalog_owners(source_lane)
        self.assertEqual(owners["gc"]["owner"], "unsupported")
        self.assertEqual(
            framework_catalog_errors(
                [self.catalog_record("rust", operations=owners)],
                [{
                    "core": "lib",
                    "framework_id": "rust",
                    "owner_by_operation": source_lane["owner_by_operation"],
                }],
            ),
            [],
        )

    def test_native_framework_operation_cannot_be_downgraded_from_lane_contract(self):
        source_lane = next(
            lane for lane in LANES
            if lane["core"] == "lib" and lane["language"] == "rust"
        )
        owners = self.catalog_owners(source_lane)
        owners["install"] = {"owner": "unsupported"}
        errors = framework_catalog_errors(
            [self.catalog_record("rust", operations=owners)],
            [{
                "core": "lib",
                "framework_id": "rust",
                "owner_by_operation": source_lane["owner_by_operation"],
            }],
        )
        self.assertTrue(any("operation install differs from lifecycle owner" in error for error in errors))

    def test_catalog_scaffold_only_framework_with_reason_is_an_explicit_scopeout(self):
        row = self.catalog_record("django", status="scaffold-only")
        row["evidence"] = "Framework is scaffolded; lifecycle qualification is not claimed."
        self.assertEqual(framework_catalog_errors([row], []), [])

    def test_catalog_scaffold_only_framework_without_reason_fails_closed(self):
        row = self.catalog_record("django", status="scaffold-only")
        row["evidence"] = "  "
        errors = framework_catalog_errors([row], [])
        self.assertTrue(any("scope-out reason" in error for error in errors))

    def test_catalog_framework_missing_dependency_owner_blocks_promotion(self):
        operations = {
            operation: {"owner": "mgc-native"}
            for operation in ALL_DEPENDENCY_OPERATIONS
        }
        del operations["offline-reinstall"]
        errors = framework_catalog_errors(
            [self.catalog_record("rust", operations=operations)],
            [{
                "core": "lib",
                "framework_id": "rust",
                "owner_by_operation": next(
                    lane["owner_by_operation"] for lane in LANES
                    if lane["core"] == "lib" and lane["language"] == "rust"
                ),
            }],
        )
        self.assertTrue(any("offline-reinstall" in error for error in errors))

    def test_catalog_rejects_duplicate_or_unregistered_framework_identity(self):
        lanes = [{"core": "lib", "framework_id": "rust"}]
        errors = framework_catalog_errors(
            [self.catalog_record("rust"), self.catalog_record("rust")],
            [{"core": "lib", "framework_id": "go"}],
        )
        self.assertTrue(any("duplicate framework catalog" in error for error in errors))
        self.assertTrue(any("not present in framework catalog" in error for error in errors))

    def test_matrix_producer_requires_compiled_framework_and_owner_crosscheck(self):
        source = (
            Path(__file__).resolve().parent
            / "lifecycle_capability_matrix.py"
        ).read_text(encoding="utf-8")
        self.assertIn("dep_gate_rc = validate_dep_gate_consistency(mgc_bin)", source)
        self.assertIn('"framework_catalog": FRAMEWORK_QUALIFICATION_EVIDENCE', source)
        self.assertIn("or dep_gate_rc != 0", source)

    def test_lane_registry_rejects_duplicate_full_lane_identity(self):
        duplicate = [dict(LANES[0]), dict(LANES[0])]
        with self.assertRaisesRegex(ValueError, "duplicate core/language/framework"):
            validate_lane_registry(duplicate)

    def test_lane_registry_allows_distinct_frameworks_for_same_core_language(self):
        first = dict(LANES[0])
        second = dict(LANES[0], framework_id="rust-macro", framework_ids=["rust-macro"])
        self.assertTrue(validate_lane_registry([first, second]))

    def test_lane_registry_rejects_unknown_required_dimension(self):
        invalid = [dict(LANES[0], required_dims=["not-a-dimension"])]
        with self.assertRaisesRegex(ValueError, "unknown required dimension"):
            validate_lane_registry(invalid)

    def test_lane_registry_rejects_invalid_lifecycle_owner_override(self):
        invalid = [dict(LANES[0], lifecycle_owner_overrides={"dev": "vite"})]
        with self.assertRaisesRegex(ValueError, "invalid lifecycle owner override"):
            validate_lane_registry(invalid)

    def test_lane_registry_rejects_unproven_native_build_test_run_overrides(self):
        for lane in LANES:
            for dimension in ("build", "test", "run"):
                invalid = dict(
                    lane,
                    lifecycle_owner_overrides={dimension: "mgc-native"},
                )
                with self.subTest(
                    core=lane["core"],
                    language=lane["language"],
                    framework=lane.get("framework_id", ""),
                    dimension=dimension,
                ):
                    with self.assertRaisesRegex(
                        ValueError, "native lifecycle owner override lacks evidence"
                    ):
                        validate_lane_registry([invalid])

    def test_lane_registry_rejects_framework_lane_mismatch(self):
        invalid = [dict(LANES[0], framework_id="not-the-declared-id")]
        with self.assertRaisesRegex(ValueError, "framework_id must match"):
            validate_lane_registry(invalid)

    def test_lane_registry_rejects_framework_ids_without_exact_binding(self):
        invalid = [dict(LANES[0], framework_id=None)]
        with self.assertRaisesRegex(ValueError, "requires an exact framework_id"):
            validate_lane_registry(invalid)

    def test_lane_registry_rejects_duplicate_framework_identity(self):
        first = dict(LANES[0])
        second = dict(LANES[1], framework_id=first["framework_id"],
                      framework_ids=list(first["framework_ids"]))
        with self.assertRaisesRegex(ValueError, "duplicate core/framework"):
            validate_lane_registry([first, second])

    def missing_native_lanes(self, source, lanes):
        # Match one Rust struct literal only; matching a bare `}` can span
        # adjacent records because each literal closes with `},`.
        # Khớp đúng một struct literal; bare `}` sẽ nuốt nhiều record liền nhau.
        records = re.findall(r"FrameworkRecord\s*\{(.*?)\n\s*\},", source, re.S)
        native_frameworks = set()
        for record in records:
            if "FrameworkStatus::NativeEngine" not in record:
                continue
            core = re.search(r'core:\s*"([^"]+)"', record)
            framework = re.search(r'framework:\s*"([^"]+)"', record)
            self.assertIsNotNone(core, f"native framework record has no core: {record}")
            self.assertIsNotNone(
                framework, f"native framework record has no id: {record}"
            )
            native_frameworks.add((core.group(1), framework.group(1)))

        covered = {
            (lane["core"], framework)
            for lane in lanes
            for framework in lane.get("framework_ids", ())
        }
        return native_frameworks - covered

    def test_every_native_engine_framework_has_a_lifecycle_lane(self):
        # Registry engine routes need executable lifecycle lanes, not only
        # per-operation declarations. Registry engine route phải có lane.
        root = Path(__file__).resolve().parents[1]
        source = (root / "cli/src/commands/framework_records.rs").read_text(
            encoding="utf-8"
        )
        missing = self.missing_native_lanes(source, LANES)
        self.assertEqual(
            missing,
            set(),
            "native engine framework(s) without lifecycle evidence lane: "
            f"{sorted(missing)}",
        )

    def test_negative_control_detects_removed_cloud_framework_lanes(self):
        root = Path(__file__).resolve().parents[1]
        source = (root / "cli/src/commands/framework_records.rs").read_text(
            encoding="utf-8"
        )
        without_cloud = [lane for lane in LANES if lane["core"] != "clo"]
        missing = self.missing_native_lanes(source, without_cloud)
        self.assertEqual(missing, {("clo", "cdk"), ("clo", "pulumi")})

    def test_framework_specific_lanes_bind_exactly_one_matching_id(self):
        for lane in LANES:
            framework_id = lane.get("framework_id")
            if framework_id:
                self.assertEqual(
                    lane.get("framework_ids"),
                    [framework_id],
                    f"{lane['core']}/{lane['language']} must bind its exact framework",
                )

    def test_typescript_scaffold_requires_source_and_config_markers(self):
        with tempfile.TemporaryDirectory() as sandbox:
            project = Path(sandbox) / "test-web-ts"
            project.mkdir()
            (project / "index.html").write_text("<main></main>", encoding="utf-8")
            self.assertFalse(
                scaffold_language_matches(
                    sandbox,
                    project.name,
                    "ts",
                    ["index.html", "src/main.ts", "tsconfig.json"],
                )
            )
            (project / "src").mkdir()
            (project / "src/main.ts").write_text("export {}", encoding="utf-8")
            (project / "tsconfig.json").write_text("{}", encoding="utf-8")
            self.assertTrue(
                scaffold_language_matches(
                    sandbox,
                    project.name,
                    "ts",
                    ["index.html", "src/main.ts", "tsconfig.json"],
                )
            )


class MatrixProvenance(unittest.TestCase):
    def test_run_sha_mismatch_is_rejected(self):
        self.assertIsNotNone(evidence_commit_error("1" * 40, "2" * 40))

    def test_matching_run_sha_is_accepted(self):
        self.assertIsNone(evidence_commit_error("a" * 40, "a" * 40))

    def test_local_run_without_expected_sha_remains_supported(self):
        self.assertIsNone(evidence_commit_error("a" * 40, None))

    def test_ci_run_without_expected_sha_is_rejected(self):
        self.assertIsNotNone(evidence_commit_error("a" * 40, None, is_ci=True))

    def test_ci_green_record_requires_sha_bound_to_run(self):
        self.assertIsNotNone(
            evidence_commit_error(
                "a" * 40,
                "a" * 40,
                is_ci=True,
                recorded_sha="b" * 40,
                require_recorded=True,
            )
        )


class PlatformGreenCounter(unittest.TestCase):
    def test_green_matrix_fixture_matches_source_owner_contracts(self):
        errors = platform_evidence_errors(
            self.green_matrix(), "a" * 40, current_tree_clean=True
        )
        self.assertEqual(errors, [])

    def test_lifecycle_output_has_a_runtime_platform_name(self):
        self.assertTrue(lifecycle_platform_name())

    def test_v12_platform_scope_covers_every_declared_core_language_lane(self):
        self.assertEqual(
            PLATFORM_EVIDENCE_RELEASE_SCOPE,
            frozenset(
                (lane["core"], lane["language"], lane.get("framework_id", ""))
                for lane in LANES
            ),
        )

    def green_matrix(self):
        def catalog_owners(lane):
            return {
                operation: {
                    "owner": (
                        "mgc-native"
                        if matrix_owner in {"mgc", "magicore-shared-cas"}
                        else "scaffold-only"
                        if matrix_owner == "scaffold-only"
                        else "unsupported"
                    )
                }
                for operation, matrix_owner in lane["owner_by_operation"].items()
            }

        def native_pm_verdict(lane):
            dimensions = {
                **{name: STATUS_NATIVE for name in ALL_DIMENSIONS},
                **{
                    name: lifecycle_status_for_owner(
                        lifecycle_owner_for(lane, name)
                    )
                    for name in lane["required_dims"]
                },
            }
            if lane["dependency_owner"] == "mgc-native":
                return (
                    "native-pm-supported"
                    if native_pm_supported(
                        lane["dependency_owner"],
                        lane["install_owner"],
                        [],
                        dimensions,
                        lane["owner_by_operation"],
                    )
                    else "not-native-pm"
                )
            if lane["dependency_owner"] == "delegated":
                return "compatibility-passed"
            return "unsupported"

        return {
            "schema_version": SCHEMA_VERSION,
            "matrix_kind": "lifecycle",
            "commit": "a" * 40,
            "platform": "Windows",
            "working_tree_clean": True,
            "lanes": [
                {
                    "core": lane["core"],
                    "language": lane["language"],
                    "framework_id": lane.get("framework_id", ""),
                    "verdict": "orchestration-lifecycle-passed",
                    "evidence_only": bool(lane.get("evidence_only")),
                    "toolchain_available": True,
                    "required_dimensions": lane["required_dims"],
                    "dimensions": {
                        **{name: STATUS_NATIVE for name in ALL_DIMENSIONS},
                        **{
                            name: lifecycle_status_for_owner(
                                lifecycle_owner_for(lane, name)
                            )
                            for name in lane["required_dims"]
                        },
                    },
                    "lifecycle_owners": {
                        name: lifecycle_owner_for(lane, name)
                        for name in lane["required_dims"]
                    },
                    "dependency_owner": lane["dependency_owner"],
                    "install_owner": lane["install_owner"],
                    "native_pm_delegated": [],
                    "native_pm_verdict": native_pm_verdict(lane),
                    "owner_by_operation": dict(lane["owner_by_operation"]),
                    "release_scope_out_reason": release_scope_out_reason(
                        lane["core"], lane["language"], lane.get("framework_id", "")
                    ),
                    "gate_role": (
                        "scoped-out"
                        if release_scope_out_reason(
                            lane["core"], lane["language"], lane.get("framework_id", "")
                        )
                        else "release-blocking"
                    ),
                }
                for lane in LANES
            ],
            "framework_catalog": [
                {
                    "core": lane["core"],
                    "framework": lane["framework_id"],
                    "status": (
                        "mgc-engine-path"
                        if lane["dependency_owner"] == "mgc-native"
                        else "scaffold-only"
                    ),
                    "dependency_ownership": catalog_owners(lane),
                    "evidence": "synthetic passing test fixture",
                }
                for lane in LANES
                if lane.get("framework_id")
            ],
        }

    def test_only_complete_release_scope_can_record_platform_green(self):
        # A complete matrix that preserves source ownership may pass the gate.
        self.assertEqual(
            platform_evidence_errors(
                self.green_matrix(), "a" * 40, current_tree_clean=True
            ),
            [],
        )

    def test_current_checkout_must_still_be_clean_when_recording_green(self):
        errors = platform_evidence_errors(
            self.green_matrix(), "a" * 40, current_tree_clean=False
        )
        self.assertTrue(any("current checkout" in error for error in errors))

    def test_platform_green_rejects_unproven_native_claim_outside_old_scope(self):
        matrix = self.green_matrix()
        matrix["lanes"].append(
            {
                "core": "lib",
                "language": "java",
                "verdict": "orchestration-lifecycle-passed",
                "dependency_owner": "mgc-native",
                "install_owner": "native-engine",
                "native_pm_verdict": "native-pm-supported",
                "native_pm_delegated": [],
                "dimensions": {},
                "owner_by_operation": {},
            }
        )
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("lib/java declares native-pm-supported without complete package-manager evidence" in error for error in errors))

    def test_platform_green_rejects_lane_owner_that_differs_from_source_contract(self):
        matrix = self.green_matrix()
        lane = matrix["lanes"][0]
        lane["dependency_owner"] = "delegated"
        lane["install_owner"] = "plain-delegation"
        lane["native_pm_verdict"] = "compatibility-passed"
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("dependency owner differs from source contract" in error for error in errors))

    def test_failed_required_probe_blocks_without_owner_status_mismatch(self):
        matrix = self.green_matrix()
        lane = next(
            row for row in matrix["lanes"]
            if row["core"] == "web" and row["language"] == "node"
        )
        lane["dimensions"]["test"] = STATUS_FAILED
        lane["dimensions"]["build"] = STATUS_FAILED
        lane["verdict"] = "partial"
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("non-passing required dimensions" in error and "test=failed" in error for error in errors))
        self.assertFalse(any("status/owner mismatch for test" in error for error in errors))
        self.assertFalse(any("status/owner mismatch for build" in error for error in errors))

    def test_platform_green_requires_exact_reason_for_every_scoped_out_lane(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "app" and lane["language"] == "objc"
        )
        lane["release_scope_out_reason"] = ""
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("scope-out reason differs from source contract" in error for error in errors))

    def test_scoped_out_lane_keeps_owner_status_consistency_when_toolchain_is_available(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "clo" and lane["language"] == "terraform"
        )
        source_lane = next(
            source for source in LANES
            if source["core"] == "clo" and source["language"] == "terraform"
        )
        lane["lifecycle_owners"] = {
            dimension: lifecycle_owner_for(source_lane, dimension)
            for dimension in source_lane["required_dims"]
        }
        lane["dimensions"]["install"] = STATUS_NATIVE
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("status/owner mismatch for install" in error for error in errors))

    def test_platform_green_rejects_lane_without_its_required_toolchain(self):
        matrix = self.green_matrix()
        matrix["lanes"][0]["toolchain_available"] = False
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("toolchain is unavailable" in error for error in errors))

    def test_lane_cannot_shrink_source_required_dimensions(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if (lane["core"], lane["language"]) == ("web", "javascript")
        )
        lane["required_dimensions"] = ["create"]
        lane["dimensions"]["dev"] = STATUS_UNSUPPORTED
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("required dimensions differ from source contract" in error for error in errors))
        self.assertTrue(any("dev=unsupported" in error for error in errors))

    def test_explicitly_non_applicable_dimension_may_be_unsupported(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if (lane["core"], lane["language"]) == ("lib", "rust")
        )
        lane["dimensions"]["dev"] = STATUS_UNSUPPORTED
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertFalse(any("dimension dev" in error for error in errors))

    def test_unknown_dimension_cannot_be_added_outside_the_v12_contract(self):
        matrix = self.green_matrix()
        matrix["lanes"][0]["dimensions"]["shadow-security"] = STATUS_NATIVE
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("unknown dimensions" in error for error in errors))

    def test_dirty_or_unknown_source_tree_cannot_record_platform_green(self):
        for value in (False, None):
            with self.subTest(working_tree_clean=value):
                matrix = self.green_matrix()
                if value is None:
                    del matrix["working_tree_clean"]
                else:
                    matrix["working_tree_clean"] = value
                errors = platform_evidence_errors(
                    matrix, "a" * 40, current_tree_clean=True
                )
                self.assertTrue(any("working tree" in error for error in errors))

    def test_stack_overflow_lane_cannot_increment_green_counter(self):
        matrix = self.green_matrix()
        matrix["lanes"][0]["dimensions"]["create"] = STATUS_FAILED
        matrix["lanes"][0]["verdict"] = "partial"
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("create" in error for error in errors))

    def test_missing_release_lane_cannot_increment_green_counter(self):
        matrix = self.green_matrix()
        matrix["lanes"].pop()
        self.assertTrue(
            platform_evidence_errors(
                matrix, "a" * 40, current_tree_clean=True
            )
        )

    def test_missing_full_framework_inventory_cannot_increment_green_counter(self):
        matrix = self.green_matrix()
        del matrix["framework_catalog"]
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("framework catalog" in error for error in errors))

    def test_framework_id_is_part_of_release_evidence_identity(self):
        matrix = self.green_matrix()
        cdk = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "clo" and lane.get("framework_id") == "cdk"
        )
        cdk["framework_id"] = ""
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("outside global v1.2 scope" in error for error in errors))
        self.assertTrue(any("clo/cdk" in error and "missing" in error for error in errors))

    def test_lifecycle_owner_must_match_the_source_contract(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "app" and lane["language"] == "flutter"
        )
        source_lane = next(
            source for source in LANES
            if source["core"] == "app" and source["language"] == "flutter"
        )
        source_owner = lifecycle_owner_for(source_lane, "build")
        lane["lifecycle_owners"]["build"] = (
            "mgc-native" if source_owner == "plain-delegation" else "plain-delegation"
        )
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("build" in error and "source contract" in error for error in errors))

    def test_native_lifecycle_status_must_match_declared_owner(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "app" and lane["language"] == "flutter"
        )
        source_lane = next(
            source for source in LANES
            if source["core"] == "app" and source["language"] == "flutter"
        )
        expected_status = lifecycle_status_for_owner(
            lifecycle_owner_for(source_lane, "build")
        )
        lane["dimensions"]["build"] = (
            STATUS_NATIVE if expected_status != STATUS_NATIVE else STATUS_PLAIN
        )
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("status/owner mismatch for build" in error for error in errors))

    def test_platform_evidence_rejects_owner_claim_not_in_source_registry(self):
        matrix = self.green_matrix()
        lane = next(
            lane for lane in matrix["lanes"]
            if lane["core"] == "app" and lane["language"] == "flutter"
        )
        lane["lifecycle_owners"]["build"] = "mgc-native"
        lane["dimensions"]["build"] = STATUS_NATIVE
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(
            any("lifecycle owner differs from source contract for build" in error for error in errors)
        )

    def test_lifecycle_owner_status_mapping_is_fail_closed(self):
        self.assertEqual(lifecycle_status_for_owner("mgc-native"), STATUS_NATIVE)
        self.assertEqual(lifecycle_status_for_owner("plain-delegation"), STATUS_PLAIN)
        self.assertEqual(lifecycle_status_for_owner("unknown"), STATUS_UNVERIFIED)

    def test_external_test_and_build_wrappers_are_not_native_evidence(self):
        for lane in LANES:
            for dimension in ("test", "build", "run"):
                with self.subTest(core=lane["core"], language=lane["language"], dimension=dimension):
                    owner = lifecycle_owner_for(lane, dimension)
                    if owner == "unsupported":
                        self.assertEqual(
                            (lane["core"], lane["language"], lane.get("framework_id", "")),
                            ("lib", "dotnet", "dotnet"),
                        )
                        self.assertEqual(
                            lifecycle_pass_status(lane, dimension), STATUS_UNSUPPORTED
                        )
                        continue
                    self.assertEqual(owner, "plain-delegation")
                    self.assertEqual(lifecycle_pass_status(lane, dimension), STATUS_PLAIN)

    def test_only_explicit_mgc_dev_server_lane_gets_native_dev_owner(self):
        web = next(
            lane for lane in LANES
            if lane["core"] == "web" and lane["language"] == "javascript"
        )
        other = next(lane for lane in LANES if lane is not web)
        self.assertEqual(lifecycle_owner_for(web, "dev"), "mgc-native")
        self.assertEqual(lifecycle_pass_status(web, "dev"), STATUS_NATIVE)
        self.assertEqual(lifecycle_owner_for(other, "dev"), "unverified")
        self.assertEqual(lifecycle_pass_status(other, "dev"), STATUS_UNVERIFIED)

    def test_missing_lifecycle_owner_is_unverified_not_native(self):
        lane = {"lifecycle_owners": {}}
        self.assertEqual(lifecycle_owner_for(lane, "optimizer"), "unverified")
        self.assertEqual(lifecycle_pass_status(lane, "optimizer"), STATUS_UNVERIFIED)

    def test_matrix_from_another_checkout_cannot_increment_green_counter(self):
        self.assertTrue(
            platform_evidence_errors(
                self.green_matrix(), "b" * 40, current_tree_clean=True
            )
        )

    def test_matrix_from_another_schema_cannot_increment_green_counter(self):
        matrix = self.green_matrix()
        matrix["schema_version"] = SCHEMA_VERSION - 1
        errors = platform_evidence_errors(
            matrix, "a" * 40, current_tree_clean=True
        )
        self.assertTrue(any("schema_version" in error for error in errors))

    def test_missing_required_dimension_cannot_increment_green_counter(self):
        matrix = self.green_matrix()
        del matrix["lanes"][0]["dimensions"]["install"]
        self.assertTrue(
            platform_evidence_errors(
                matrix, "a" * 40, current_tree_clean=True
            )
        )

    def test_record_green_rejects_unknown_platform_key(self):
        with patch("lifecycle_capability_matrix._fail", side_effect=SystemExit):
            with self.assertRaises(SystemExit):
                record_platform_green("macos-arm64")

    def test_record_green_refuses_dirty_checkout_without_writing_counter(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            matrix_path = Path(temp_dir) / "matrix.json"
            matrix_path.write_text(json.dumps(self.green_matrix()), encoding="utf-8")
            with (
                patch.dict(
                    "os.environ",
                    {
                        "MGC_LIFECYCLE_MATRIX_OUT": str(matrix_path),
                        "MGC_PLATFORM_EVIDENCE_SHA": "a" * 40,
                        "CI": "true",
                    },
                    clear=True,
                ),
                patch(
                    "lifecycle_capability_matrix.subprocess.run",
                    return_value=subprocess.CompletedProcess(
                        ["git"], 0, stdout="a" * 40, stderr=""
                    ),
                ),
                patch(
                    "lifecycle_capability_matrix.lifecycle_working_tree_clean",
                    return_value=False,
                ),
                patch("lifecycle_capability_matrix._platform_evidence_write") as write,
                patch("lifecycle_capability_matrix._fail", side_effect=SystemExit),
            ):
                with self.assertRaises(SystemExit):
                    record_platform_green("windows-latest")
                write.assert_not_called()

    def test_record_green_refuses_workflow_sha_mismatch_without_writing(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            matrix_path = Path(temp_dir) / "matrix.json"
            matrix_path.write_text(json.dumps(self.green_matrix()), encoding="utf-8")
            with (
                patch.dict(
                    "os.environ",
                    {
                        "MGC_LIFECYCLE_MATRIX_OUT": str(matrix_path),
                        "MGC_PLATFORM_EVIDENCE_SHA": "b" * 40,
                        "CI": "true",
                    },
                    clear=True,
                ),
                patch(
                    "lifecycle_capability_matrix.subprocess.run",
                    return_value=subprocess.CompletedProcess(
                        ["git"], 0, stdout="a" * 40, stderr=""
                    ),
                ),
                patch(
                    "lifecycle_capability_matrix.lifecycle_working_tree_clean",
                    return_value=True,
                ),
                patch("lifecycle_capability_matrix._platform_evidence_write") as write,
                patch("lifecycle_capability_matrix._fail", side_effect=SystemExit),
            ):
                with self.assertRaises(SystemExit):
                    record_platform_green("windows-latest")
                write.assert_not_called()

    def test_record_green_increments_only_a_clean_same_sha_matrix(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            matrix_path = Path(temp_dir) / "matrix.json"
            matrix_path.write_text(json.dumps(self.green_matrix()), encoding="utf-8")
            with (
                patch.dict(
                    "os.environ",
                    {
                        "MGC_LIFECYCLE_MATRIX_OUT": str(matrix_path),
                        "MGC_PLATFORM_EVIDENCE_SHA": "a" * 40,
                        "CI": "true",
                    },
                    clear=True,
                ),
                patch(
                    "lifecycle_capability_matrix.subprocess.run",
                    return_value=subprocess.CompletedProcess(
                        ["git"], 0, stdout="a" * 40, stderr=""
                    ),
                ),
                patch(
                    "lifecycle_capability_matrix.lifecycle_working_tree_clean",
                    return_value=True,
                ),
                patch(
                    "lifecycle_capability_matrix._platform_evidence_load",
                    return_value={"windows-latest": {"runs": 2}},
                ),
                patch("lifecycle_capability_matrix._platform_evidence_write") as write,
            ):
                self.assertEqual(record_platform_green("windows-latest"), 0)
                written = write.call_args.args[0]
                self.assertEqual(written["windows-latest"]["runs"], 3)
                self.assertEqual(
                    written["windows-latest"]["last_green_sha"], "a" * 40
                )

    def test_ci_green_record_rejects_missing_recorded_sha(self):
        self.assertIsNotNone(
            evidence_commit_error(
                "a" * 40,
                "a" * 40,
                is_ci=True,
                recorded_sha=None,
                require_recorded=True,
            )
        )


class ProvenanceMatrixCleanliness(unittest.TestCase):
    def test_dirty_or_legacy_matrix_cannot_bind_to_current_head(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            matrix_path = Path(temp_dir) / "matrix.json"
            base = {"commit": "a" * 40, "working_tree_clean": True}
            for matrix, clean in (
                (base, False),
                ({"commit": "a" * 40}, True),
            ):
                with self.subTest(matrix=matrix, clean=clean):
                    matrix_path.write_text(json.dumps(matrix), encoding="utf-8")
                    with (
                        patch("provenance_chain.MATRIX_PATH", matrix_path),
                        patch("provenance_chain._working_tree_clean", return_value=clean),
                    ):
                        self.assertEqual(_matrix_sha_for_head("a" * 40), (None, False))


class WorkingTreeCleanProbe(unittest.TestCase):
    @patch("lifecycle_capability_matrix.subprocess.run")
    def test_clean_probe_fails_closed_for_dirty_git_and_command_errors(self, run):
        run.return_value.returncode = 0
        run.return_value.stdout = ""
        self.assertTrue(lifecycle_working_tree_clean())

        run.return_value.stdout = " M cli/src/main.rs\n"
        self.assertFalse(lifecycle_working_tree_clean())

        run.return_value.stdout = ""
        run.return_value.returncode = 128
        self.assertFalse(lifecycle_working_tree_clean())

        run.side_effect = OSError("git unavailable")
        self.assertFalse(lifecycle_working_tree_clean())

    @patch("lifecycle_capability_matrix.subprocess.run")
    def test_matrix_source_must_remain_clean_at_the_same_commit(self, run):
        clean = subprocess.CompletedProcess([], 0, stdout="", stderr="")
        same_head = subprocess.CompletedProcess([], 0, stdout="a" * 40, stderr="")
        other_head = subprocess.CompletedProcess([], 0, stdout="b" * 40, stderr="")
        failed_git = subprocess.CompletedProcess([], 128, stdout="", stderr="fatal")

        run.side_effect = [clean, same_head]
        self.assertTrue(lifecycle_source_matches("a" * 40, True))

        run.side_effect = [
            subprocess.CompletedProcess([], 0, stdout=" M cli/src/main.rs", stderr="")
        ]
        self.assertFalse(lifecycle_source_matches("a" * 40, True))

        run.side_effect = [clean, other_head]
        self.assertFalse(lifecycle_source_matches("a" * 40, True))

        run.side_effect = [clean, failed_git]
        self.assertFalse(lifecycle_source_matches("a" * 40, True))

        run.reset_mock()
        self.assertFalse(lifecycle_source_matches("a" * 40, False))
        run.assert_not_called()


class WindowsMatrixRuntime(unittest.TestCase):
    def test_captured_child_output_replaces_bytes_outside_windows_ansi_codepage(self):
        result = matrix_module.run_text_capture(
            [
                sys.executable,
                "-c",
                "import sys; sys.stdout.buffer.write(bytes([0x90]) + b'ok'); "
                "sys.stderr.buffer.write(bytes([0x90]) + b'err')",
            ],
            capture_output=True,
        )
        self.assertEqual(result.stdout, "\ufffdok")
        self.assertEqual(result.stderr, "\ufffderr")

    def test_gitignore_excludes_python_bytecode_from_matrix_provenance(self):
        ignore_rules = (
            Path(__file__).resolve().parent.parent / ".gitignore"
        ).read_text(encoding="utf-8")
        self.assertIn("\n__pycache__/\n", ignore_rules)
        self.assertIn("\n*.py[cod]\n", ignore_rules)


if __name__ == "__main__":
    unittest.main(verbosity=2)
