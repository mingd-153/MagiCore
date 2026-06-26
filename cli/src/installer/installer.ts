import { readFile, writeFile, access, constants } from 'fs/promises';
import { resolve } from 'path';
import {
    PackageJson,
    LockFile,
    InstallOptions,
    AddOptions,
    UpdateOptions,
    ResolvedDependency,
    type FetchResult,
} from '../types.js';
import {
    loadLock,
    saveLock,
    createEmptyLock,
    lockKey,
    addPackageToLock,
    getImporterDeps,
    updateImporter,
    verifyLockIntegrity,
} from '../lockfile/lock.js';
import { resolveDependencies, Resolver } from '../resolver.js';
import { createFetcher } from '../fetcher.js';
import { createLinker, Linker } from '../linker/linker.js';
import { getStoreDir } from '../store.js';

export interface InstallResult {
    lock: LockFile;
    fetched: Map<string, FetchResult>;
    added: string[];
    updated: string[];
    removed: string[];
}

export class Installer {
    private cwd: string;
    private storeDir: string;
    private production: boolean;
    private registry: string;
    private lock: LockFile | null = null;
    private pkgJson: PackageJson | null = null;

    constructor(options: InstallOptions = {}) {
        this.cwd = resolve(options.storeDir ? process.cwd() : process.cwd());
        this.storeDir = getStoreDir(options.storeDir);
        this.production = options.production || false;
        this.registry = options.registry || 'https://registry.npmjs.org';
    }

    async install(options: InstallOptions = {}): Promise<InstallResult> {
        await this.loadPackageJson();
        await this.loadLock(options.frozenLockfile ?? false);

        const resolution = await resolveDependencies(this.pkgJson!, this.lock, {
            ...options,
            storeDir: this.storeDir,
            production: this.production,
            registry: this.registry,
        });

        this.lock = resolution.lock;

        console.error('DEBUG: Lock packages count:', Object.keys(this.lock.packages).length);
        console.error('DEBUG: Chai in lock:', Object.keys(this.lock.packages).filter(k => k.includes('chai')));

        const fetcher = createFetcher(this.storeDir, this.registry, this.lock!);
        const fetched = await fetcher.fetchMultiple(resolution.newPackages);

        const linker = createLinker({
            cwd: this.cwd,
            storeDir: this.storeDir,
            production: this.production,
        });
        await linker.link(this.lock);

        await saveLock(this.lock, this.cwd);

        return {
            lock: this.lock,
            fetched,
            added: Array.from(resolution.newPackages.keys()),
            updated: [],
            removed: [],
        };
    }

    async add(pkgSpec: string, options: AddOptions = {}): Promise<InstallResult> {
        await this.loadPackageJson();
        await this.loadLock(false);

        const [name, version = 'latest'] = this.parsePackageSpec(pkgSpec);
        const depType = options.dev
            ? 'devDependencies'
            : options.optional
              ? 'optionalDependencies'
              : 'dependencies';

        if (!this.pkgJson![depType]) {
            this.pkgJson![depType] = {};
        }
        this.pkgJson![depType]![name] = version;

        await this.writePackageJson();

        const resolution = await resolveDependencies(this.pkgJson!, this.lock, {
            storeDir: this.storeDir,
            production: this.production,
            registry: this.registry,
        });

        this.lock = resolution.lock;

        const fetcher = createFetcher(this.storeDir, this.registry, this.lock!);
        const fetched = await fetcher.fetchMultiple(resolution.newPackages);

        const linker = createLinker({
            cwd: this.cwd,
            storeDir: this.storeDir,
            production: this.production,
        });
        await linker.link(this.lock);

        await saveLock(this.lock, this.cwd);
        await this.writePackageJson();

        return {
            lock: this.lock,
            fetched,
            added: Array.from(resolution.newPackages.keys()),
            updated: [],
            removed: [],
        };
    }

    async update(pkgSpec?: string, options: UpdateOptions = {}): Promise<InstallResult> {
        await this.loadPackageJson();
        await this.loadLock(false);

        if (pkgSpec) {
            const [name] = this.parsePackageSpec(pkgSpec);
            delete this.lock?.packages[lockKey(name, '')];
        }

        const resolution = await resolveDependencies(this.pkgJson!, this.lock, {
            storeDir: this.storeDir,
            production: this.production,
            registry: this.registry,
        });

        this.lock = resolution.lock;

        const fetcher = createFetcher(this.storeDir, this.registry, this.lock!);
        const fetched = await fetcher.fetchMultiple(resolution.newPackages);

        const linker = createLinker({
            cwd: this.cwd,
            storeDir: this.storeDir,
            production: this.production,
        });
        await linker.link(this.lock);

        await saveLock(this.lock, this.cwd);

        return {
            lock: this.lock,
            fetched,
            added: [],
            updated: Array.from(resolution.newPackages.keys()),
            removed: [],
        };
    }

