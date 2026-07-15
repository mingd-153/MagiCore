use crate::ecosystem::Ecosystem;
use crate::package::DependencySpec;
use crate::version::Version;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Manifest {
    pub name: String,
    pub ecosystem: Ecosystem,
    pub version: Option<Version>,
    pub dependencies: Vec<DependencySpec>,
    pub dev_dependencies: Vec<DependencySpec>,
    pub peer_dependencies: Vec<DependencySpec>,
    pub optional_dependencies: Vec<DependencySpec>,
}

impl Manifest {
    pub fn new(name: &str, ecosystem: Ecosystem) -> Self {
        Self {
            name: name.to_string(),
            ecosystem,
            version: None,
            dependencies: vec![],
            dev_dependencies: vec![],
            peer_dependencies: vec![],
            optional_dependencies: vec![],
        }
    }

    pub fn add_dep(&mut self, spec: DependencySpec, dev: bool, optional: bool, peer: bool) {
        if peer {
            push_unique(&mut self.peer_dependencies, spec);
        } else if optional {
            push_unique(&mut self.optional_dependencies, spec);
        } else if dev {
            push_unique(&mut self.dev_dependencies, spec);
        } else {
            push_unique(&mut self.dependencies, spec);
        }
    }

    pub fn remove_dep(&mut self, name: &str) {
        for deps in [
            &mut self.dependencies,
            &mut self.dev_dependencies,
            &mut self.peer_dependencies,
            &mut self.optional_dependencies,
        ] {
            deps.retain(|dep| dep.name.as_str() != name);
        }
    }

    pub fn all_dependencies(&self) -> impl Iterator<Item = &DependencySpec> {
        self.dependencies
            .iter()
            .chain(self.dev_dependencies.iter())
            .chain(self.peer_dependencies.iter())
            .chain(self.optional_dependencies.iter())
    }

    pub fn find_dep(&self, name: &str) -> Option<&DependencySpec> {
        self.all_dependencies()
            .find(|dep| dep.name.as_str() == name)
    }

    pub fn dep_groups(&self) -> [(&str, &[DependencySpec]); 4] {
        [
            ("dependencies", &self.dependencies),
            ("devDependencies", &self.dev_dependencies),
            ("peerDependencies", &self.peer_dependencies),
            ("optionalDependencies", &self.optional_dependencies),
        ]
    }
}

fn push_unique(list: &mut Vec<DependencySpec>, spec: DependencySpec) {
    list.retain(|dep| dep.name != spec.name);
    list.push(spec);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecosystem::Ecosystem;
    use crate::package::{PackageName, VersionRange};

    fn dep(name: &str) -> DependencySpec {
        DependencySpec::new(PackageName::new(name).unwrap(), VersionRange::star())
    }

    #[test]
    fn manifest_new_sets_fields() {
        let m = Manifest::new("test", Ecosystem::Web);
        assert_eq!(m.name, "test");
        assert_eq!(m.ecosystem, Ecosystem::Web);
        assert!(m.version.is_none());
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn add_dep_puts_in_correct_group() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        let d = dep("foo");
        m.add_dep(d, false, false, false);
        assert_eq!(m.dependencies.len(), 1);
        assert!(m.dev_dependencies.is_empty());
    }

    #[test]
    fn add_dep_with_dev_goes_to_dev() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("foo"), true, false, false);
        assert_eq!(m.dev_dependencies.len(), 1);
        assert!(m.dependencies.is_empty());
    }

    #[test]
    fn add_dep_with_optional_goes_to_optional() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("foo"), false, true, false);
        assert_eq!(m.optional_dependencies.len(), 1);
    }

    #[test]
    fn add_dep_with_peer_goes_to_peer() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("foo"), false, false, true);
        assert_eq!(m.peer_dependencies.len(), 1);
    }

    #[test]
    fn add_dep_dedup_replaces_existing() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        let d1 = DependencySpec {
            name: PackageName::new("foo").unwrap(),
            range: VersionRange::parse("1.0.0").unwrap(),
            dev: false,
            optional: false,
            peer: false,
        };
        let d2 = DependencySpec {
            name: PackageName::new("foo").unwrap(),
            range: VersionRange::parse("2.0.0").unwrap(),
            dev: false,
            optional: false,
            peer: false,
        };
        m.add_dep(d1, false, false, false);
        m.add_dep(d2, false, false, false);
        assert_eq!(m.dependencies.len(), 1);
        assert_eq!(m.dependencies[0].range.as_str(), "2.0.0");
    }

    #[test]
    fn remove_dep_removes_from_all_groups() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        let d = dep("foo");
        m.add_dep(d.clone(), false, false, false);
        m.add_dep(d.clone(), true, false, false);
        m.add_dep(d.clone(), false, true, false);
        m.add_dep(d, false, false, true);
        assert_eq!(m.all_dependencies().count(), 4);
        m.remove_dep("foo");
        assert_eq!(m.all_dependencies().count(), 0);
    }

    #[test]
    fn remove_dep_only_removes_target() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("foo"), false, false, false);
        m.add_dep(dep("bar"), true, false, false);
        m.remove_dep("foo");
        assert_eq!(m.dependencies.len(), 0);
        assert_eq!(m.dev_dependencies.len(), 1);
    }

    #[test]
    fn find_dep_searches_all_groups() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("foo"), false, false, true);
        assert!(m.find_dep("foo").is_some());
        assert!(m.find_dep("missing").is_none());
    }

    #[test]
    fn all_dependencies_yields_all_groups() {
        let mut m = Manifest::new("test", Ecosystem::Lib);
        m.add_dep(dep("a"), false, false, false);
        m.add_dep(dep("b"), true, false, false);
        m.add_dep(dep("c"), false, true, false);
        m.add_dep(dep("d"), false, false, true);
        assert_eq!(m.all_dependencies().count(), 4);
    }

    #[test]
    fn dep_groups_returns_four_entries() {
        let m = Manifest::new("test", Ecosystem::Lib);
        let groups = m.dep_groups();
        assert_eq!(groups.len(), 4);
        assert_eq!(groups[0].0, "dependencies");
        assert_eq!(groups[1].0, "devDependencies");
        assert_eq!(groups[2].0, "peerDependencies");
        assert_eq!(groups[3].0, "optionalDependencies");
    }
}
