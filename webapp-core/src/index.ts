export { createInstaller, Installer } from './installer.js';
export type { InstallResult } from './installer.js';
export { Resolver } from './resolver.js';
export type { ResolutionResult } from './resolver.js';
export { Fetcher } from './fetcher.js';
export { Linker } from './linker.js';
export { RegistryClient } from './registry.js';
export { loadLock, saveLock, createEmptyLock, verifyLockIntegrity, getLockPath } from './lock.js';
export {
    getStoreDir,
    ensureStoreDirs,
    getPackageStorePath,
    computeIntegrity,
    verifyIntegrity,
} from './store.js';

export type {
    PackageJson,
    RegistryPackage,
    RegistryVersion,
    ResolvedDependency,
    StoreMeta,
    InstallOptions,
    AddOptions,
    UpdateOptions,
    LinkOptions,
    FetchResult,
    LockFile,
    LockPackage,
    LockImporter,
} from './types.js';
