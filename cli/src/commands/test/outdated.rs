use mgc_types::adapter::{ResolvedGraph, ResolvedPackage};
use mgc_types::{
    DependencySpec, Ecosystem, Manifest, PackageId, PackageName, Version, VersionRange,
};

fn package(name: &str, version: &str, direct: bool) -> ResolvedPackage {
    ResolvedPackage {
        id: PackageId::new(
            PackageName::new(name).unwrap(),
            Version::parse(version).unwrap(),
        ),
        integrity: String::new(),
        tarball_url: String::new(),
        deps: Vec::new(),
        peer_deps: Vec::new(),
        direct,
        dev: false,
    }
}

#[test]
fn latest_candidate_manifest_removes_constraints_from_all_dependency_groups() {
    let mut manifest = Manifest::new("fixture", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("runtime").unwrap(),
            VersionRange::parse("^1.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("dev-only").unwrap(),
            VersionRange::parse("~2.0").unwrap(),
        ),
        true,
        false,
        false,
    );
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("peer-only").unwrap(),
            VersionRange::parse("3.0.0").unwrap(),
        ),
        false,
        false,
        true,
    );
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("optional-only").unwrap(),
            VersionRange::parse("4.0.0").unwrap(),
        ),
        false,
        true,
        false,
    );

    let latest = super::latest_candidate_manifest(&manifest);

    assert_eq!(latest.dependencies.len(), 4);
    assert!(
        latest
            .dependencies
            .iter()
            .all(|dependency| dependency.range.is_star()
                && !dependency.dev
                && !dependency.optional
                && !dependency.peer)
    );
    assert!(latest.dev_dependencies.is_empty());
    assert!(latest.peer_dependencies.is_empty());
    assert!(latest.optional_dependencies.is_empty());
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dev_dependencies.len(), 1);
    assert_eq!(manifest.peer_dependencies.len(), 1);
    assert_eq!(manifest.optional_dependencies.len(), 1);
    assert_eq!(manifest.dependencies[0].range.as_str(), "^1.0");
}

#[test]
fn native_outdated_compares_locked_direct_packages_and_ignores_transitives() {
    let mut manifest = Manifest::new("fixture", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("runtime").unwrap(),
            VersionRange::parse("^1.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let current = ResolvedGraph {
        packages: vec![
            package("runtime", "1.4.0", true),
            package("transitive", "4.0.0", false),
        ],
    };
    let latest = ResolvedGraph {
        packages: vec![
            package("runtime", "2.0.0", true),
            package("transitive", "8.0.0", false),
        ],
    };

    let outdated = super::native_outdated_packages(&manifest, &current, &latest).unwrap();

    assert_eq!(outdated.len(), 1);
    assert_eq!(outdated[0].name, "runtime");
    assert_eq!(outdated[0].current, "1.4.0");
    assert_eq!(outdated[0].latest, "2.0.0");
}

#[test]
fn native_outdated_reports_a_package_once_when_declared_in_multiple_groups() {
    let mut manifest = Manifest::new("fixture", Ecosystem::Lib);
    manifest.dependencies.push(DependencySpec::new(
        PackageName::new("shared").unwrap(),
        VersionRange::parse("^1.0").unwrap(),
    ));
    manifest.dev_dependencies.push(DependencySpec::new(
        PackageName::new("shared").unwrap(),
        VersionRange::parse("~1.2").unwrap(),
    ));
    let current = ResolvedGraph {
        packages: vec![package("shared", "1.4.0", true)],
    };
    let latest = ResolvedGraph {
        packages: vec![package("shared", "2.0.0", true)],
    };

    let outdated = super::native_outdated_packages(&manifest, &current, &latest).unwrap();

    assert_eq!(outdated.len(), 1);
    assert_eq!(outdated[0].name, "shared");
}

#[test]
fn native_outdated_fails_when_a_direct_lock_pin_is_missing() {
    let mut manifest = Manifest::new("fixture", Ecosystem::Lib);
    manifest.add_dep(
        DependencySpec::new(
            PackageName::new("runtime").unwrap(),
            VersionRange::parse("^1.0").unwrap(),
        ),
        false,
        false,
        false,
    );
    let latest = ResolvedGraph {
        packages: vec![package("runtime", "2.0.0", true)],
    };

    let error = super::native_outdated_packages(&manifest, &ResolvedGraph::empty(), &latest)
        .expect_err("missing locked current version must not look up to date");

    assert!(error.to_string().contains("runtime"));
}
