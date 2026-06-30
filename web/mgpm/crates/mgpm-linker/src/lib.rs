pub mod linker;

pub use linker::{
    Linker, LinkerOptions, PackageLinkInfo, LinkResult,
    PackageLinkResult, LinkError, RefcountCallback,
    LinkerStrategy, IsolatedLinker,
};
