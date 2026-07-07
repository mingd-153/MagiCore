const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // ── C Library ──
    const c_lib = b.addStaticLibrary(.{
        .name = "mg_core_c",
        .target = target,
        .optimize = optimize,
    });

    c_lib.addCSourceFiles(.{
        .root = b.path("."),
        .files = &.{
            "crates/mg-core-c/src/semver.c",
            "crates/mg-core-c/src/json_extract.c",
            "crates/mg-core-c/src/sha256.c",
        },
        .flags = &.{ "-std=c99", "-Wall", "-Wextra", "-Werror", "-O2" },
    });
    c_lib.addIncludePath(b.path("crates/mg-core-c/include"));
    c_lib.linkLibC();

    // Install static library for Rust to link
    b.installArtifact(c_lib);

    // ── C Tests ──
    const c_semver_test = b.addExecutable(.{
        .name = "test_semver",
        .target = target,
        .optimize = optimize,
    });
    c_semver_test.addCSourceFiles(.{
        .root = b.path("."),
        .files = &.{
            "crates/mg-core-c/src/semver.c",
            "crates/mg-core-c/src/test/test_semver.c",
        },
        .flags = &.{ "-std=c99", "-Wall", "-Wextra", "-O0", "-g" },
    });
    c_semver_test.addIncludePath(b.path("crates/mg-core-c/include"));
    c_semver_test.linkLibC();

    const c_json_test = b.addExecutable(.{
        .name = "test_json",
        .target = target,
        .optimize = optimize,
    });
    c_json_test.addCSourceFiles(.{
        .root = b.path("."),
        .files = &.{
            "crates/mg-core-c/src/json_extract.c",
            "crates/mg-core-c/src/test/test_json.c",
        },
        .flags = &.{ "-std=c99", "-Wall", "-Wextra", "-O0", "-g" },
    });
    c_json_test.addIncludePath(b.path("crates/mg-core-c/include"));
    c_json_test.linkLibC();

    const c_sha256_test = b.addExecutable(.{
        .name = "test_sha256",
        .target = target,
        .optimize = optimize,
    });
    c_sha256_test.addCSourceFiles(.{
        .root = b.path("."),
        .files = &.{
            "crates/mg-core-c/src/sha256.c",
            "crates/mg-core-c/src/test/test_sha256.c",
        },
        .flags = &.{ "-std=c99", "-Wall", "-Wextra", "-O0", "-g" },
    });
    c_sha256_test.addIncludePath(b.path("crates/mg-core-c/include"));
    c_sha256_test.linkLibC();

    const run_semver_test = b.addRunArtifact(c_semver_test);
    const run_json_test = b.addRunArtifact(c_json_test);
    const run_sha256_test = b.addRunArtifact(c_sha256_test);
    const test_c_step = b.step("test-c", "Run C library tests");
    test_c_step.dependOn(&run_semver_test.step);
    test_c_step.dependOn(&run_json_test.step);
    test_c_step.dependOn(&run_sha256_test.step);

    // ── Rust via Cargo ──
    const cargo_build = b.addSystemCommand(&.{ "cargo", "build", "--workspace" });
    cargo_build.setEnvironmentVariable("CC", "zig cc");

    const cargo_test = b.addSystemCommand(&.{ "cargo", "test", "--workspace" });
    cargo_test.setEnvironmentVariable("CC", "zig cc");

    const cargo_check = b.addSystemCommand(&.{ "cargo", "check", "--workspace" });
    cargo_check.setEnvironmentVariable("CC", "zig cc");

    const cargo_clippy = b.addSystemCommand(&.{ "cargo", "clippy", "--workspace" });
    cargo_clippy.setEnvironmentVariable("CC", "zig cc");

    // ── Combine steps ──
    const test_step = b.step("test", "Run all tests (C + Rust)");
    test_step.dependOn(test_c_step);
    test_step.dependOn(&cargo_test.step);

    const check_step = b.step("check", "Check all (build + test + clippy)");
    check_step.dependOn(&cargo_check.step);
    check_step.dependOn(test_step);
    check_step.dependOn(&cargo_clippy.step);

    // Default: build + test
    b.default_step.dependOn(&cargo_build.step);
}
