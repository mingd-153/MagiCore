// MegaGate Package Manager - Core Types
// Single source of truth for all types

import { Readable } from 'stream';

export interface MegagateConfig {
    storeDir: string;
    registry: string;
    minimumReleaseAgeHours: number;
    approveBuilds: boolean;
    lockdownMode: boolean;
    linkStrategy: 'hardlink' | 'symlink' | 'copy';
    maxConcurrency: number;
    offlineMode: boolean;
    preferOffline: boolean;
    timeout: number;
    retries: number;
}

export interface PackageManifest {
    name: string;
    version: string;
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    peerDependenciesMeta?: Record<string, { optional: boolean }>;
    bin?: Record<string, string>;
    scripts?: Record<string, string>;
    engines?: { node?: string; megagate?: string };
    files?: string[];
    main?: string;
    module?: string;
    types?: string;
    exports?: any;
    sideEffects?: boolean | string[];
    megagate?: {
        lockdown?: boolean;
        entryPoints?: string[];
        testEntryPoints?: string[];
    };
}

export interface LockfileV1 {
    version: 1;
    lockfileVersion: 1;
    packages: Record<string, LockedPackage>;
    importers: Record<string, ImporterDeps>;
    store: { dir: string; layoutVersion: 1 };
    metadata: {
        createdAt: string;
        megagateVersion: string;
        contentHash: string;
    };
}

export interface LockedPackage {
    name: string;
    version: string;
    integrity: string;
    resolved: string;
    size: number;
    dependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    provenance?: ProvenanceInfo;
    approvedBuilds?: string[];
    publishTime?: string;
}

export interface ProvenanceInfo {
    repositoryUrl?: string;
    commitHash?: string;
    builderId?: string;
    signature?: string;
}

export interface ImporterDeps {
    dependencies: Record<string, string>;
    devDependencies: Record<string, string>;
    optionalDependencies: Record<string, string>;
}

export interface ResolvedDependency {
    name: string;
    version: string;
    integrity: string;
    resolved: string;
    size: number;
    dependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    scripts?: Record<string, string>;
    publishTime?: string;
    approvedBuilds?: string[];
}

export interface InstallOptions {
    frozenLockfile?: boolean;
    production?: boolean;
    registry?: string;
    storeDir?: string;
}

export interface FetchResult {
    tarballPath: string;
    extractPath: string;
    integrity: string;
    size: number;
}

export interface LinkOptions {
    cwd?: string;
    storeDir?: string;
    production?: boolean;
}

export interface WorkspaceConfig {
    packages: string[];
    catalog?: Record<string, string>;
    overrides?: Record<string, string>;
    linkWorkspacePackages?: 'shallow' | 'deep' | false;
}

export interface PackageRef {
    name: string;
    version: string;
}

export interface IntegrityInfo {
    integrity: string;
    size: number;
}

export interface PackageMetadata {
    integrity: string;
    size: number;
    extractedAt: string;
    publishTime?: string;
    approvedBuilds?: string[];
}

export interface PruneResult {
    removed: number;
    freedBytes: number;
}

export interface SecurityCheckResult {
    passed: boolean;
    violations: string[];
    warnings: string[];
    checks: {
        typosquat: boolean;
        slopsquat: boolean;
        minimumAge: boolean;
        approveBuilds: boolean;
        lockdown: boolean;
        provenance: boolean;
    };
}

export interface LockdownCheckResult {
    allowed: boolean;
    reason?: string;
    approvedBuilds: string[];
}

export interface StoreBackend {
    init(config: MegagateConfig): Promise<void>;
    exists(pkg: PackageRef): Promise<boolean>;
    getPath(pkg: PackageRef): string;
    writeTarball(pkg: PackageRef, stream: Readable): Promise<IntegrityInfo>;
    readTarball(pkg: PackageRef): Promise<Readable>;
    writeManifest(pkg: PackageRef, manifest: PackageManifest): Promise<void>;
    readManifest(pkg: PackageRef): Promise<PackageManifest | null>;
    writeMetadata(pkg: PackageRef, meta: PackageMetadata): Promise<void>;
    readMetadata(pkg: PackageRef): Promise<PackageMetadata | null>;
    createHardlink(pkg: PackageRef, target: string): Promise<void>;
    createSymlink(pkg: PackageRef, target: string): Promise<void>;
    remove(pkg: PackageRef): Promise<void>;
    prune(referenced: Set<string>): Promise<PruneResult>;
    verifyIntegrity(pkg: PackageRef): Promise<boolean>;
}
