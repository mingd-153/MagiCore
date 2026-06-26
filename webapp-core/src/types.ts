export interface PackageJson {
    name?: string;
    version?: string;
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    scripts?: Record<string, string>;
    main?: string;
    module?: string;
    types?: string;
    exports?: Record<string, any>;
    files?: string[];
}

export interface RegistryPackage {
    name: string;
    version: string;
    integrity: string;
    dependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    dist: {
        tarball: string;
        shasum: string;
        integrity: string;
        size: number;
    };
    versions?: Record<string, RegistryVersion>;
}

export interface RegistryVersion {
    name: string;
    version: string;
    integrity: string;
    dependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    dist: {
        tarball: string;
        shasum: string;
        integrity: string;
        size: number;
    };
}

export interface LockFile {
    version: number;
    packages: Record<string, LockPackage>;
    importers: Record<string, LockImporter>;
    store: {
        dir: string;
        layout: string;
    };
}

export interface LockPackage {
    name: string;
    version: string;
    integrity: string;
    dependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
    bin?: Record<string, string>;
    engines?: Record<string, string>;
    resolved: string;
    size: number;
}

export interface LockImporter {
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    optionalDependencies?: Record<string, string>;
}

export interface StoreMeta {
    integrity: string;
    size: number;
    extractedAt: string;
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
}

export interface InstallOptions {
    frozenLockfile?: boolean;
    production?: boolean;
    registry?: string;
    storeDir?: string;
}

export interface AddOptions {
    dev?: boolean;
    optional?: boolean;
    registry?: string;
    storeDir?: string;
}

export interface UpdateOptions {
    latest?: boolean;
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
