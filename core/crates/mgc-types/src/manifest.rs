use crate::ecosystem::Ecosystem;
use crate::package::DependencySpec;
use crate::version::Version;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

    pub fn add_dep(&mut self, spec: DependencySpec, dev: bool, optional: bool, peer: bool) -> bool {
        let before = self.clone();
        self.remove_dep(spec.name.as_str());
        if peer {
            self.peer_dependencies.push(spec);
        } else if optional {
            self.optional_dependencies.push(spec);
        } else if dev {
            self.dev_dependencies.push(spec);
        } else {
            self.dependencies.push(spec);
        }
        *self != before
    }

    pub fn remove_dep(&mut self, name: &str) -> bool {
        let mut changed = false;
        for deps in [
            &mut self.dependencies,
            &mut self.dev_dependencies,
            &mut self.peer_dependencies,
            &mut self.optional_dependencies,
        ] {
            let before = deps.len();
            deps.retain(|dep| dep.name.as_str() != name);
            changed |= deps.len() != before;
        }
        changed
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