    async remove(pkgName: string): Promise<InstallResult> {
        await this.loadPackageJson();
        await this.loadLock(false);

        for (const depType of [
            'dependencies',
            'devDependencies',
            'optionalDependencies',
        ] as const) {
            if (this.pkgJson![depType]?.[pkgName]) {
                delete this.pkgJson![depType]![pkgName];
            }
        }

        if (this.lock) {
            for (const key of Object.keys(this.lock.packages)) {
                const { name } = this.parseLockKey(key);
                if (name === pkgName) {
                    delete this.lock.packages[key];
                }
            }
            for (const imp of Object.values(this.lock.importers)) {
                delete imp.dependencies?.[pkgName];
                delete imp.devDependencies?.[pkgName];
                delete imp.optionalDependencies?.[pkgName];
            }
        }

        await this.writePackageJson();

        const linker = createLinker({
            cwd: this.cwd,
            storeDir: this.storeDir,
            production: this.production,
        });
        await linker.unlinkPackage(pkgName);

        if (this.lock) {
            await saveLock(this.lock, this.cwd);
        }

        return {
            lock: this.lock!,
            fetched: new Map(),
            added: [],
            updated: [],
            removed: [pkgName],
        };
    }

    async list(depth = 0): Promise<Record<string, string>> {
        await this.loadLock(true);
        if (!this.lock) return {};

        const importer = this.lock.importers['.'] || {
            dependencies: {},
            devDependencies: {},
            optionalDependencies: {},
        };
        const allDeps = {
            ...importer.dependencies,
            ...importer.devDependencies,
            ...importer.optionalDependencies,
        };

        if (depth === 0) return allDeps;

        const result: Record<string, string> = {};
        for (const [name, version] of Object.entries(allDeps)) {
            result[name] = version;
            if (depth > 1) {
                const transitive = this.getTransitiveDeps(name, version, depth - 1);
                Object.assign(result, transitive);
            }
        }
        return result;
    }

    private getTransitiveDeps(
        name: string,
        version: string,
        depth: number
    ): Record<string, string> {
        if (!this.lock || depth <= 0) return {};
        const pkgKey = lockKey(name, version);
        const pkg = this.lock.packages[pkgKey];
        if (!pkg?.dependencies) return {};

        const result: Record<string, string> = {};
        for (const [depName, depVersion] of Object.entries(pkg.dependencies)) {
            result[depName] = depVersion;
            const transitive = this.getTransitiveDeps(depName, depVersion, depth - 1);
            Object.assign(result, transitive);
        }
        return result;
    }

    async verify(): Promise<{ valid: boolean; errors: string[] }> {
        await this.loadLock(true);
        if (!this.lock) return { valid: false, errors: ['No lock file found'] };
        return verifyLockIntegrity(this.lock);
    }

    private async loadPackageJson(): Promise<void> {
        const pkgPath = resolve(this.cwd, 'package.json');
        try {
            const content = await readFile(pkgPath, 'utf-8');
            this.pkgJson = JSON.parse(content) as PackageJson;
        } catch (e: any) {
            if (e.code === 'ENOENT') {
                throw new Error('package.json not found in current directory');
            }
            throw e;
        }
    }

    private async writePackageJson(): Promise<void> {
        if (!this.pkgJson) return;
        const pkgPath = resolve(this.cwd, 'package.json');
        await writeFile(pkgPath, JSON.stringify(this.pkgJson, null, 2), 'utf-8');
    }

    private async loadLock(frozen: boolean): Promise<void> {
        this.lock = await loadLock(this.cwd);
        if (!this.lock && frozen) {
            throw new Error('Lock file not found. Run install first.');
        }
        if (!this.lock) {
            this.lock = createEmptyLock(this.storeDir);
        }
    }

    private parsePackageSpec(spec: string): [string, string] {
        if (spec.startsWith('@')) {
            const parts = spec.split('@');
            if (parts.length === 3) {
                return [`@${parts[1]}`, parts[2]];
            }
            if (parts.length === 2) {
                return [`@${parts[1]}`, 'latest'];
            }
        }
        const parts = spec.split('@');
        if (parts.length === 2) {
            return [parts[0], parts[1]];
        }
        return [spec, 'latest'];
    }

    private parseLockKey(key: string): { name: string; version: string } {
        const atIndex = key.lastIndexOf('@');
        if (atIndex === -1) {
            throw new Error(`Invalid lock key: ${key}`);
        }
        return {
            name: key.slice(0, atIndex),
            version: key.slice(atIndex + 1),
        };
    }
}

export function createInstaller(options?: InstallOptions): Installer {
    return new Installer(options);
}
